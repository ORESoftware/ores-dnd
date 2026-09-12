import test from "node:test";
import assert from "node:assert/strict";
import { DndSession, isTerminal } from "../dist/session.js";
import { Xorshift64, randomSequence } from "../dist/fuzz.js";
import { decodeSessionSnapshot } from "../dist/corpus.js";

test("xorshift64* reproduces the Rust reference values", () => {
  const rng = new Xorshift64(7);
  assert.deepEqual([rng.nextU64(), rng.nextU64(), rng.nextU64()], [0xd1fbaf7f728d2eaen, 0xeda46c77629da6aen, 0x16df9d6ac76bd322n]);
  const b = new Xorshift64(7);
  assert.deepEqual([b.below(100), b.below(100), b.below(100)], [7, 63, 78]);
  assert.deepEqual(randomSequence(42, 10), randomSequence(42, 10));
  assert.notDeepEqual(randomSequence(42, 10), randomSequence(43, 10));
});

test("session invariants hold over random sequences", () => {
  const seeds = new Xorshift64(0xc0ffee00);
  for (let run = 0; run < 1500; run += 1) {
    const seed = seeds.nextU64();
    const session = new DndSession();
    let previous = { state: "idle" };
    randomSequence(seed, 24).forEach((input, step) => {
      const snapshot = session.apply(input);
      const ctx = `seed ${seed.toString(16)} step ${step} ${input.kind} -> ${JSON.stringify(snapshot)}`;
      // 1. structurally valid and JSON round-trippable
      assert.deepEqual(decodeSessionSnapshot(JSON.parse(JSON.stringify(snapshot))), snapshot, ctx);
      // 2. terminal states absorb everything but start
      if (isTerminal(previous.state) && input.kind !== "start") assert.deepEqual(snapshot, previous, ctx);
      switch (snapshot.state) {
        case "idle":
          assert.ok(snapshot.dragId === undefined && snapshot.targetId === undefined && snapshot.operation === undefined, ctx);
          assert.equal(session.envelope, null, ctx);
          break;
        case "dragging":
          assert.ok(snapshot.dragId !== undefined && snapshot.operation === undefined, ctx);
          assert.equal(snapshot.targetId !== undefined, snapshot.errorCode !== undefined, ctx); // 3. rejecting target ⇔ reason
          break;
        case "over-target":
          assert.ok(session.envelope.allowedOperations.includes(snapshot.operation), ctx); // 4. source-allowed operation
          assert.ok(snapshot.targetId !== undefined && snapshot.errorCode === undefined, ctx);
          if (input.kind === "enter" && JSON.stringify(snapshot) !== JSON.stringify(previous)) {
            assert.ok(input.policy.allowedOperations.includes(snapshot.operation), ctx);
            assert.equal(snapshot.targetId, input.policy.targetId, ctx);
          }
          break;
        case "dropped":
          assert.ok(snapshot.operation !== undefined && snapshot.targetId !== undefined && snapshot.errorCode === undefined, ctx);
          assert.equal(session.result.accepted, true, ctx);
          if (JSON.stringify(snapshot) !== JSON.stringify(previous)) { // 5. drop lands only on the accepting target
            assert.equal(input.kind, "drop", ctx);
            assert.equal(previous.state, "over-target", ctx);
            assert.equal(previous.targetId, snapshot.targetId, ctx);
            assert.equal(input.targetId, snapshot.targetId, ctx);
          }
          break;
        case "cancelled":
          assert.ok(snapshot.errorCode !== undefined && snapshot.operation === undefined, ctx);
          assert.equal(session.result.accepted, false, ctx);
          assert.equal(session.result.errorCode, snapshot.errorCode, ctx);
          break;
        default:
          assert.fail(ctx);
      }
      // 6. dragId only changes on start
      if (input.kind !== "start" && !isTerminal(previous.state) && previous.state !== "idle") assert.equal(snapshot.dragId, previous.dragId, ctx);
      previous = snapshot;
    });
  }
});

test("the TypeScript generator reproduces every Rust-generated fuzz trace input for input", async () => {
  const { traces } = await import("./helpers.mjs");
  const fuzz = (await traces()).filter((t) => t.id.startsWith("fuzz-"));
  assert.ok(fuzz.length >= 40, `expected the fuzz corpus, got ${fuzz.length}`);
  for (const trace of fuzz) {
    const seed = BigInt(`0x${trace.id.slice("fuzz-".length)}`);
    assert.deepEqual(randomSequence(seed, trace.inputs.length), trace.inputs, trace.id);
  }
});
