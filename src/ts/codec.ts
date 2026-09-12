// Wire types and codec for the ores.dnd/v1 envelope (see docs/DESIGN.md).
import {
  ENVELOPE_ITEMS_MAX, ITEM_DATA_MAX_CHARS, ITEM_NAME_MAX, OPERATIONS_MAX, checkLength, codePoints, isMediaType, isProtocolId,
  isTraceparent, optionalSafeId, requireSafeId,
} from "./wire.js";
export const ORES_DND_PROTOCOL = "ores.dnd/v1" as const;
export const ORES_DND_MIME = "application/vnd.ores.dnd+json" as const;
export const DEFAULT_MAX_PAYLOAD_BYTES = 1024 * 1024;
export const DEFAULT_MAX_ITEMS = 64;

export type DndOperation = "copy" | "move" | "link";
export type DndItemKind = "text" | "uri" | "json" | "bytes";
export type DndLifecyclePhase =
  | "drag-start"
  | "drag-enter"
  | "drag-over"
  | "drag-leave"
  | "drop"
  | "drag-end";

export interface DndItem {
  kind: DndItemKind;
  mediaType: string;
  data: string;
  name?: string;
}

export interface DndEnvelope {
  protocol: string;
  dragId: string;
  sourceRuntime: string;
  allowedOperations: DndOperation[];
  items: DndItem[];
  traceparent?: string;
  formId?: string;
}

export interface DndDropResult {
  dragId: string;
  accepted: boolean;
  operation?: DndOperation;
  targetId?: string;
  errorCode?: string;
}

export interface DndTelemetryEvent {
  phase: DndLifecyclePhase;
  dragId: string;
  sourceRuntime: string;
  itemCount: number;
  operation?: DndOperation;
  targetId?: string;
}

export interface ValidationOptions {
  maxPayloadBytes?: number;
  maxItems?: number;
  acceptedProtocols?: readonly string[];
}

export interface OresOtelPort {
  emitDndEvent(event: DndTelemetryEvent): void | Promise<void>;
}

export interface OptoSyncPort {
  persistAcceptedDrop(envelope: DndEnvelope, result: DndDropResult): void | Promise<void>;
}

export interface OresFormsPort {
  applyAcceptedDrop(envelope: DndEnvelope, result: DndDropResult): void | Promise<void>;
}

export interface DropCommitPorts {
  otel?: OresOtelPort;
  optoSync?: OptoSyncPort;
  forms?: OresFormsPort;
}

const OPERATIONS: readonly DndOperation[] = ["copy", "move", "link"];
const ITEM_KINDS: readonly DndItemKind[] = ["text", "uri", "json", "bytes"];
const PHASES: readonly DndLifecyclePhase[] = [
  "drag-start",
  "drag-enter",
  "drag-over",
  "drag-leave",
  "drop",
  "drag-end",
];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function assertExactKeys(value: Record<string, unknown>, allowed: readonly string[], label: string): void {
  const unknown = Object.keys(value).filter((key) => !allowed.includes(key));
  if (unknown.length > 0) {
    throw new Error(`${label} contains unsupported properties: ${unknown.join(", ")}`);
  }
}

function asOperation(value: unknown): DndOperation {
  if (typeof value !== "string" || !OPERATIONS.includes(value as DndOperation)) {
    throw new Error(`unsupported drag operation: ${String(value)}`);
  }
  return value as DndOperation;
}

function asItem(value: unknown): DndItem {
  if (!isRecord(value)) throw new Error("drag item must be an object");
  assertExactKeys(value, ["kind", "mediaType", "data", "name"], "drag item");
  if (typeof value.kind !== "string" || !ITEM_KINDS.includes(value.kind as DndItemKind)) {
    throw new Error(`unsupported drag item kind: ${String(value.kind)}`);
  }
  if (!isMediaType(value.mediaType)) throw new Error("drag item mediaType must be a canonical lowercase type/subtype");
  if (typeof value.data !== "string") throw new Error("drag item data must be a string");
  if (codePoints(value.data) > ITEM_DATA_MAX_CHARS) throw new Error("drag item data exceeds the contract maximum length");
  const item: DndItem = { kind: value.kind as DndItemKind, mediaType: value.mediaType, data: value.data };
  if (value.name !== undefined) {
    if (typeof value.name !== "string" || value.name.length === 0 || codePoints(value.name) > ITEM_NAME_MAX) {
      throw new Error("drag item name must be 1..=255 characters");
    }
    item.name = value.name;
  }
  return item;
}

