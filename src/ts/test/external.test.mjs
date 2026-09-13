import test from "node:test";
import assert from "node:assert/strict";
import { encodeEnvelope, ORES_DND_MIME } from "../dist/codec.js";
import {
  base64ToBytes,
  bytesToBase64,
  readExternalTransfer,
  requiresExternalMaterialization,
} from "../dist/external.js";
import { validEnvelope } from "./helpers.mjs";

class Transfer {
  #data = new Map();
  files = [];
  setData(type, value) { this.#data.set(type, String(value)); }
  getData(type) { return this.#data.get(type) ?? ""; }
}

function fakeFile(name, type, bytes, { declaredSize = bytes.length, onRead } = {}) {
  return {
    name,
    type,
    size: declaredSize,
    async arrayBuffer() {
      onRead?.();
      const copy = Uint8Array.from(bytes);
      return copy.buffer.slice(copy.byteOffset, copy.byteOffset + copy.byteLength);
    },
  };
}

test("browser-safe base64 codec roundtrips exact bytes", () => {
  const bytes = Uint8Array.from([0, 1, 2, 3, 127, 128, 253, 254, 255]);
  const encoded = bytesToBase64(bytes);
  assert.equal(encoded, "AAECA3+A/f7/");
  assert.deepEqual([...base64ToBytes(encoded)], [...bytes]);
  assert.throws(() => base64ToBytes("not canonical==="), /canonical base64/);
});

test("valid ores MIME is authoritative over every external fallback", async () => {
  const envelope = await validEnvelope();
  const transfer = new Transfer();
  transfer.setData(ORES_DND_MIME, encodeEnvelope(envelope));
  transfer.setData("text/uri-list", "https://example.invalid/fallback");
  transfer.setData("application/json", '{"fallback":true}');
  transfer.setData("text/plain", "fallback");
  transfer.files = [fakeFile("fallback.png", "image/png", [1, 2, 3])];
  const read = await readExternalTransfer(transfer);
  assert.equal(read.kind, "ores");
  assert.equal(read.envelope.dragId, envelope.dragId);
  assert.equal(requiresExternalMaterialization(transfer), false);
});

test("native files are bounded before read and materialized as opaque base64 bytes", async () => {
  const transfer = new Transfer();
  transfer.files = [
    fakeFile("pixel.png", "IMAGE/PNG; charset=binary", [0, 255, 7]),
    fakeFile("unknown.bin", "", [8, 9]),
  ];
  const read = await readExternalTransfer(transfer, { maxExternalRawBytes: 8 });
  assert.equal(read.kind, "files");
  assert.equal(read.rawBytesRead, 5);
  assert.deepEqual(read.envelope.items.map((item) => [item.kind, item.mediaType, item.name]), [
    ["bytes", "image/png", "pixel.png"],
    ["bytes", "application/octet-stream", "unknown.bin"],
  ]);
  assert.deepEqual([...base64ToBytes(read.envelope.items[0].data)], [0, 255, 7]);
  assert.equal(read.envelope.sourceRuntime, "external-browser");
  assert.deepEqual(read.envelope.allowedOperations, ["copy"]);
  assert.equal(requiresExternalMaterialization(transfer), true);

  let reads = 0;
  const tooLarge = new Transfer();
  tooLarge.files = [fakeFile("large.bin", "application/octet-stream", [1], { declaredSize: 9, onRead: () => { reads += 1; } })];
  await assert.rejects(() => readExternalTransfer(tooLarge, { maxExternalRawBytes: 8 }), /raw byte limit/);
  assert.equal(reads, 0, "declared file sizes must be rejected before arrayBuffer allocates");
});

test("URI list beats JSON/text, ignores comments, and stays data-only", async () => {
  const transfer = new Transfer();
  transfer.setData("text/uri-list", "# browser comment\r\nhttps://example.com/a\n\nfile:///tmp/local-reference\n");
  transfer.setData("application/json", '{"fallback":true}');
  transfer.setData("text/plain", "fallback");
  const read = await readExternalTransfer(transfer);
  assert.equal(read.kind, "uri-list");
  assert.deepEqual(read.envelope.items.map((item) => item.data), [
    "https://example.com/a",
    "file:///tmp/local-reference",
  ]);
  assert.ok(read.envelope.items.every((item) => item.kind === "uri" && item.mediaType === "text/uri-list"));
});

test("JSON is parsed before admission and plain text remains the final fallback", async () => {
  const jsonTransfer = new Transfer();
  jsonTransfer.setData("application/json", '{"snake_case":true}');
  jsonTransfer.setData("text/plain", "fallback");
  const json = await readExternalTransfer(jsonTransfer);
  assert.equal(json.kind, "json");
  assert.equal(json.envelope.items[0].data, '{"snake_case":true}');

  const invalid = new Transfer();
  invalid.setData("application/json", "{broken");
  invalid.setData("text/plain", "must-not-downgrade");
  await assert.rejects(() => readExternalTransfer(invalid), /application\/json is invalid/);

  const textTransfer = new Transfer();
  textTransfer.setData("text/plain", "hello");
  const text = await readExternalTransfer(textTransfer);
  assert.equal(text.kind, "text");
  assert.equal(text.envelope.items[0].data, "hello");
  assert.equal(requiresExternalMaterialization(textTransfer), false);
});

test("external item and payload bounds fail closed", async () => {
  const uris = new Transfer();
  uris.setData("text/uri-list", "https://a.invalid\nhttps://b.invalid");
  await assert.rejects(() => readExternalTransfer(uris, { maxItems: 1 }), /too many external URI items/);

  const text = new Transfer();
  text.setData("text/plain", "12345");
  await assert.rejects(() => readExternalTransfer(text, { maxPayloadBytes: 4 }), /payload byte limit/);
});
