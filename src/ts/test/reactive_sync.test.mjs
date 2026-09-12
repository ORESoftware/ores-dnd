import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import { DndReactiveBus, commitAcceptedDropReactive } from "../dist/reactive_sync.js";

const fixture = JSON.parse(
  await readFile("../../contracts/instances/DndEnvelope/valid/text-copy.json", "utf8"),
);

test("reactive commit calls Opto-Sync and ORES-OTel Supabase hooks in order", async () => {
  const calls = [];
  const events = [];
  const bus = new DndReactiveBus();
  const subscription = bus.events$.subscribe((event) => events.push(event));

  await commitAcceptedDropReactive(
    fixture,
    {
      dragId: fixture.dragId,
      accepted: true,
      operation: "copy",
      targetId: "field-1",
    },
    {
      reactive: bus,
      forms: { applyAcceptedDrop: async () => calls.push("forms") },
      optoSync: {
        persistAcceptedDrop: async () => calls.push("opto-local"),
        syncAcceptedDropToSupabase: async () => calls.push("opto-supabase"),
      },
      otel: {
        emitDndEvent: async (event) => {
          assert.equal(JSON.stringify(event).includes("hello"), false);
          calls.push("otel-local");
        },
        syncDndEventToSupabase: async (event) => {
          assert.equal(JSON.stringify(event).includes("hello"), false);
          calls.push("otel-supabase");
        },
      },
    },
  );

  subscription.unsubscribe();
  bus.complete();

  assert.deepEqual(calls, [
    "forms",
    "opto-local",
    "opto-supabase",
    "otel-local",
    "otel-supabase",
  ]);
  assert.equal(JSON.stringify(events).includes("hello"), false);
  assert.deepEqual(
    events.filter((event) => event.kind === "supabase-sync").map((event) => [event.channel, event.ok]),
    [["opto-sync", true], ["ores-otel", true]],
  );
});

test("failed Opto-Sync Supabase write emits a redacted receipt and stops OTel", async () => {
  const calls = [];
  const receipts = [];
  const bus = new DndReactiveBus();
  const subscription = bus.supabaseSync$.subscribe((event) => receipts.push(event));

  await assert.rejects(
    commitAcceptedDropReactive(
      fixture,
      { dragId: fixture.dragId, accepted: true, operation: "copy" },
      {
        reactive: bus,
        optoSync: {
          persistAcceptedDrop: async () => calls.push("opto-local"),
          syncAcceptedDropToSupabase: async () => {
            calls.push("opto-supabase");
            throw new Error("sensitive provider detail");
          },
        },
        otel: {
          emitDndEvent: async () => calls.push("otel-local"),
          syncDndEventToSupabase: async () => calls.push("otel-supabase"),
        },
      },
    ),
    /sensitive provider detail/,
  );

  subscription.unsubscribe();
  bus.complete();

  assert.deepEqual(calls, ["opto-local", "opto-supabase"]);
  assert.deepEqual(receipts, [{
    kind: "supabase-sync",
    dragId: fixture.dragId,
    channel: "opto-sync",
    backend: "supabase",
    ok: false,
    errorCode: "sync-failed",
  }]);
  assert.equal(JSON.stringify(receipts).includes("sensitive provider detail"), false);
  assert.equal(JSON.stringify(receipts).includes("hello"), false);
});