/**
 * Validate an envelope. `mode: "semantic"` (default) applies the runtime rules
 * (accepted protocol, non-empty operations/items/strings, item limit) on top of
 * the structural contract; `mode: "structural"` checks only what the schema
 * authorities check (types, closed enums, no unknown properties) — used when an
 * envelope is embedded in another declaration such as `DndSessionInput`.
 */
export function validateEnvelope(value: unknown, options: ValidationOptions = {}, mode: "semantic" | "structural" = "semantic"): DndEnvelope {
  if (!isRecord(value)) throw new Error("drag envelope must be an object");
  assertExactKeys(
    value,
    ["protocol", "dragId", "sourceRuntime", "allowedOperations", "items", "traceparent", "formId"],
    "drag envelope",
  );
  const semantic = mode === "semantic";

  if (!isProtocolId(value.protocol)) throw new Error(`malformed drag protocol tag: ${String(value.protocol)}`);
  const protocol = value.protocol;
  const acceptedProtocols = options.acceptedProtocols ?? [ORES_DND_PROTOCOL];
  if (semantic && !acceptedProtocols.includes(protocol)) throw new Error(`unsupported drag protocol: ${protocol}`);

  if (!Array.isArray(value.allowedOperations)) throw new Error("allowedOperations must be an array");
  checkLength(value.allowedOperations.length, 1, OPERATIONS_MAX, "allowedOperations");
  const allowedOperations = [...new Set(value.allowedOperations.map(asOperation))];

  if (!Array.isArray(value.items)) throw new Error("items must be an array");
  checkLength(value.items.length, 1, ENVELOPE_ITEMS_MAX, "items");
  const maxItems = options.maxItems ?? DEFAULT_MAX_ITEMS;
  if (semantic && value.items.length > maxItems) throw new Error(`too many drag items: ${value.items.length} > ${maxItems}`);

  const envelope: DndEnvelope = {
    protocol,
    dragId: requireSafeId(value.dragId, "dragId"),
    sourceRuntime: requireSafeId(value.sourceRuntime, "sourceRuntime"),
    allowedOperations,
    items: value.items.map((item) => asItem(item)),
  };
  if (value.traceparent !== undefined) {
    if (!isTraceparent(value.traceparent)) throw new Error("traceparent must be a W3C trace-context value");
    envelope.traceparent = value.traceparent;
  }
  const formId = optionalSafeId(value.formId, "formId");
  if (formId !== undefined) envelope.formId = formId;
  return envelope;
}

function utf8Bytes(value: string): number {
  return new TextEncoder().encode(value).byteLength;
}

export function encodeEnvelope(envelope: DndEnvelope, options: ValidationOptions = {}): string {
  const validated = validateEnvelope(envelope, options);
  const json = JSON.stringify(validated);
  const maxPayloadBytes = options.maxPayloadBytes ?? DEFAULT_MAX_PAYLOAD_BYTES;
  const size = utf8Bytes(json);
  if (size > maxPayloadBytes) throw new Error(`drag payload too large: ${size} > ${maxPayloadBytes} bytes`);
  return json;
}

export function decodeEnvelope(json: string, options: ValidationOptions = {}): DndEnvelope {
  const maxPayloadBytes = options.maxPayloadBytes ?? DEFAULT_MAX_PAYLOAD_BYTES;
  const size = utf8Bytes(json);
  if (size > maxPayloadBytes) throw new Error(`drag payload too large: ${size} > ${maxPayloadBytes} bytes`);
  let value: unknown;
  try {
    value = JSON.parse(json) as unknown;
  } catch (error) {
    throw new Error(`invalid drag payload JSON: ${error instanceof Error ? error.message : String(error)}`);
  }
  return validateEnvelope(value, options);
}

