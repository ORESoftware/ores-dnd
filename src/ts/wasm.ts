// Optional: run the Rust core (`ores-dnd-wasm`) instead of, or next to, the
// TypeScript core. The wasm-bindgen exports all speak JSON strings, so this is
// a thin typed shim plus cross-checks for debugging parity.
import type { DndEnvelope, DndOperation } from "./codec.js";
import { evaluatePolicy, type DndDropPolicy, type PolicyVerdict } from "./policy.js";
import { decodeSessionSnapshot } from "./corpus.js";
import { replayTrace, type DndSessionInput, type DndSessionSnapshot, type DndSessionTrace } from "./session.js";

/** The subset of `ores-dnd-wasm` exports this shim uses. */
export interface OresDndWasmExports {
  protocol_version(): string;
  mime_type(): string;
  validate_envelope_json(json: string): void;
  validate_declaration_json(declaration: string, json: string): void;
  evaluate_policy_json(envelope: string, policy: string, preferred?: string | null): string;
  replay_trace_json(trace: string): string | null | undefined;
  WasmDndSession: new () => WasmSessionHandle;
}

export interface WasmSessionHandle {
  apply(inputJson: string): string;
  snapshot(): string;
  envelope(): string | null | undefined;
  result(): string | null | undefined;
  free?(): void;
}

/** A session backed by the Rust state machine, exposing the TypeScript shape. */
export class WasmSession {
  readonly #inner: WasmSessionHandle;

  constructor(exports: OresDndWasmExports) {
    this.#inner = new exports.WasmDndSession();
  }

  apply(input: DndSessionInput): DndSessionSnapshot {
    return decodeSessionSnapshot(JSON.parse(this.#inner.apply(JSON.stringify(input))));
  }

  get snapshot(): DndSessionSnapshot {
    return decodeSessionSnapshot(JSON.parse(this.#inner.snapshot()));
  }

  get envelope(): DndEnvelope | null {
    const json = this.#inner.envelope();
    return json ? (JSON.parse(json) as DndEnvelope) : null;
  }

  dispose(): void {
    this.#inner.free?.();
  }
}

/** Evaluate a policy through the WASM core. */
export function evaluatePolicyWasm(exports: OresDndWasmExports, envelope: DndEnvelope, policy: DndDropPolicy, preferred?: DndOperation): PolicyVerdict {
  return JSON.parse(exports.evaluate_policy_json(JSON.stringify(envelope), JSON.stringify(policy), preferred ?? null)) as PolicyVerdict;
}

/** True when the TypeScript and WASM cores agree on a policy verdict. */
export function crossCheckPolicy(exports: OresDndWasmExports, envelope: DndEnvelope, policy: DndDropPolicy, preferred?: DndOperation): boolean {
  const ts = evaluatePolicy(envelope, policy, preferred);
  const wasm = evaluatePolicyWasm(exports, envelope, policy, preferred);
  return JSON.stringify(ts) === JSON.stringify(wasm);
}

/** Replay a trace through both cores; returns a description of the first disagreement or null. */
export function crossCheckTrace(exports: OresDndWasmExports, trace: DndSessionTrace): string | null {
  const ts = replayTrace(trace);
  const wasm = exports.replay_trace_json(JSON.stringify(trace)) ?? null;
  if (ts && !wasm) return `typescript diverged at step ${ts.step} of ${trace.id}`;
  if (!ts && wasm) return `wasm diverged: ${wasm}`;
  if (ts && wasm) return `both diverged: ts step ${ts.step}; wasm: ${wasm}`;
  return null;
}
