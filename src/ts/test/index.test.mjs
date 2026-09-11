import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import {
  ORES_DND_MIME,
  commitAcceptedDrop,
  decodeEnvelope,
  encodeEnvelope,
  negotiateOperation,
  telemetryFor,
  validateEnvelope,
  writeToDataTransfer,
} from "../dist/index.js";

const fixture = JSON.parse(await readFile("../../contracts/instances/DndEnvelope/valid/text-copy.json", "utf8"));

test("shared fixture round-trips canonically", () => {
  const parsed = validateEnvelope(fixture);
  assert.deepEqual(decodeEnvelope(encodeEnvelope(parsed)), parsed);
});

test("unknown operation fails closed", async () => {
  const invalid = await readFile("../../contracts/instances/DndEnvelope/invalid/unknown-op.json", "utf8");
  assert.throws(() => decodeEnvelope(invalid), /unsupported drag operation/);
});

test("unknown properties fail closed", () => {
  assert.throws(() => validateEnvelope({ ...fixture, secret: "do-not-accept" }), /unsupported properties/);
});

test("payload byte limit is checked before parsing", () => {
  assert.throws(() => decodeEnvelope(" ".repeat(32), { maxPayloadBytes: 16 }), /payload too large/);
});

test("operation negotiation prefers move then copy then link", () => {
  assert.equal(negotiateOperation(["copy", "move"], ["copy", "move"]), "move");
  assert.equal(negotiateOperation(["copy"], ["move"]), null);
  assert.equal(negotiateOperation(["copy", "link"], ["copy", "link"], "link"), "link");
});

test("telemetry is content-free", () => {
  const event = telemetryFor("drop", fixture, "copy", "zone-a");
  const serialized = JSON.stringify(event);
  assert.equal(serialized.includes("hello"), false);
  assert.equal(event.itemCount, 1);
});

test("DataTransfer adapter writes the ores MIME and text fallback", () => {
  const values = new Map();
  const transfer = {
    effectAllowed: "none",
    setData(type, value) { values.set(type, value); },
    getData(type) { return values.get(type) ?? ""; },
  };
  writeToDataTransfer(transfer, fixture);
  assert.equal(transfer.effectAllowed, "copyMove");
  assert.equal(values.has(ORES_DND_MIME), true);
  assert.equal(values.get("text/plain"), "hello");
});

test("forms -> opto-sync -> otel side effects occur only after accepted validation", async () => {
  const calls = [];
  await commitAcceptedDrop(fixture, { dragId: fixture.dragId, accepted: true, operation: "copy", targetId: "field-1" }, {
    forms: { applyAcceptedDrop: async () => calls.push("forms") },
    optoSync: { persistAcceptedDrop: async () => calls.push("sync") },
    otel: { emitDndEvent: async (event) => { calls.push("otel"); assert.equal(JSON.stringify(event).includes("hello"), false); } },
  });
  assert.deepEqual(calls, ["forms", "sync", "otel"]);
});
