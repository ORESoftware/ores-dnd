// Bounded materialization of external browser/webview DataTransfer payloads.
//
// Browsers expose only DataTransfer.types during dragover, then make the real
// data/files readable on drop. This module performs that definitive read
// without changing the ores.dnd/v1 wire contract or fetching/opening any URI.
import {
  DEFAULT_MAX_ITEMS,
  DEFAULT_MAX_PAYLOAD_BYTES,
  ORES_DND_MIME,
  ORES_DND_PROTOCOL,
  decodeEnvelope,
  encodeEnvelope,
  validateEnvelope,
  type DndEnvelope,
  type DndItem,
  type DndOperation,
  type ValidationOptions,
} from "./codec.js";
import { ENVELOPE_ITEMS_MAX, ITEM_NAME_MAX, codePoints, isMediaType } from "./wire.js";

/** Conservative raw-file budget before base64 expansion into the v1 string field. */
export const DEFAULT_MAX_EXTERNAL_RAW_BYTES = 512 * 1024;

export type ExternalTransferKind = "ores" | "files" | "uri-list" | "json" | "text";

export interface ExternalTransferOptions extends ValidationOptions {
  /** Source runtime placed on synthesized external envelopes. */
  sourceRuntime?: string;
  /** Source operations for synthesized external envelopes; defaults to copy. */
  allowedOperations?: readonly DndOperation[];
  /** Maximum sum of File bytes read into memory before base64 expansion. */
  maxExternalRawBytes?: number;
}

export interface ExternalTransferRead {
  envelope: DndEnvelope;
  kind: ExternalTransferKind;
  /** Raw file bytes read from File objects. Non-file fallbacks report zero. */
  rawBytesRead: number;
}

const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
let externalReadCounter = 0;

/** RFC 4648 base64 without Node.js Buffer so browser bundles stay platform-neutral. */
export function bytesToBase64(bytes: Uint8Array): string {
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const a = bytes[i]!;
    const hasB = i + 1 < bytes.length;
    const hasC = i + 2 < bytes.length;
    const b = hasB ? bytes[i + 1]! : 0;
    const c = hasC ? bytes[i + 2]! : 0;
    out += BASE64[(a >>> 2) & 0x3f]!;
    out += BASE64[((a & 0x03) << 4) | ((b >>> 4) & 0x0f)]!;
    out += hasB ? BASE64[((b & 0x0f) << 2) | ((c >>> 6) & 0x03)]! : "=";
    out += hasC ? BASE64[c & 0x3f]! : "=";
  }
  return out;
}

/** Strict inverse used by consumers/tests that opt into this adapter convention. */
export function base64ToBytes(value: string): Uint8Array {
  if (value.length === 0) return new Uint8Array();
  if (value.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(value) || /=/.test(value.slice(0, -2))) {
    throw new Error("external byte payload is not canonical base64");
  }
  const padding = value.endsWith("==") ? 2 : value.endsWith("=") ? 1 : 0;
  const out = new Uint8Array((value.length / 4) * 3 - padding);
  let offset = 0;
  for (let i = 0; i < value.length; i += 4) {
    const chunk = value.slice(i, i + 4);
    const sextets = [...chunk].map((ch) => (ch === "=" ? 0 : BASE64.indexOf(ch)));
    if (sextets.some((n) => n < 0)) throw new Error("external byte payload is not canonical base64");
    const n = (sextets[0]! << 18) | (sextets[1]! << 12) | (sextets[2]! << 6) | sextets[3]!;
    if (offset < out.length) out[offset++] = (n >>> 16) & 0xff;
    if (offset < out.length) out[offset++] = (n >>> 8) & 0xff;
    if (offset < out.length) out[offset++] = n & 0xff;
  }
  if (bytesToBase64(out) !== value) throw new Error("external byte payload is not canonical base64");
  return out;
}

function utf8Bytes(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}

function nextDragId(): string {
  externalReadCounter += 1;
  return `external-drop-${externalReadCounter}`;
}

function canonicalMediaType(value: string): string {
  const mediaType = (value.split(";")[0] ?? "").trim().toLowerCase();
  return isMediaType(mediaType) ? mediaType : "application/octet-stream";
}

function safeName(value: string): string | undefined {
  if (value.length === 0 || codePoints(value) > ITEM_NAME_MAX) return undefined;
  return value;
}

function boundedItemLimit(options: ExternalTransferOptions): number {
  const requested = options.maxItems ?? DEFAULT_MAX_ITEMS;
  if (!Number.isSafeInteger(requested) || requested < 1) throw new Error("external maxItems must be a positive integer");
  return Math.min(requested, ENVELOPE_ITEMS_MAX);
}

function boundedPayloadBytes(value: string, options: ExternalTransferOptions, label: string): void {
  const limit = options.maxPayloadBytes ?? DEFAULT_MAX_PAYLOAD_BYTES;
  if (!Number.isSafeInteger(limit) || limit < 1) throw new Error("external maxPayloadBytes must be a positive integer");
  const size = utf8Bytes(value);
  if (size > limit) throw new Error(`${label} exceeds the drag payload byte limit: ${size} > ${limit}`);
}

