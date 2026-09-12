// Runs against the REAL compiled module (wasm-pack --target nodejs → ./pkg).
// Three questions: does the WASM build decode the corpus exactly as declared,
// does it replay every trace, and does it agree with the TypeScript core on
// thousands of random policy evaluations and sessions?
import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

const require = createRequire(import.meta.url);
const wasm = require("../pkg/ores_dnd_wasm.js");
const ts = await import("../../ts/dist/index.js");

const CONTRACTS = new URL("../../../contracts/", import.meta.url).pathname;

async function corpus() {
  const out = [];
  const root = join(CONTRACTS, "instances");
  for (const declaration of (await readdir(root)).sort()) {
    for (const [lane, expectation] of [["valid", "accepted"], ["invalid", "rejected"]]) {
      let files = [];
      try { files = (await readdir(join(root, declaration, lane))).filter((f) => f.endsWith(".json")).sort(); } catch { continue; }
      for (const file of files) out.push({ declaration, expectation, file, json: await readFile(join(root, declaration, lane, file), "utf8") });
    }
  }
  return out;
}

test("compiled module identifies the protocol", () => {
  assert.equal(wasm.protocol_version(), ts.ORES_DND_PROTOCOL);
  assert.equal(wasm.mime_type(), ts.ORES_DND_MIME);
});

test("compiled module gives every corpus instance the declared verdict", async () => {
  const all = await corpus();
  assert.ok(all.length >= 100);
  const failures = [];
  for (const { declaration, expectation, file, json } of all) {
    let verdict = "accepted";
    try { wasm.validate_declaration_json(declaration, json); } catch { verdict = "rejected"; }
    if (verdict !== expectation) failures.push(`${declaration}/${expectation}/${file}: wasm said ${verdict}`);
  }
  assert.deepEqual(failures, []);
});

test("compiled module replays every trace and agrees with the TypeScript core", async () => {
  const traces = (await corpus()).filter((c) => c.declaration === "DndSessionTrace" && c.expectation === "accepted").map((c) => JSON.parse(c.json));
  assert.ok(traces.length >= 60);
  for (const trace of traces) {
    assert.equal(wasm.replay_trace_json(JSON.stringify(trace)) ?? null, null, trace.id);
    assert.equal(ts.crossCheckTrace(wasm, trace), null, trace.id);
  }
});

test("WasmDndSession drives the state machine over the JSON ABI", async () => {
  const [trace] = (await corpus()).filter((c) => c.file === "basic-drop.json").map((c) => JSON.parse(c.json));
  const session = new ts.WasmSession(wasm);
  trace.inputs.forEach((input, i) => assert.deepEqual(session.apply(input), trace.expected[i], `step ${i}`));
  assert.equal(session.envelope.dragId, "drag-0001");
  session.dispose();
});

test("WASM and TypeScript cores agree on random policy evaluations and sessions", () => {
  const rng = new ts.Xorshift64(0xbeef);
  let evaluated = 0;
  for (let i = 0; i < 2000; i += 1) {
    const envelope = ts.randomEnvelope(rng, i + 1);
    envelope.protocol = ts.ORES_DND_PROTOCOL;
    const policy = ts.randomPolicy(rng);
    const preferred = rng.chance(30) ? rng.pick(ts.OPS) : undefined;
    assert.equal(ts.crossCheckPolicy(wasm, envelope, policy, preferred), true, JSON.stringify({ envelope, policy, preferred }));
    evaluated += 1;
  }
  assert.equal(evaluated, 2000);
  for (let run = 0; run < 300; run += 1) {
    const seed = rng.nextU64();
    const inputs = ts.randomSequence(seed, 20);
    const a = new ts.DndSession();
    const b = new ts.WasmSession(wasm);
    inputs.forEach((input, step) => assert.deepEqual(b.apply(input), a.apply(input), `seed ${seed.toString(16)} step ${step}`));
    b.dispose();
  }
});
