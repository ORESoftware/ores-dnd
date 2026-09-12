import assert from "node:assert/strict";
import test from "node:test";

import { ORES_DND_PROTOCOL } from "../dist/index.js";
import { DndLifecycleGuard, OresDndReactiveBus } from "../dist/reactive.js";

function envelope(dragId = "drag-rx-1") {
  return {
    protocol: ORES_DND_PROTOCOL,
    dragId,
    sourceRuntime: "typescript-test",
    allowedOperations: ["copy", "move"],
    items: [{ kind: "text", mediaType: "text/plain", data: "TOP-SECRET-DRAG-DATA" }],
  };
}

test("RxJS bus keeps replayable state and telemetry payload-free", () => {
  const bus = new OresDndReactiveBus();
  const states = [];
  const telemetry = [];
  const active = [];
  const drops = [];

  const subscriptions = [
    bus.state$.subscribe((value) => states.push(value)),
    bus.telemetry$.subscribe((value) => telemetry.push(value)),
    bus.active$.subscribe((value) => active.push(value)),
    bus.drops$.subscribe((value) => drops.push(value)),
  ];

  bus.emit("drag-start", envelope());
  bus.emit("drag-over", envelope(), { operation: "copy", targetId: "zone-a" });
  bus.emit("drop", envelope(), { operation: "copy", targetId: "zone-a" });
  bus.emit("drag-end", envelope(), { operation: "copy", targetId: "zone-a" });

  assert.deepEqual(active, [false, true, false]);
  assert.equal(drops.length, 1);
  assert.equal(states.at(-1)?.phase, "drag-end");
  assert.equal(states.at(-1)?.active, false);
  assert.equal(telemetry.length, 4);
  assert.equal(telemetry[2]?.phase, "drop");
  assert.equal(telemetry[2]?.itemCount, 1);
  assert.equal(JSON.stringify(states).includes("TOP-SECRET-DRAG-DATA"), false);
  assert.equal(JSON.stringify(telemetry).includes("TOP-SECRET-DRAG-DATA"), false);

  for (const subscription of subscriptions) subscription.unsubscribe();
  bus.complete();
});

test("raw RxJS event stream is hot and does not replay dragged envelopes", () => {
  const bus = new OresDndReactiveBus();
  bus.emit("drag-start", envelope());

  const seen = [];
  const subscription = bus.events$.subscribe((event) => seen.push(event.phase));
  assert.deepEqual(seen, []);

  bus.emit("drag-over", envelope());
  assert.deepEqual(seen, ["drag-over"]);

  subscription.unsubscribe();
  bus.complete();
});

test("strict lifecycle rejects out-of-order, cross-drag and duplicate terminal phases", () => {
  const bus = new OresDndReactiveBus();
  assert.throws(() => bus.emit("drag-over", envelope()), /active drag-start/);
  bus.emit("drag-start", envelope());
  assert.throws(() => bus.emit("drag-enter", envelope("drag-other")), /dragId changed/);
  bus.emit("drop", envelope(), { operation: "copy", targetId: "zone-a" });
  assert.throws(() => bus.emit("drop", envelope(), { operation: "copy" }), /invalid after drop/);
  assert.throws(() => bus.emit("drag-over", envelope()), /invalid after drop/);
  bus.emit("drag-end", envelope());
  assert.throws(() => bus.emit("drag-end", envelope()), /active drag-start/);
  bus.complete();
});

test("reactive options reject source-disallowed operations and empty targets", () => {
  const bus = new OresDndReactiveBus();
  bus.emit("drag-start", envelope());
  assert.throws(
    () => bus.emit("drag-over", envelope(), { operation: "link" }),
    /not source-allowed/,
  );
  assert.throws(
    () => bus.emit("drag-over", envelope(), { targetId: "" }),
    /non-empty string/,
  );
  bus.emit("drag-end", envelope());
  bus.complete();
});

test("external-drop-compatible guard accepts an idle one-shot drop without activating replay state", () => {
  const guard = new DndLifecycleGuard("external-drop-compatible");
  guard.accept("drop", "external-1");
  assert.equal(guard.active, false);

  const bus = new OresDndReactiveBus({ lifecycleMode: "external-drop-compatible" });
  const states = [];
  const subscription = bus.state$.subscribe((state) => states.push(state));
  bus.emit("drop", envelope("external-1"), { operation: "copy", targetId: "zone-a" });
  assert.equal(states.at(-1)?.phase, "drop");
  assert.equal(states.at(-1)?.active, false);
  subscription.unsubscribe();
  bus.complete();
});
