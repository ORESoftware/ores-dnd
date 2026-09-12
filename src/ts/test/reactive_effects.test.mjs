import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import {
  OresDndEffectBus,
  commitAcceptedDropEffects,
  dndEffectKey,
} from "../dist/reactive_effects.js";

const envelope = JSON.parse(
  await readFile("../../contracts/instances/DndEnvelope/valid/text-copy.json", "utf8"),
);

class MemoryJournal {
  completed = new Set();
  async hasCompleted(key, stage) {
    return this.completed.has(`${key}|${stage}`);
  }
  async markCompleted(key, stage) {
    this.completed.add(`${key}|${stage}`);
  }
}

function result() {
  return {
    dragId: envelope.dragId,
    accepted: true,
    operation: "copy",
    targetId: "field-1",
  };
}

test("reactive effects execute in order and expose one stable idempotency key", async () => {
  const calls = [];
  const keys = [];
  const receipts = [];
  const journal = new MemoryJournal();
  const bus = new OresDndEffectBus();
  const sub = bus.receipts$.subscribe((value) => receipts.push(value));
  const drop = result();

  await commitAcceptedDropEffects(envelope, drop, {
    journal,
    receipts: bus,
    forms: { applyAcceptedDrop: async () => calls.push("forms") },
    optoSync: {
      persistAcceptedDrop: async () => calls.push("opto-local"),
      syncAcceptedDropToSupabase: async (_envelope, _result, key) => {
        calls.push("opto-supabase");
        keys.push(key);
      },
    },
    otel: {
      emitDndEvent: async (event) => {
        assert.equal(JSON.stringify(event).includes("hello"), false);
        calls.push("otel-local");
      },
      syncDndEventToSupabase: async (event, key) => {
        assert.equal(JSON.stringify(event).includes("hello"), false);
        calls.push("otel-supabase");
        keys.push(key);
      },
    },
  });

  sub.unsubscribe();
  bus.complete();
  assert.deepEqual(calls, ["forms", "opto-local", "opto-supabase", "otel-local", "otel-supabase"]);
  assert.equal(new Set(keys).size, 1);
  assert.equal(keys[0], dndEffectKey(drop));
  assert.deepEqual(receipts.map((entry) => entry.status), Array(5).fill("completed"));
  assert.equal(JSON.stringify(receipts).includes("hello"), false);
});

test("retry skips completed stages after a later Supabase failure", async () => {
  const calls = [];
  const receipts = [];
  const journal = new MemoryJournal();
  const bus = new OresDndEffectBus();
  const sub = bus.receipts$.subscribe((value) => receipts.push(value));
  const drop = result();
  let failOtelRemote = true;

  const ports = {
    journal,
    receipts: bus,
    forms: { applyAcceptedDrop: async () => calls.push("forms") },
    optoSync: {
      persistAcceptedDrop: async () => calls.push("opto-local"),
      syncAcceptedDropToSupabase: async () => calls.push("opto-supabase"),
    },
    otel: {
      emitDndEvent: async () => calls.push("otel-local"),
      syncDndEventToSupabase: async () => {
        calls.push("otel-supabase");
        if (failOtelRemote) {
          failOtelRemote = false;
          throw new Error("provider-token=sensitive-value");
        }
      },
    },
  };

  await assert.rejects(commitAcceptedDropEffects(envelope, drop, ports), /provider-token/);
  await commitAcceptedDropEffects(envelope, drop, ports);

  sub.unsubscribe();
  bus.complete();

  assert.deepEqual(calls, [
    "forms",
    "opto-local",
    "opto-supabase",
    "otel-local",
    "otel-supabase",
    "otel-supabase",
  ]);
  const secondAttempt = receipts.slice(5);
  assert.deepEqual(secondAttempt.map((entry) => [entry.stage, entry.status]), [
    ["forms", "skipped"],
    ["opto-local", "skipped"],
    ["opto-supabase", "skipped"],
    ["otel-local", "skipped"],
    ["otel-supabase", "completed"],
  ]);
  const serialized = JSON.stringify(receipts);
  assert.equal(serialized.includes("provider-token"), false);
  assert.equal(serialized.includes("sensitive-value"), false);
  assert.equal(serialized.includes("hello"), false);
});

test("rejected drops execute no effects and emit no receipts", async () => {
  const calls = [];
  const receipts = [];
  const bus = new OresDndEffectBus();
  const sub = bus.receipts$.subscribe((value) => receipts.push(value));

  await commitAcceptedDropEffects(envelope, {
    dragId: envelope.dragId,
    accepted: false,
  }, {
    receipts: bus,
    forms: { applyAcceptedDrop: async () => calls.push("forms") },
  });

  sub.unsubscribe();
  bus.complete();
  assert.deepEqual(calls, []);
  assert.deepEqual(receipts, []);
});
