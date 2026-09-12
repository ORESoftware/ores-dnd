import test from "node:test";
import assert from "node:assert/strict";
import { DndSession, inputs, replayTrace, resultOf } from "../dist/session.js";
import { traces, validEnvelope } from "./helpers.mjs";

test("every shared DndSessionTrace replays identically", async () => {
  const all = await traces();
  assert.ok(all.length >= 20, `expected the full corpus, got ${all.length}`);
  const divergences = all.map(replayTrace).filter(Boolean);
  assert.deepEqual(divergences, []);
});

test("trace replay detects a divergence", async () => {
  const [trace] = (await traces()).filter((t) => t.id === "basic-drop");
  trace.expected[2].operation = "link";
  const divergence = replayTrace(trace);
  assert.equal(divergence?.step, 2);
});

test("session exposes envelope, result and listeners", async () => {
  const envelope = await validEnvelope();
  const session = new DndSession();
  const seen = [];
  const unsubscribe = session.subscribe((snapshot, input) => seen.push(`${input.kind}:${snapshot.state}`));
  assert.equal(session.result, null);
  session.apply(inputs.start(envelope));
  assert.deepEqual(session.envelope, envelope);
  session.apply(inputs.enter({ targetId: "zone-a", allowedOperations: ["move"], acceptedKinds: ["text"] }));
  session.apply(inputs.drop("zone-a"));
  assert.deepEqual(session.result, { dragId: envelope.dragId, accepted: true, operation: "move", targetId: "zone-a" });
  unsubscribe();
  session.apply(inputs.cancel());
  assert.deepEqual(seen, ["start:dragging", "enter:over-target", "drop:dropped"]);
  assert.deepEqual(resultOf({ state: "cancelled", dragId: "d", errorCode: "cancelled" }), { dragId: "d", accepted: false, errorCode: "cancelled" });
  assert.equal(resultOf({ state: "dragging", dragId: "d" }), null);
});

test("malformed inputs are ignored", async () => {
  const envelope = await validEnvelope();
  const session = new DndSession();
  session.apply(inputs.start(envelope));
  const before = session.snapshot;
  assert.deepEqual(session.apply({ kind: "enter", targetId: "z" }), before);
  assert.deepEqual(session.apply({ kind: "leave" }), before);
  assert.deepEqual(session.apply({ kind: "enter", targetId: "z", policy: { targetId: "z", allowedOperations: ["copy"], acceptedKinds: ["text"], maxItems: 0 } }), before);
});