function synthesizedEnvelope(items: DndItem[], options: ExternalTransferOptions): DndEnvelope {
  const envelope: DndEnvelope = {
    protocol: ORES_DND_PROTOCOL,
    dragId: nextDragId(),
    sourceRuntime: options.sourceRuntime ?? "external-browser",
    allowedOperations: [...(options.allowedOperations ?? ["copy"])],
    items,
  };
  const validated = validateEnvelope(envelope, options);
  // Final serialization is deliberately part of admission: it proves base64,
  // names and JSON framing still fit the canonical total payload bound.
  encodeEnvelope(validated, options);
  return validated;
}

function uriItems(raw: string, maxItems: number): DndItem[] {
  const uris = raw
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith("#"));
  if (uris.length > maxItems) throw new Error(`too many external URI items: ${uris.length} > ${maxItems}`);
  return uris.map((data) => ({ kind: "uri", mediaType: "text/uri-list", data }));
}

/**
 * Definitively materialize an external drop. Precedence is deterministic:
 * ores MIME -> native files -> URI list -> JSON -> plain text.
 *
 * Presence of the ores MIME is authoritative: a malformed ores envelope throws
 * instead of silently downgrading to a less-structured fallback representation.
 */
export async function readExternalTransfer(
  transfer: Pick<DataTransfer, "getData" | "files">,
  options: ExternalTransferOptions = {},
): Promise<ExternalTransferRead | null> {
  const ores = transfer.getData(ORES_DND_MIME);
  if (ores) {
    return { envelope: decodeEnvelope(ores, options), kind: "ores", rawBytesRead: 0 };
  }

  const maxItems = boundedItemLimit(options);
  const files = Array.from(transfer.files ?? []);
  if (files.length > 0) {
    if (files.length > maxItems) throw new Error(`too many external files: ${files.length} > ${maxItems}`);
    const rawLimit = options.maxExternalRawBytes ?? DEFAULT_MAX_EXTERNAL_RAW_BYTES;
    if (!Number.isSafeInteger(rawLimit) || rawLimit < 0) throw new Error("external raw byte limit must be a non-negative integer");
    let declaredTotal = 0;
    for (const file of files) {
      if (!Number.isSafeInteger(file.size) || file.size < 0) throw new Error("external file size must be a non-negative integer");
      declaredTotal += file.size;
      if (declaredTotal > rawLimit) throw new Error(`external files exceed raw byte limit: ${declaredTotal} > ${rawLimit}`);
    }

    let actualTotal = 0;
    const items: DndItem[] = [];
    for (const file of files) {
      const bytes = new Uint8Array(await file.arrayBuffer());
      actualTotal += bytes.byteLength;
      if (actualTotal > rawLimit) throw new Error(`external files exceed raw byte limit after read: ${actualTotal} > ${rawLimit}`);
      const item: DndItem = { kind: "bytes", mediaType: canonicalMediaType(file.type), data: bytesToBase64(bytes) };
      const name = safeName(file.name);
      if (name !== undefined) item.name = name;
      items.push(item);
    }
    return { envelope: synthesizedEnvelope(items, options), kind: "files", rawBytesRead: actualTotal };
  }

  const uriList = transfer.getData("text/uri-list");
  if (uriList) {
    boundedPayloadBytes(uriList, options, "external URI list");
    const items = uriItems(uriList, maxItems);
    if (items.length > 0) return { envelope: synthesizedEnvelope(items, options), kind: "uri-list", rawBytesRead: 0 };
  }

  const json = transfer.getData("application/json");
  if (json) {
    boundedPayloadBytes(json, options, "external JSON");
    try {
      JSON.parse(json);
    } catch (error) {
      throw new Error(`external application/json is invalid: ${error instanceof Error ? error.message : String(error)}`);
    }
    return {
      envelope: synthesizedEnvelope([{ kind: "json", mediaType: "application/json", data: json }], options),
      kind: "json",
      rawBytesRead: 0,
    };
  }

  const text = transfer.getData("text/plain");
  if (text) {
    boundedPayloadBytes(text, options, "external text");
    return {
      envelope: synthesizedEnvelope([{ kind: "text", mediaType: "text/plain", data: text }], options),
      kind: "text",
      rawBytesRead: 0,
    };
  }

  return null;
}

/** True when drop-time async materialization is needed instead of the sync text/custom reader. */
export function requiresExternalMaterialization(transfer: Pick<DataTransfer, "getData" | "files">): boolean {
  if (transfer.getData(ORES_DND_MIME)) return false;
  if ((transfer.files?.length ?? 0) > 0) return true;
  return Boolean(transfer.getData("text/uri-list") || transfer.getData("application/json"));
}
