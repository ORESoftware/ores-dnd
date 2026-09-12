import test from "node:test";
import assert from "node:assert/strict";
import { DndSession, replayTrace } from "../dist/session.js";
import { evaluatePolicy } from "../dist/policy.js";
import { WasmSession, crossCheckPolicy, crossCheckTrace, evaluatePolicyWasm } from "../dist/wasm.js";
import { traces, validEnvelope } from "./helpers.mjs";

/** A stand-in for the wasm-bindgen glue, implemented over the TypeScript core with the same JSON ABI. */
function fakeExports({ lie = false } = {}) {
  return {
    protocol_version: () => "ores.dnd/v1",
    mime_type: () => "application/vnd.ores.dnd+json",
    validate_envelope_json: () => {},
    validate_declaration_json: () => {},
    evaluate_policy_json: (env, policy, preferred) => {
      const verdict = evaluatePolicy(JSON.parse(env), JSON.parse(policy), preferred ?? undefined);
      return JSON.stringify(lie && verdict.accepted ? { accepted: false, errorCode: "cancelled" } : verdict);
    },
    replay_trace_json: (trace) => {
      const d = replayTrace(JSON.parse(trace));
      return lie ? "wasm says no" : d ? `step ${d.step}` : null;
    },
    WasmDndSession: class {
      #s = new DndSession();
      apply(input) { return JSON.stringify(this.#s.apply(JSON.parse(input))); }
      snapshot() { return JSON.stringify(this.#s.snapshot); }
      envelope() { return this.#s.envelope ? JSON.stringify(this.#s.envelope) : null; }
      result() { return this.#s.result ? JSON.stringify(this.#s.result) : null; }
      free() {}
    },
  };
}

test("WasmSession speaks the JSON ABI", async () => {
  const envelope = await validEnvelope();
  const session = new WasmSession(fakeExports());
  assert.deepEqual(session.snapshot, { state: "idle" });
  session.apply({ kind: "start", envelope });
  assert.deepEqual(session.envelope, envelope);
  const over = session.apply({ kind: "enter", targetId: "zone-a", policy: { targetId: "zone-a", allowedOperations: ["copy"], acceptedKinds: ["text"] } });
  assert.equal(over.operation, "copy");
  session.dispose();
});

test("cross-checks agree with an honest core and catch a divergent one", async () => {
  const envelope = await validEnvelope();
  const policy = { targetId: "zone-a", allowedOperations: ["copy"], acceptedKinds: ["text"] };
  assert.equal(crossCheckPolicy(fakeExports(), envelope, policy), true);
  assert.equal(crossCheckPolicy(fakeExports({ lie: true }), envelope, policy), false);
  assert.deepEqual(evaluatePolicyWasm(fakeExports(), envelope, policy, "copy"), { accepted: true, operation: "copy" });
  for (const trace of await traces()) assert.equal(crossCheckTrace(fakeExports(), trace), null, trace.id);
  assert.match(crossCheckTrace(fakeExports({ lie: true }), (await traces())[0]), /wasm diverged/);
});
