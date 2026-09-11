import test from "node:test";
import assert from "node:assert/strict";
import { DndSession } from "../dist/session.js";
import { ATTR_STATE, EVENT_DROP } from "../dist/dom.js";
import { ZoneRegistry, bindPointerDragSource } from "../dist/pointer.js";
import { FakeDocument, otelRecorder, pointerEvent } from "./fake-dom.mjs";
import { validEnvelope } from "./helpers.mjs";

function setup(doc) {
  const registry = new ZoneRegistry();
  const zoneA = doc.body.appendChild(doc.createElement("div"));
  const inner = zoneA.appendChild(doc.createElement("span"));
  const zoneJson = doc.body.appendChild(doc.createElement("div"));
  registry.register(zoneA, { targetId: "zone-a", allowedOperations: ["copy", "move"], acceptedKinds: ["text"] });
  registry.register(zoneJson, { targetId: "zone-json", allowedOperations: ["copy"], acceptedKinds: ["json"] });
  doc.place(zoneA, 100, 0, 200, 100);
  doc.place(inner, 150, 50, 160, 60); // innermost element inside zone-a
  doc.place(zoneJson, 300, 0, 400, 100);
  return { registry, zoneA, zoneJson, inner };
}

test("pointer drag: threshold, hit-testing, drop", async () => {
  const doc = new FakeDocument();
  const { registry, zoneA, zoneJson } = setup(doc);
  const envelope = await validEnvelope();
  const session = new DndSession();
  const otel = otelRecorder();
  const source = doc.body.appendChild(doc.createElement("div"));
  const drops = [];
  zoneA.addEventListener(EVENT_DROP, (e) => drops.push(`event:${e.detail.result.targetId}`));
  bindPointerDragSource(source, envelope, session, registry, { doc, otel, onDrop: (_env, result, zone) => drops.push(`handler:${zone.policy.targetId}:${result.operation}`) });
  assert.equal(source.style.touchAction, "none");

  source.dispatchEvent(pointerEvent("pointerdown", { clientX: 10, clientY: 10 }));
  source.dispatchEvent(pointerEvent("pointermove", { clientX: 14, clientY: 10 }));
  assert.equal(session.snapshot.state, "idle", "below the threshold nothing starts");
  source.dispatchEvent(pointerEvent("pointermove", { clientX: 30, clientY: 10 }));
  assert.equal(session.snapshot.state, "dragging");
  assert.equal(source.getAttribute("data-ores-dnd-dragging"), "true");

  source.dispatchEvent(pointerEvent("pointermove", { clientX: 350, clientY: 50 }));
  assert.equal(session.snapshot.state, "dragging");
  assert.equal(session.snapshot.errorCode, "item-kind-not-accepted");
  assert.equal(zoneJson.getAttribute(ATTR_STATE), "rejecting");

  source.dispatchEvent(pointerEvent("pointermove", { clientX: 155, clientY: 55, shiftKey: true }));
  assert.equal(session.snapshot.state, "over-target");
  assert.equal(session.snapshot.targetId, "zone-a", "the innermost element resolves to its registered zone");
  assert.equal(session.snapshot.operation, "move");
  assert.equal(zoneA.getAttribute(ATTR_STATE), "accepting");
  assert.equal(zoneJson.getAttribute(ATTR_STATE), "dragging");

  source.dispatchEvent(pointerEvent("pointerup", { clientX: 155, clientY: 55 }));
  assert.equal(session.snapshot.state, "dropped");
  assert.deepEqual(drops, ["event:zone-a", "handler:zone-a:move"]);
  assert.equal(source.hasAttribute("data-ores-dnd-dragging"), false);
  assert.equal(zoneA.getAttribute(ATTR_STATE), "dropped");
  // telemetry only reports accepting targets: the rejecting hover over zone-json is not an enter/leave pair
  assert.deepEqual(otel.events.map((e) => e.phase), ["drag-start", "drag-enter", "drop"]);
  assert.equal(JSON.stringify(otel.events).includes("hello"), false);
});

test("pointer drag released outside any zone ends the session; pointercancel cancels", async () => {
  const doc = new FakeDocument();
  const { registry } = setup(doc);
  const envelope = await validEnvelope();
  const session = new DndSession();
  const source = doc.body.appendChild(doc.createElement("div"));
  const rejected = [];
  bindPointerDragSource(source, envelope, session, registry, { doc, threshold: 2, onReject: (r) => rejected.push(r.errorCode) });
  source.dispatchEvent(pointerEvent("pointerdown", { clientX: 0, clientY: 0 }));
  source.dispatchEvent(pointerEvent("pointermove", { clientX: 5, clientY: 5 }));
  source.dispatchEvent(pointerEvent("pointerup", { clientX: 5, clientY: 5 }));
  assert.equal(session.snapshot.state, "cancelled");
  assert.deepEqual(rejected, ["cancelled"]);

  source.dispatchEvent(pointerEvent("pointerdown", { clientX: 0, clientY: 0 }));
  source.dispatchEvent(pointerEvent("pointermove", { clientX: 150, clientY: 50 }));
  assert.equal(session.snapshot.state, "over-target");
  source.dispatchEvent(pointerEvent("pointercancel"));
  assert.equal(session.snapshot.state, "cancelled");
  // a second pointer is ignored while the first is active
  source.dispatchEvent(pointerEvent("pointerdown", { pointerId: 1, clientX: 0, clientY: 0 }));
  source.dispatchEvent(pointerEvent("pointermove", { pointerId: 2, clientX: 150, clientY: 50 }));
  assert.equal(session.snapshot.state, "cancelled");
});

test("registry validates policies and unregisters", () => {
  const doc = new FakeDocument();
  const registry = new ZoneRegistry();
  const el = doc.createElement("div");
  assert.throws(() => registry.register(el, { targetId: "", allowedOperations: [], acceptedKinds: [] }), /targetId/);
  const unregister = registry.register(el, { targetId: "z", allowedOperations: ["copy"], acceptedKinds: ["text"] });
  assert.equal(registry.zoneFor(el)?.policy.targetId, "z");
  unregister();
  assert.equal(registry.zoneFor(el), null);
});
