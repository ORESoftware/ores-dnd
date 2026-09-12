import { Observable, Subject, filter } from "rxjs";

import {
  commitAcceptedDrop,
  telemetryFor,
  type DndDropResult,
  type DndEnvelope,
  type DndOperation,
  type DndTelemetryEvent,
  type DropCommitPorts,
  type OptoSyncPort,
  type OresFormsPort,
  type OresOtelPort,
} from "./index.js";

export type DndSyncChannel = "opto-sync" | "ores-otel";

export interface DndAcceptedDropEvent {
  readonly kind: "accepted-drop";
  readonly dragId: string;
  readonly operation: DndOperation;
  readonly targetId?: string;
}

export interface DndLifecycleReactiveEvent {
  readonly kind: "lifecycle";
  readonly event: DndTelemetryEvent;
}

export interface DndSupabaseSyncReceipt {
  readonly kind: "supabase-sync";
  readonly dragId: string;
  readonly channel: DndSyncChannel;
  readonly backend: "supabase";
  readonly ok: boolean;
  readonly targetId?: string;
  readonly errorCode?: "sync-failed";
}

export type DndReactiveEvent =
  | DndAcceptedDropEvent
  | DndLifecycleReactiveEvent
  | DndSupabaseSyncReceipt;

/**
 * Opto-Sync adapter boundary owned by the host application.
 *
 * ores-dnd deliberately knows neither Supabase credentials nor table names.
 * The concrete adapter should route the accepted drop through opto-sync's
 * local-first queue/reconciliation path and perform the Supabase write there.
 */
export interface OptoSyncSupabasePort extends OptoSyncPort {
  syncAcceptedDropToSupabase(
    envelope: DndEnvelope,
    result: DndDropResult,
  ): void | Promise<void>;
}

/**
 * ORES-OTel adapter boundary owned by the host application.
 *
 * Only the sanitized DndTelemetryEvent crosses this boundary. Raw DndItem.data
 * is intentionally unavailable to this method.
 */
export interface OresOtelSupabasePort extends OresOtelPort {
  syncDndEventToSupabase(event: DndTelemetryEvent): void | Promise<void>;
}

export interface ReactiveDropCommitPorts {
  readonly forms?: OresFormsPort;
  readonly optoSync?: OptoSyncSupabasePort;
  readonly otel?: OresOtelSupabasePort;
  readonly reactive?: DndReactiveBus;
}

/** RxJS-backed, payload-free event surface for UI/state composition. */
export class DndReactiveBus {
  readonly #events = new Subject<DndReactiveEvent>();

  readonly events$: Observable<DndReactiveEvent> = this.#events.asObservable();
  readonly lifecycle$: Observable<DndLifecycleReactiveEvent> = this.events$.pipe(
    filter((event): event is DndLifecycleReactiveEvent => event.kind === "lifecycle"),
  );
  readonly acceptedDrops$: Observable<DndAcceptedDropEvent> = this.events$.pipe(
    filter((event): event is DndAcceptedDropEvent => event.kind === "accepted-drop"),
  );
  readonly supabaseSync$: Observable<DndSupabaseSyncReceipt> = this.events$.pipe(
    filter((event): event is DndSupabaseSyncReceipt => event.kind === "supabase-sync"),
  );

  publish(event: DndReactiveEvent): void {
    this.#events.next(event);
  }

  complete(): void {
    this.#events.complete();
  }
}

function acceptedDropEvent(result: DndDropResult, operation: DndOperation): DndAcceptedDropEvent {
  return result.targetId === undefined
    ? { kind: "accepted-drop", dragId: result.dragId, operation }
    : { kind: "accepted-drop", dragId: result.dragId, operation, targetId: result.targetId };
}

function syncReceipt(
  result: DndDropResult,
  channel: DndSyncChannel,
  ok: boolean,
): DndSupabaseSyncReceipt {
  const base = {
    kind: "supabase-sync" as const,
    dragId: result.dragId,
    channel,
    backend: "supabase" as const,
    ok,
  };
  const withTarget = result.targetId === undefined ? base : { ...base, targetId: result.targetId };
  return ok ? withTarget : { ...withTarget, errorCode: "sync-failed" as const };
}

async function syncOptoToSupabase(
  port: OptoSyncSupabasePort,
  envelope: DndEnvelope,
  result: DndDropResult,
  bus?: DndReactiveBus,
): Promise<void> {
  try {
    await port.syncAcceptedDropToSupabase(envelope, result);
    bus?.publish(syncReceipt(result, "opto-sync", true));
  } catch (error) {
    bus?.publish(syncReceipt(result, "opto-sync", false));
    throw error;
  }
}

async function syncOtelToSupabase(
  port: OresOtelSupabasePort,
  event: DndTelemetryEvent,
  result: DndDropResult,
  bus?: DndReactiveBus,
): Promise<void> {
  try {
    await port.syncDndEventToSupabase(event);
    bus?.publish(syncReceipt(result, "ores-otel", true));
  } catch (error) {
    bus?.publish(syncReceipt(result, "ores-otel", false));
    throw error;
  }
}

/**
 * Commit an accepted drop through existing local adapters, then call the
 * injected Opto-Sync and ORES-OTel Supabase sync functions.
 *
 * The core never imports a Supabase SDK and never owns credentials. It only
 * invokes explicit host-provided methods after the base ores.dnd/v1 validation
 * path succeeds. Reactive events are metadata-only and never contain item data.
 */
export async function commitAcceptedDropReactive(
  envelope: DndEnvelope,
  result: DndDropResult,
  ports: ReactiveDropCommitPorts,
): Promise<void> {
  const basePorts: DropCommitPorts = {};
  if (ports.forms !== undefined) basePorts.forms = ports.forms;
  if (ports.optoSync !== undefined) basePorts.optoSync = ports.optoSync;

  // Reuse the canonical validation + local side-effect boundary. OTel is held
  // back so the Opto-Sync Supabase write is ordered before telemetry emission.
  await commitAcceptedDrop(envelope, result, basePorts);
  if (!result.accepted) return;

  const operation = result.operation;
  if (operation === undefined) {
    throw new Error("accepted drop requires an operation");
  }

  ports.reactive?.publish(acceptedDropEvent(result, operation));

  if (ports.optoSync !== undefined) {
    await syncOptoToSupabase(ports.optoSync, envelope, result, ports.reactive);
  }

  const telemetry = telemetryFor("drop", envelope, operation, result.targetId);
  ports.reactive?.publish({ kind: "lifecycle", event: telemetry });

  if (ports.otel !== undefined) {
    await ports.otel.emitDndEvent(telemetry);
    await syncOtelToSupabase(ports.otel, telemetry, result, ports.reactive);
  }
}
