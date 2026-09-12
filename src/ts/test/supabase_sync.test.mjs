import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import { OresDndReactiveBus } from "../dist/reactive.js";
import {
  OresDndSupabaseSyncBus,
  commitAcceptedDropWithSupabase,
} from "../dist/supabase_sync.js";

const fixture = JSON.parse(
  await readFile("../../contracts/instances/DndEnvelope/valid/text-copy.json", "utf8"),
);

test("Opto-Sync and ORES-OTel Supabase hooks run in fail-closed order", async () => {
  const calls = [];
  const receipts = [];
  const lifecycle = new OresDndReactiveBus();
  const sync = new OresDndSupabaseSyncBus();
  const subscription = sync.receipts$.subscribe((value) => receipts.push(value));

  await commitAcceptedDropWithSupabase(
    fixture,
    {
      dragId: fixture.dragId,
      accepted: true,
      operation: "copy",
      targetId: "field-1",
    },
    {
      lifecycle,
      sync,
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
  lifecycle.complete();
  sync.complete();

  assert.deepEqual(calls, [
    "forms",
    "opto-local",
    "opto-supabase",
    "otel-local",
    "otel-supabase",
  ]);
  assert.deepEqual(
    receipts.map((value) => [value.channel, value.ok]),
    [["opto-sync", true], ["ores-otel", true]],
  );
  assert.equal(JSON.stringify(receipts).includes("hello"), false);
});

test("Opto-Sync Supabase failure is redacted and prevents ORES-OTel", async () => {
  const calls = [];
  const receipts = [];
  const sync = new OresDndSupabaseSyncBus();
  const subscription = sync.receipts$.subscribe((value) => receipts.push(value));

  await assert.rejects(
    commitAcceptedDropWithSupabase(
      fixture,
      { dragId: fixture.dragId, accepted: true, operation: "copy" },
      {
        sync,
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
  sync.complete();

  assert.deepEqual(calls, ["opto-local", "opto-supabase"]);
  assert.deepEqual(receipts, [{
    dragId: fixture.dragId,
    channel: "opto-sync",
    backend: "supabase",
    ok: false,
    errorCode: "sync-failed",
  }]);
  assert.equal(JSON.stringify(receipts).includes("sensitive provider detail"), false);
  assert.equal(JSON.stringify(receipts).includes("hello"), false);
});
