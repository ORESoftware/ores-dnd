// The drag session state machine — a pure apply(snapshot, input) → snapshot
// specified by docs/DESIGN.md §Session and proven by the shared trace corpus.
import { validateEnvelope, type DndDropResult, type DndEnvelope, type DndOperation, type ValidationOptions } from "./codec.js";
import { evaluatePolicy, validatePolicy, type DndDropPolicy, type DndRejectCode } from "./policy.js";

export type DndSessionState = "idle" | "dragging" | "over-target" | "dropped" | "cancelled";
export type DndSessionInputKind = "start" | "enter" | "leave" | "drop" | "cancel" | "end";

export interface DndSessionInput {
  kind: DndSessionInputKind;
  /** Required for `start`. */
  envelope?: DndEnvelope;
  /** Required for `enter`, `leave` and `drop`. */
  targetId?: string;
  /** Required for `enter`. */
  policy?: DndDropPolicy;
  /** Optional for `enter`. */
  preferredOperation?: DndOperation;
}

export interface DndSessionSnapshot {
  state: DndSessionState;
  dragId?: string;
  targetId?: string;
  operation?: DndOperation;
  errorCode?: DndRejectCode;
}

export interface DndSessionTrace {
  id: string;
  description?: string;
  inputs: DndSessionInput[];
  expected: DndSessionSnapshot[];
}

export const IDLE_SNAPSHOT: Readonly<DndSessionSnapshot> = Object.freeze({ state: "idle" });

export function isTerminal(state: DndSessionState): boolean {
  return state === "dropped" || state === "cancelled";
}

/** Input constructors. */
export const inputs = {
  start: (envelope: DndEnvelope): DndSessionInput => ({ kind: "start", envelope }),
  enter: (policy: DndDropPolicy, preferredOperation?: DndOperation): DndSessionInput =>
    preferredOperation ? { kind: "enter", targetId: policy.targetId, policy, preferredOperation } : { kind: "enter", targetId: policy.targetId, policy },
  leave: (targetId: string): DndSessionInput => ({ kind: "leave", targetId }),
  drop: (targetId: string): DndSessionInput => ({ kind: "drop", targetId }),
  cancel: (): DndSessionInput => ({ kind: "cancel" }),
  end: (): DndSessionInput => ({ kind: "end" }),
} as const;

interface SnapshotFields {
  dragId?: string | undefined;
  targetId?: string | undefined;
  operation?: DndOperation | undefined;
  errorCode?: DndRejectCode | undefined;
}

function snapshot(state: DndSessionState, fields: SnapshotFields = {}): DndSessionSnapshot {
  const out: DndSessionSnapshot = { state };
  if (fields.dragId !== undefined) out.dragId = fields.dragId;
  if (fields.targetId !== undefined) out.targetId = fields.targetId;
  if (fields.operation !== undefined) out.operation = fields.operation;
  if (fields.errorCode !== undefined) out.errorCode = fields.errorCode;
  return out;
}

/** The terminal DndDropResult for a snapshot, or null while the session runs. */
export function resultOf(s: DndSessionSnapshot): DndDropResult | null {
  if (s.dragId === undefined) return null;
  if (s.state === "dropped") {
    const result: DndDropResult = { dragId: s.dragId, accepted: true };
    if (s.operation !== undefined) result.operation = s.operation;
    if (s.targetId !== undefined) result.targetId = s.targetId;
    return result;
  }
  if (s.state === "cancelled") {
    const result: DndDropResult = { dragId: s.dragId, accepted: false };
    if (s.targetId !== undefined) result.targetId = s.targetId;
    if (s.errorCode !== undefined) result.errorCode = s.errorCode;
    return result;
  }
  return null;
}

export type SessionListener = (snapshot: DndSessionSnapshot, input: DndSessionInput) => void;

/** One drag session. Hosts keep one per drag source (or one global one). */
export class DndSession {
  #snapshot: DndSessionSnapshot = { state: "idle" };
  #envelope: DndEnvelope | null = null;
  readonly #options: ValidationOptions;
  readonly #listeners = new Set<SessionListener>();

