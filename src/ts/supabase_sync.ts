import { Subject, filter, type Observable } from "rxjs";

import {
  commitAcceptedDrop,
  telemetryFor,
  type DndDropResult,
  type DndEnvelope,
  type DndTelemetryEvent,
  type DropCommitPorts,
  type OptoSyncPort,
  type OresFormsPort,
  type OresOtelPort,
} from "./index.js";
import { OresDndReactiveBus } from "./reactive.js";

export type DndSyncChannel = "opto-sync" | "ores-otel";

export interface DndSupabaseSyncReceipt {
  readonly dragId: string;
  readonly channel: DndSyncChannel;
  readonly backend: "supabase";
  readonly ok: boolean;
  readonly targetId?: string;
  readonly errorCode?: "sync-failed";
}

/** Host adapter: Opto-Sync remains the local-first/state synchronization owner. */
export interface OptoSyncSupabasePort extends OptoSyncPort {
  syncAcceptedDropToSupabase(
    envelope: DndEnvelope,
    result: DndDropResult,
  ): void | Promise<void>;
}

/** Host adapter: only sanitized ORES-OTel metadata crosses this boundary. */
export interface OresOtelSupabasePort extends OresOtelPort {
  syncDndEventToSupabase(event: DndTelemetryEvent): void | Promise<void>;
}

export interface SupabaseDropCommitPorts {
  readonly forms?: OresFormsPort;
  readonly optoSync?: OptoSyncSupabasePort;
  readonly otel?: OresOtelSupabasePort;
  /** Canonical RxJS lifecycle bus from `@oresoftware/ores-dnd/reactive`. */
  readonly lifecycle?: OresDndReactiveBus;
  /** Payload-free Supabase synchronization receipts. */
  readonly sync?: OresDndSupabaseSyncBus;
}

/** Hot, non-replaying RxJS stream of payload-free synchronization receipts. */
export class OresDndSupabaseSyncBus {
  readonly #receipts = new Subject<DndSupabaseSyncReceipt>();

  readonly receipts$: Observable<DndSupabaseSyncReceipt> = this.#receipts.asObservable();
  readonly failures$: Observable<DndSupabaseSyncReceipt> = this.receipts$.pipe(
    filter((receipt) => !receipt.ok),
  );

  publish(receipt: DndSupabaseSyncReceipt): void {
    this.#receipts.next(receipt);
  }

  complete(): void {
    this.#receipts.complete();
  }
}

function receipt(
  result: DndDropResult,
  channel: DndSyncChannel,
  ok: boolean,
): DndSupabaseSyncReceipt {
  const base = {
    dragId: result.dragId,
    channel,
    backend: "supabase" as const,
    ok,
  };
  const targeted = result.targetId === undefined ? base : { ...base, targetId: result.targetId };
  return ok ? targeted : { ...targeted, errorCode: "sync-failed" as const };
}

async function syncOpto(
  port: OptoSyncSupabasePort,
  envelope: DndEnvelope,
  result: DndDropResult,
  bus?: OresDndSupabaseSyncBus,
): Promise<void> {
  try {
    await port.syncAcceptedDropToSupabase(envelope, result);
    bus?.publish(receipt(result, "opto-sync", true));
  } catch (error) {
    bus?.publish(receipt(result, "opto-sync", false));
    throw error;
  }
}

async function syncOtel(
  port: OresOtelSupabasePort,
  event: DndTelemetryEvent,
  result: DndDropResult,
  bus?: OresDndSupabaseSyncBus,
): Promise<void> {
  try {
    await port.syncDndEventToSupabase(event);
    bus?.publish(receipt(result, "ores-otel", true));
  } catch (error) {
    bus?.publish(receipt(result, "ores-otel", false));
    throw error;
  }
}

/**
 * Canonical accepted-drop effects plus explicit Opto-Sync and ORES-OTel
 * Supabase calls. ores-dnd owns no Supabase SDK, endpoint, table, or credential.
 *
 * Order is fail-closed:
 * forms -> Opto-Sync local -> Opto-Sync Supabase -> reactive drop/telemetry ->
 * ORES-OTel local -> ORES-OTel Supabase.
 */
export async function commitAcceptedDropWithSupabase(
  envelope: DndEnvelope,
  result: DndDropResult,
  ports: SupabaseDropCommitPorts,
): Promise<void> {
  const basePorts: DropCommitPorts = {};
  if (ports.forms !== undefined) basePorts.forms = ports.forms;
  if (ports.optoSync !== undefined) basePorts.optoSync = ports.optoSync;

  // Reuse canonical validation/local effects but hold OTel until Opto-Sync's
  // Supabase write has succeeded.
  await commitAcceptedDrop(envelope, result, basePorts);
  if (!result.accepted) return;

  const operation = result.operation;
  if (operation === undefined) {
    throw new Error("accepted drop requires an operation");
  }

  if (ports.optoSync !== undefined) {
    await syncOpto(ports.optoSync, envelope, result, ports.sync);
  }

  ports.lifecycle?.emit("drop", envelope, {
    operation,
    ...(result.targetId === undefined ? {} : { targetId: result.targetId }),
  });

  const telemetry = telemetryFor("drop", envelope, operation, result.targetId);
  if (ports.otel !== undefined) {
    await ports.otel.emitDndEvent(telemetry);
    await syncOtel(ports.otel, telemetry, result, ports.sync);
  }
}