function effectAllowedForInternal(ops: readonly DndOperation[]): DataTransfer["effectAllowed"] {
  const set = new Set(ops);
  if (set.size === 3) return "all";
  if (set.has("copy") && set.has("move")) return "copyMove";
  if (set.has("copy") && set.has("link")) return "copyLink";
  if (set.has("link") && set.has("move")) return "linkMove";
  if (set.has("copy")) return "copy";
  if (set.has("move")) return "move";
  if (set.has("link")) return "link";
  return "none";
}

export function writeToDataTransfer(
  transfer: DataTransfer,
  envelope: DndEnvelope,
  options: ValidationOptions = {},
): void {
  const json = encodeEnvelope(envelope, options);
  transfer.setData(ORES_DND_MIME, json);
  transfer.effectAllowed = effectAllowedForInternal(envelope.allowedOperations);
  const text = envelope.items.find((item) => item.kind === "text" && item.mediaType === "text/plain");
  if (text) transfer.setData("text/plain", text.data);
}

export function readFromDataTransfer(
  transfer: DataTransfer,
  options: ValidationOptions = {},
): DndEnvelope | null {
  const json = transfer.getData(ORES_DND_MIME);
  if (json) return decodeEnvelope(json, options);

  const text = transfer.getData("text/plain");
  if (!text) return null;
  return validateEnvelope(
    {
      protocol: ORES_DND_PROTOCOL,
      dragId: `external-text-${Date.now()}`,
      sourceRuntime: "external-browser",
      allowedOperations: ["copy"],
      items: [{ kind: "text", mediaType: "text/plain", data: text }],
    },
    options,
  );
}

/** Deterministic negotiation order shared by every runtime. */
export const NEGOTIATION_ORDER: readonly DndOperation[] = ["move", "copy", "link"];

/** The HTML5 `effectAllowed` keyword for a set of operations. */
export function effectAllowedFor(ops: readonly DndOperation[]): DataTransfer["effectAllowed"] {
  return effectAllowedForInternal(ops);
}

export function negotiateOperation(
  source: readonly DndOperation[],
  target: readonly DndOperation[],
  preferred?: DndOperation,
): DndOperation | null {
  if (preferred && source.includes(preferred) && target.includes(preferred)) return preferred;
  for (const op of NEGOTIATION_ORDER) {
    if (source.includes(op) && target.includes(op)) return op;
  }
  return null;
}

export function telemetryFor(
  phase: DndLifecyclePhase,
  envelope: DndEnvelope,
  operation?: DndOperation,
  targetId?: string,
): DndTelemetryEvent {
  if (!PHASES.includes(phase)) throw new Error(`unsupported lifecycle phase: ${phase}`);
  const event: DndTelemetryEvent = {
    phase,
    dragId: envelope.dragId,
    sourceRuntime: envelope.sourceRuntime,
    itemCount: envelope.items.length,
  };
  if (operation !== undefined) event.operation = operation;
  if (targetId !== undefined) event.targetId = targetId;
  return event;
}

export async function commitAcceptedDrop(
  envelope: DndEnvelope,
  result: DndDropResult,
  ports: DropCommitPorts,
): Promise<void> {
  const safeEnvelope = validateEnvelope(envelope);
  if (result.dragId !== safeEnvelope.dragId) throw new Error("drop result dragId does not match envelope");
  if (!result.accepted) return;
  if (!result.operation || !safeEnvelope.allowedOperations.includes(result.operation)) {
    throw new Error("accepted drop must use an operation allowed by the source envelope");
  }
  await ports.forms?.applyAcceptedDrop(safeEnvelope, result);
  await ports.optoSync?.persistAcceptedDrop(safeEnvelope, result);
  await ports.otel?.emitDndEvent(telemetryFor("drop", safeEnvelope, result.operation, result.targetId));
}