  constructor(options: ValidationOptions = {}) {
    this.#options = options;
  }

  get snapshot(): DndSessionSnapshot {
    return this.#snapshot;
  }

  /** The envelope of the running (or just finished) session. */
  get envelope(): DndEnvelope | null {
    return this.#envelope;
  }

  get result(): DndDropResult | null {
    return resultOf(this.#snapshot);
  }

  subscribe(listener: SessionListener): () => void {
    this.#listeners.add(listener);
    return () => this.#listeners.delete(listener);
  }

  /** Apply one input and return the new snapshot. Malformed inputs are ignored. */
  apply(input: DndSessionInput): DndSessionSnapshot {
    this.#snapshot = this.#next(input);
    for (const listener of this.#listeners) listener(this.#snapshot, input);
    return this.#snapshot;
  }

  #next(input: DndSessionInput): DndSessionSnapshot {
    const current = this.#snapshot;
    if (input.kind === "start") {
      let envelope: DndEnvelope | null = null;
      try {
        envelope = input.envelope === undefined ? null : validateEnvelope(input.envelope, this.#options);
      } catch {
        envelope = null;
      }
      this.#envelope = envelope;
      return envelope ? snapshot("dragging", { dragId: envelope.dragId }) : snapshot("idle", { errorCode: "invalid-envelope" });
    }
    if (current.state === "idle" || isTerminal(current.state)) return current;
    switch (input.kind) {
      case "enter": {
        if (!input.policy || !this.#envelope) return current;
        let policy: DndDropPolicy;
        try {
          policy = validatePolicy(input.policy);
        } catch {
          return current;
        }
        const targetId = input.targetId ?? policy.targetId;
        const verdict = evaluatePolicy(this.#envelope, policy, input.preferredOperation);
        return verdict.accepted
          ? snapshot("over-target", { dragId: current.dragId, targetId, operation: verdict.operation })
          : snapshot("dragging", { dragId: current.dragId, targetId, errorCode: verdict.errorCode });
      }
      case "leave":
        return input.targetId !== undefined && current.targetId === input.targetId ? snapshot("dragging", { dragId: current.dragId }) : current;
      case "drop": {
        const cancelled = (errorCode: DndRejectCode): DndSessionSnapshot =>
          snapshot("cancelled", { dragId: current.dragId, targetId: input.targetId, errorCode });
        if (current.targetId === undefined) return cancelled("no-active-target");
        if (input.targetId !== current.targetId) return cancelled("target-mismatch");
        if (current.state === "over-target") return snapshot("dropped", current);
        return cancelled(current.errorCode ?? "no-active-target");
      }
      case "cancel":
      case "end":
        return snapshot("cancelled", { dragId: current.dragId, errorCode: "cancelled" });
      default:
        return current;
    }
  }
}

export interface TraceDivergence {
  traceId: string;
  step: number;
  expected?: DndSessionSnapshot;
  actual?: DndSessionSnapshot;
}

function sameSnapshot(a: DndSessionSnapshot, b: DndSessionSnapshot): boolean {
  return a.state === b.state && a.dragId === b.dragId && a.targetId === b.targetId && a.operation === b.operation && a.errorCode === b.errorCode;
}

/** Replay a trace from the idle state; returns the first divergence or null. */
export function replayTrace(trace: DndSessionTrace): TraceDivergence | null {
  if (trace.inputs.length !== trace.expected.length) {
    return { traceId: trace.id, step: Math.min(trace.inputs.length, trace.expected.length) };
  }
  const session = new DndSession();
  for (let step = 0; step < trace.inputs.length; step += 1) {
    const actual = session.apply(trace.inputs[step]!);
    const expected = trace.expected[step]!;
    if (!sameSnapshot(actual, expected)) return { traceId: trace.id, step, expected, actual };
  }
  return null;
}
