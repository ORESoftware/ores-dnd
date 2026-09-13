import test from "node:test";
import assert from "node:assert/strict";
import {
  KeyboardDndController,
  bindKeyboardDragSource,
  keyboardPreferredOperation,
} from "../dist/keyboard.js";
import { DndSession } from "../dist/session.js";
import { FakeDocument, otelRecorder } from "./fake-dom.mjs";
import { validEnvelope } from "./helpers.mjs";

const textTarget = {
  policy: {
    targetId: "text-zone",
    allowedOperations: ["copy", "move"],
    acceptedKinds: ["text"],
  },
};
const jsonTarget = {
  policy: {
    targetId: "json-zone",
    allowedOperations: ["copy"],
    acceptedKinds: ["json"],
  },
};

function keyEvent(key, keys = {}) {
  const event = new Event("keydown", { bubbles: true, cancelable: true });
  Object.assign(event, { key, ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...keys });
  return event;
}

test("keyboard controller reuses DndSession policy order and wraps target navigation", async () => {
  const envelope = await validEnvelope();
  const session = new DndSession();
  const announcements = [];
  const targets = [];
  const dropped = [];
  const controller = new KeyboardDndController([textTarget, jsonTarget], session, {
    announce: (announcement) => announcements.push(announcement),
    onTargetChange: (targetId) => targets.push(targetId),
    onDrop: (_env, result) => dropped.push(result),
  });

  assert.equal(controller.start(envelope).state, "dragging");
  assert.equal(controller.move(1).state, "over-target");
  assert.equal(session.snapshot.targetId, "text-zone");
  assert.equal(session.snapshot.operation, "move");

  assert.equal(controller.move(1).state, "dragging", "json-only target rejects a text envelope");
  assert.equal(session.snapshot.targetId, "json-zone");
  assert.equal(session.snapshot.errorCode, "item-kind-not-accepted");

  assert.equal(controller.move(1, "copy").state, "over-target", "navigation wraps to the first target");
  assert.equal(session.snapshot.operation, "copy");
  const result = controller.drop();
  assert.equal(result.accepted, true);
  assert.equal(result.targetId, "text-zone");
  assert.equal(result.operation, "copy");
  assert.equal(dropped.length, 1);
  assert.deepEqual(targets, ["text-zone", "json-zone", "text-zone"]);
  assert.deepEqual(announcements.map((a) => a.kind), [
    "started",
    "target-accepted",
    "target-rejected",
    "target-accepted",
    "dropped",
  ]);
  assert.equal(JSON.stringify(announcements).includes("hello"), false, "announcements must never contain dragged data");
});

test("Escape cancels and emits only content-free telemetry", async () => {
  const envelope = await validEnvelope();
  const otel = otelRecorder();
  const rejected = [];
  const controller = new KeyboardDndController([textTarget], new DndSession(), {
    otel,
    onReject: (_env, result) => rejected.push(result),
  });
  controller.start(envelope);
  controller.move(1);
  const result = controller.cancel();
  assert.equal(result.accepted, false);
  assert.equal(result.errorCode, "cancelled");
  assert.equal(rejected.length, 1);
  assert.deepEqual(otel.events.map((event) => event.phase), ["drag-start", "drag-enter", "drag-end"]);
  assert.equal(JSON.stringify(otel.events).includes("hello"), false);
});

test("DOM keyboard binder uses Space/Enter, arrows and Escape without touching ARIA attributes", async () => {
  const envelope = await validEnvelope();
  const doc = new FakeDocument();
  const source = doc.createElement("button");
  const controller = new KeyboardDndController([textTarget]);
  const unbind = bindKeyboardDragSource(source, envelope, controller);

  const start = keyEvent(" ");
  source.dispatchEvent(start);
  assert.equal(start.defaultPrevented, true);
  assert.equal(controller.session.snapshot.state, "dragging");
  assert.equal(source.hasAttribute("aria-grabbed"), false);
  assert.equal(source.hasAttribute("aria-dropeffect"), false);

  const move = keyEvent("ArrowDown", { ctrlKey: true });
  source.dispatchEvent(move);
  assert.equal(controller.session.snapshot.state, "over-target");
  assert.equal(controller.session.snapshot.operation, "copy");

  const drop = keyEvent("Enter");
  source.dispatchEvent(drop);
  assert.equal(drop.defaultPrevented, true);
  assert.equal(controller.session.snapshot.state, "dropped");

  source.dispatchEvent(keyEvent(" "));
  assert.equal(controller.session.snapshot.state, "dragging", "a terminal session can start a new keyboard drag");
  source.dispatchEvent(keyEvent("Escape"));
  assert.equal(controller.session.snapshot.state, "cancelled");

  unbind();
  source.dispatchEvent(keyEvent(" "));
  assert.equal(controller.session.snapshot.state, "cancelled", "unbind removes the key listener");
});

test("keyboard modifier preference follows native copy/move/link conventions", () => {
  assert.equal(keyboardPreferredOperation({ ctrlKey: true }), "copy");
  assert.equal(keyboardPreferredOperation({ altKey: true }), "copy");
  assert.equal(keyboardPreferredOperation({ shiftKey: true }), "move");
  assert.equal(keyboardPreferredOperation({ ctrlKey: true, shiftKey: true }), "link");
  assert.equal(keyboardPreferredOperation({ metaKey: true }), "link");
  assert.equal(keyboardPreferredOperation({}), undefined);
});

test("duplicate keyboard target ids fail closed", () => {
  assert.throws(() => new KeyboardDndController([textTarget, textTarget]), /duplicate keyboard DnD target/);
});
