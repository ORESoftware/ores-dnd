import { Subject, filter, type Observable } from "rxjs";

import {
  commitAcceptedDrop,
  telemetryFor,
  type DndDropResult,
  type DndEnvelope,
  type DndTelemetryEvent,
  type OresFormsPort,
  type OptoSyncPort,
  type OresOtelPort,
} from "./index.js";

export type DndEffectStage =
  | "forms"
  | "opto-local"
  | "opto-supabase"
  | "otel-local"
  | "otel-supabase";

export type DndEffectStatus = "completed" | "skipped" | "failed";

export interface DndEffectReceipt {
  readonly idempotencyKey: string;
  readonly dragId: string;
  readonly stage: DndEffectStage;
  readonly status: DndEffectStatus;
  readonly targetId?: string;
  readonly errorCode?: "effect-failed";
}

/**
 * Durable stage journal supplied by the host (normally Opto-Sync/local storage).
 * The core never decides where these markers live.
 */
export interface DndEffectJournalPort {
  hasCompleted(idempotencyKey: string, stage: DndEffectStage): boolean | Promise<boolean>;
  markCompleted(idempotencyKey: string, stage: DndEffectStage): void | Promise<void>;
}

export interface OptoSyncSupabasePort extends OptoSyncPort {
  syncAcceptedDropToSupabase(
    envelope: DndEnvelope,
    result: DndDropResult,
    idempotencyKey: string,
  ): void | Promise<void>;
}

export interface OresOtelSupabasePort extends OresOtelPort {
  syncDndEventToSupabase(
    event: DndTelemetryEvent,
    idempotencyKey: string,
  ): void | Promise<void>;
}

export interface ReactiveEffectPorts {
  readonly forms?: OresFormsPort;
  readonly optoSync?: OptoSyncSupabasePort;
  readonly otel?: OresOtelSupabasePort;
  readonly journal?: DndEffectJournalPort;
  readonly receipts?: OresDndEffectBus;
}

/** Metadata-only effect stream. Raw envelopes/provider errors never enter it. */
export class OresDndEffectBus {
  readonly #receipts = new Subject<DndEffectReceipt>();
  readonly receipts$: Observable<DndEffectReceipt> = this.#receipts.asObservable();
  readonly failures$: Observable<DndEffectReceipt> = this.receipts$.pipe(
    filter((receipt) => receipt.status === "failed"),
  );

  publish(receipt: DndEffectReceipt): void {
    this.#receipts.next(Object.freeze(receipt));
  }

  complete(): void {
    this.#receipts.complete();
  }
}

function component(value: string | undefined): string {
  return encodeURIComponent(value ?? "-");
}

/**
 * Stable logical operation key. Remote adapters MUST also use this key for
 * provider-side upsert/deduplication because a journal write can fail after an
 * external side effect succeeds.
 */
export function dndEffectKey(result: DndDropResult): string {
  return [
    "ores.dnd/v1",
    component(result.dragId),
    component(result.targetId),
    component(result.operation),
  ].join(":");
}

function receipt(
  key: string,
  result: DndDropResult,
  stage: DndEffectStage,
  status: DndEffectStatus,
): DndEffectReceipt {
  const base = { idempotencyKey: key, dragId: result.dragId, stage, status } as const;
  const withTarget = result.targetId === undefined ? base : { ...base, targetId: result.targetId };
  return status === "failed" ? { ...withTarget, errorCode: "effect-failed" } : withTarget;
}

async function runStage(
  key: string,
  result: DndDropResult,
  stage: DndEffectStage,
  ports: ReactiveEffectPorts,
  effect: () => void | Promise<void>,
): Promise<void> {
  if (await ports.journal?.hasCompleted(key, stage)) {
    ports.receipts?.publish(receipt(key, result, stage, "skipped"));
    return;
  }
  try {
    await effect();
    await ports.journal?.markCompleted(key, stage);
    ports.receipts?.publish(receipt(key, result, stage, "completed"));
  } catch (error) {
    ports.receipts?.publish(receipt(key, result, stage, "failed"));
    throw error;
  }
}

/**
 * Fail-closed accepted-drop effect pipeline.
 *
 * Validation is delegated to the canonical v1 commit path with no effects.
 * Effects then run in deterministic order with optional durable stage markers.
 * Provider diagnostics are rethrown to the caller but are never copied into
 * reactive receipts, telemetry, or replay state.
 */
export async function commitAcceptedDropEffects(
  envelope: DndEnvelope,
  result: DndDropResult,
  ports: ReactiveEffectPorts,
): Promise<void> {
  await commitAcceptedDrop(envelope, result, {});
  if (!result.accepted) return;

  const operation = result.operation;
  if (operation === undefined) throw new Error("accepted drop requires an operation");
  const key = dndEffectKey(result);

  if (ports.forms !== undefined) {
    await runStage(key, result, "forms", ports, () =>
      ports.forms!.applyAcceptedDrop(envelope, result));
  }
  if (ports.optoSync !== undefined) {
    await runStage(key, result, "opto-local", ports, () =>
      ports.optoSync!.persistAcceptedDrop(envelope, result));
    await runStage(key, result, "opto-supabase", ports, () =>
      ports.optoSync!.syncAcceptedDropToSupabase(envelope, result, key));
  }

  const event = telemetryFor("drop", envelope, operation, result.targetId);
  if (ports.otel !== undefined) {
    await runStage(key, result, "otel-local", ports, () =>
      ports.otel!.emitDndEvent(event));
    await runStage(key, result, "otel-supabase", ports, () =>
      ports.otel!.syncDndEventToSupabase(event, key));
  }
}
