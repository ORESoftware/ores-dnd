import test from "node:test";
import assert from "node:assert/strict";
import { DndSession } from "../dist/session.js";
import { ORES_DND_MIME } from "../dist/index.js";
import {
  ATTR_COMMIT, ATTR_POLICY, ATTR_SOURCE, ATTR_STATE, ATTR_ZONE, EVENT_DROP, EVENT_STATE,
  autoBind, bindDragSource, bindDropZone, kindForType, preferredOperation, provisionalEnvelope, zoneStateAttribute,
} from "../dist/dom.js";
import { FakeDataTransfer, FakeDocument, dragEvent, otelRecorder } from "./fake-dom.mjs";
import { validEnvelope } from "./helpers.mjs";

const policyA = { targetId: "zone-a", allowedOperations: ["copy", "move"], acceptedKinds: ["text"] };
const policyJson = { targetId: "zone-json", allowedOperations: ["copy"], acceptedKinds: ["json"] };

test("drag source writes the ores MIME payload and starts the session; dragend ends it", async () => {
  const doc = new FakeDocument();
  const envelope = await validEnvelope();
  const session = new DndSession();
  const otel = otelRecorder();
  const source = doc.createElement("div");
  bindDragSource(source, envelope, session, { otel });
  assert.equal(source.getAttribute("draggable"), "true");
  const dt = new FakeDataTransfer();
  source.dispatchEvent(dragEvent("dragstart", { dataTransfer: dt }));
  assert.equal(session.snapshot.state, "dragging");
  assert.equal(dt.effectAllowed, "copyMove");
  assert.ok(dt.getData(ORES_DND_MIME).includes('"dragId":"drag-0001"'));
  assert.equal(dt.getData("text/plain"), "hello");
  source.dispatchEvent(dragEvent("dragend"));
  assert.equal(session.snapshot.state, "cancelled");
  assert.deepEqual(otel.events.map((e) => e.phase), ["drag-start", "drag-end"]);
  assert.equal(JSON.stringify(otel.events).includes("hello"), false);
});

test("drop zone accepts, sets dropEffect/state, and commits on drop", async () => {
  const doc = new FakeDocument();
  const envelope = await validEnvelope();
  const session = new DndSession();
  const otel = otelRecorder();
  const source = doc.createElement("div");
  const zone = doc.createElement("div");
  const dropped = [];
  const stateEvents = [];
  zone.addEventListener(EVENT_STATE, (e) => stateEvents.push(e.detail.state));
  zone.addEventListener(EVENT_DROP, (e) => dropped.push(`event:${e.detail.result.operation}`));
  bindDragSource(source, envelope, session);
  bindDropZone(zone, policyA, session, { onDrop: (env, result) => dropped.push(`handler:${result.operation}:${env.dragId}`) }, { otel });
  assert.equal(zone.getAttribute(ATTR_ZONE), "zone-a");
  assert.equal(zone.getAttribute(ATTR_STATE), "idle");
  assert.ok(zone.getAttribute(ATTR_POLICY).includes('"targetId":"zone-a"'));

  const dt = new FakeDataTransfer();
  source.dispatchEvent(dragEvent("dragstart", { dataTransfer: dt }));
  dt.protectedMode = true;
  const over = dragEvent("dragover", { dataTransfer: dt, ctrlKey: true });
  zone.dispatchEvent(over);
  assert.equal(over.defaultPrevented, true, "an accepting zone must preventDefault on dragover");
  assert.equal(dt.dropEffect, "copy");
  assert.equal(zone.getAttribute(ATTR_STATE), "accepting");

  dt.protectedMode = false;
  const drop = dragEvent("drop", { dataTransfer: dt, ctrlKey: true });
  zone.dispatchEvent(drop);
  assert.equal(drop.defaultPrevented, true);
  assert.equal(session.snapshot.state, "dropped");
  assert.equal(zone.getAttribute(ATTR_STATE), "dropped");
  assert.deepEqual(dropped, ["event:copy", "handler:copy:drag-0001"]);
  assert.deepEqual(otel.events.map((e) => e.phase), ["drag-enter", "drop"]);
  assert.ok(stateEvents.includes("over-target") && stateEvents.includes("dropped"));
});

test("rejecting zone never preventDefaults and exposes the reason", async () => {
  const doc = new FakeDocument();
  const envelope = await validEnvelope();
  const session = new DndSession();
  const source = doc.createElement("div");
  const zone = doc.createElement("div");
  const rejected = [];
  bindDragSource(source, envelope, session);
  bindDropZone(zone, policyJson, session, { onReject: (_env, result) => rejected.push(result.errorCode) });
  const dt = new FakeDataTransfer();
  source.dispatchEvent(dragEvent("dragstart", { dataTransfer: dt }));
  const over = dragEvent("dragover", { dataTransfer: dt });
  zone.dispatchEvent(over);
  assert.equal(over.defaultPrevented, false);
  assert.equal(zone.getAttribute(ATTR_STATE), "rejecting");
  assert.equal(session.snapshot.errorCode, "item-kind-not-accepted");
  zone.dispatchEvent(dragEvent("drop", { dataTransfer: dt }));
  assert.deepEqual(rejected, ["item-kind-not-accepted"]);
  assert.equal(session.snapshot.state, "cancelled");
});

