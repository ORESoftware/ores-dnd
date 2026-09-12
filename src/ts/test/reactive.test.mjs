import assert from "node:assert/strict";
import test from "node:test";

import { ORES_DND_PROTOCOL } from "../dist/index.js";
import {
  DndLifecycleTracker,
  OresDndReactiveBus,
  isHighFrequencyLifecyclePhase,
  isLosslessLifecyclePhase,
} from "../dist/reactive.js";

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
  const lossless = [];
  const dragOvers = [];

  const subscriptions = [
    bus.state$.subscribe((value) => states.push(value)),
    bus.telemetry$.subscribe((value) => telemetry.push(value)),
    bus.active$.subscribe((value) => active.push(value)),
    bus.drops$.subscribe((value) => drops.push(value)),
    bus.lossless$.subscribe((value) => lossless.push(value.phase)),
    bus.dragOvers$.subscribe((value) => dragOvers.push(value.phase)),
  ];

  bus.emit("drag-start", envelope());
  bus.emit("drag-over", envelope(), { operation: "copy", targetId: "zone-a" });
  bus.emit("drop", envelope(), { operation: "copy", targetId: "zone-a" });
  bus.emit("drag-end", envelope(), { operation: "copy", targetId: "zone-a" });

  assert.deepEqual(active, [false, true, false]);
  assert.equal(drops.length, 1);
  assert.deepEqual(lossless, ["drop", "drag-end"]);
  assert.deepEqual(dragOvers, ["drag-over"]);
  assert.equal(states.at(-1)?.phase, "drag-end");
  assert.equal(states.at(-1)?.active, false);
  assert.equal(telemetry.length, 4);
  assert.equal(telemetry[2]?.phase, "drop");
  assert.equal(telemetry[2]?.itemCount, 1);
  assert.equal(JSON.stringify(states).includes("TOP-SECRET-DRAG-DATA"), false);
  assert.equal(JSON.stringify(telemetry).includes("TOP-SECRET-DRAG-DATA"), false);
  assert.equal(isHighFrequencyLifecyclePhase("drag-over"), true);
  assert.equal(isLosslessLifecyclePhase("drop"), true);
  assert.equal(isLosslessLifecyclePhase("drag-end"), true);
  assert.equal(isLosslessLifecyclePhase("drag-over"), false);

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

test("lifecycle tracker accepts external drags beginning at drag-enter", () => {
  const bus = new OresDndReactiveBus();
  bus.emit("drag-enter", envelope("external-1"), { targetId: "zone-a" });
  bus.emit("drag-over", envelope("external-1"), { targetId: "zone-a" });
  bus.emit("drop", envelope("external-1"), { operation: "copy", targetId: "zone-a" });
  bus.emit("drag-end", envelope("external-1"), { operation: "copy", targetId: "zone-a" });
  bus.complete();
});

test("lifecycle tracker fails closed on impossible transitions and drag id switches", () => {
  const dropFirst = new OresDndReactiveBus();
  assert.throws(
    () => dropFirst.emit("drop", envelope(), { operation: "copy", targetId: "zone-a" }),
    /invalid reactive lifecycle transition/,
  );
  dropFirst.complete();

  const switched = new OresDndReactiveBus();
  switched.emit("drag-start", envelope("drag-a"));
  assert.throws(
    () => switched.emit("drag-over", envelope("drag-b")),
    /reactive dragId switched before drag-end/,
  );
  switched.complete();

  const tracker = new DndLifecycleTracker();
  assert.equal(tracker.dragId, null);
  assert.equal(tracker.phase, null);
});

test("drop events require source-allowed operation and target", () => {
  const missingOperation = new OresDndReactiveBus();
  missingOperation.emit("drag-start", envelope());
  assert.throws(
    () => missingOperation.emit("drop", envelope(), { targetId: "zone-a" }),
    /drop event requires a negotiated operation/,
  );
  missingOperation.complete();

  const missingTarget = new OresDndReactiveBus();
  missingTarget.emit("drag-start", envelope());
  assert.throws(
    () => missingTarget.emit("drop", envelope(), { operation: "copy" }),
    /drop event requires a targetId/,
  );
  missingTarget.complete();

  const disallowed = new OresDndReactiveBus();
  disallowed.emit("drag-start", envelope());
  assert.throws(
    () => disallowed.emit("drag-over", envelope(), { operation: "link" }),
    /reactive event operation is not source-allowed/,
  );
  disallowed.complete();
});