test("dragleave into a child element is ignored; leaving the zone returns to dragging", async () => {
  const doc = new FakeDocument();
  const envelope = await validEnvelope();
  const session = new DndSession();
  const source = doc.createElement("div");
  const zone = doc.createElement("div");
  const child = zone.appendChild(doc.createElement("span"));
  bindDragSource(source, envelope, session);
  bindDropZone(zone, policyA, session);
  const dt = new FakeDataTransfer();
  source.dispatchEvent(dragEvent("dragstart", { dataTransfer: dt }));
  zone.dispatchEvent(dragEvent("dragenter", { dataTransfer: dt }));
  assert.equal(session.snapshot.state, "over-target");
  zone.dispatchEvent(dragEvent("dragleave", { dataTransfer: dt, relatedTarget: child }));
  assert.equal(session.snapshot.state, "over-target");
  zone.dispatchEvent(dragEvent("dragleave", { dataTransfer: dt, relatedTarget: doc.body }));
  assert.equal(session.snapshot.state, "dragging");
  assert.equal(zone.getAttribute(ATTR_STATE), "dragging");
});

test("external text drag is evaluated provisionally then definitively on drop", () => {
  const doc = new FakeDocument();
  const session = new DndSession();
  const zone = doc.createElement("div");
  const dropped = [];
  bindDropZone(zone, { ...policyA, maxTotalBytes: 3 }, session, { onDrop: (env) => dropped.push(env.items[0].data), onReject: (_e, r) => dropped.push(`rejected:${r.errorCode}`) });
  const dt = new FakeDataTransfer();
  dt.setData("text/plain", "hello"); // 5 bytes > maxTotalBytes 3
  dt.protectedMode = true;
  const over = dragEvent("dragover", { dataTransfer: dt });
  zone.dispatchEvent(over);
  assert.equal(over.defaultPrevented, true, "types-only provisional evaluation accepts text/plain");
  assert.equal(session.envelope.sourceRuntime, "external-browser");
  dt.protectedMode = false;
  zone.dispatchEvent(dragEvent("drop", { dataTransfer: dt }));
  assert.deepEqual(dropped, ["rejected:payload-too-large"], "the real payload is re-evaluated on drop");
  assert.equal(session.snapshot.state, "cancelled");
});

test("provisional envelopes, kinds and modifier preferences", () => {
  const env = provisionalEnvelope(["text/uri-list", ORES_DND_MIME, "Files"]);
  assert.deepEqual(env.items.map((i) => i.kind), ["uri", "bytes"]);
  assert.equal(provisionalEnvelope([]).items[0].mediaType, "text/plain");
  assert.equal(kindForType("TEXT/Markdown; charset=utf-8"), "text");
  assert.equal(preferredOperation({ ctrlKey: true }), "copy");
  assert.equal(preferredOperation({ shiftKey: true }), "move");
  assert.equal(preferredOperation({ metaKey: true }), "link");
  assert.equal(preferredOperation({ ctrlKey: true, shiftKey: true }), "link");
  assert.equal(preferredOperation({}), undefined);
  assert.equal(zoneStateAttribute({ state: "over-target", targetId: "a" }, "b"), "dragging");
});

test("autoBind wires HTML-first sources and zones and commits through the zone's endpoint", async () => {
  const doc = new FakeDocument();
  const envelope = await validEnvelope();
  const source = doc.body.appendChild(doc.createElement("div"));
  source.setAttribute(ATTR_SOURCE, JSON.stringify(envelope));
  const zone = doc.body.appendChild(doc.createElement("div"));
  zone.setAttribute(ATTR_ZONE, "zone-a");
  zone.setAttribute(ATTR_POLICY, JSON.stringify(policyA));
  zone.setAttribute(ATTR_COMMIT, "/drops");
  const broken = doc.body.appendChild(doc.createElement("div"));
  broken.setAttribute(ATTR_SOURCE, "{not json");
  const commits = [];
  const session = new DndSession();
  const unbind = autoBind(doc.body, { session, commit: async (el, env, result) => { commits.push(`${el.getAttribute(ATTR_COMMIT)}:${env.dragId}:${result.operation}`); } });
  assert.equal(source.getAttribute("draggable"), "true");
  assert.equal(broken.getAttribute("draggable"), "false");
  const dt = new FakeDataTransfer();
  source.dispatchEvent(dragEvent("dragstart", { dataTransfer: dt }));
  zone.dispatchEvent(dragEvent("dragover", { dataTransfer: dt }));
  zone.dispatchEvent(dragEvent("drop", { dataTransfer: dt }));
  await new Promise((r) => setTimeout(r, 0));
  assert.deepEqual(commits, ["/drops:drag-0001:move"]);
  unbind();
  source.dispatchEvent(dragEvent("dragstart", { dataTransfer: new FakeDataTransfer() }));
  assert.equal(session.snapshot.state, "dropped", "unbound source no longer drives the session");
});
