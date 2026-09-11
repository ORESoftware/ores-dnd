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

function asNonEmptyString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} must be a non-empty string`);
  }
  return value;
}

function asOptionalString(value: unknown, label: string): string | undefined {
  if (value === undefined) return undefined;
  return asNonEmptyString(value, label);
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
  const item: DndItem = {
    kind: value.kind as DndItemKind,
    mediaType: asNonEmptyString(value.mediaType, "drag item mediaType"),
    data: typeof value.data === "string" ? value.data : (() => { throw new Error("drag item data must be a string"); })(),
  };
  const name = asOptionalString(value.name, "drag item name");
  if (name !== undefined) item.name = name;
  return item;
}

export function validateEnvelope(value: unknown, options: ValidationOptions = {}): DndEnvelope {
  if (!isRecord(value)) throw new Error("drag envelope must be an object");
  assertExactKeys(
    value,
    ["protocol", "dragId", "sourceRuntime", "allowedOperations", "items", "traceparent", "formId"],
    "drag envelope",
  );

  const acceptedProtocols = options.acceptedProtocols ?? [ORES_DND_PROTOCOL];
  const protocol = asNonEmptyString(value.protocol, "protocol");
  if (!acceptedProtocols.includes(protocol)) throw new Error(`unsupported drag protocol: ${protocol}`);

  if (!Array.isArray(value.allowedOperations) || value.allowedOperations.length === 0) {
    throw new Error("allowedOperations must contain at least one operation");
  }
  const allowedOperations = [...new Set(value.allowedOperations.map(asOperation))];

  if (!Array.isArray(value.items) || value.items.length === 0) {
    throw new Error("items must contain at least one drag item");
  }
  const maxItems = options.maxItems ?? DEFAULT_MAX_ITEMS;
  if (value.items.length > maxItems) throw new Error(`too many drag items: ${value.items.length} > ${maxItems}`);

  const envelope: DndEnvelope = {
    protocol,
    dragId: asNonEmptyString(value.dragId, "dragId"),
    sourceRuntime: asNonEmptyString(value.sourceRuntime, "sourceRuntime"),
    allowedOperations,
    items: value.items.map(asItem),
  };
  const traceparent = asOptionalString(value.traceparent, "traceparent");
  const formId = asOptionalString(value.formId, "formId");
  if (traceparent !== undefined) envelope.traceparent = traceparent;
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

function effectAllowedFor(ops: readonly DndOperation[]): DataTransfer["effectAllowed"] {
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
  transfer.effectAllowed = effectAllowedFor(envelope.allowedOperations);
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

export function negotiateOperation(
  source: readonly DndOperation[],
  target: readonly DndOperation[],
  preferred?: DndOperation,
): DndOperation | null {
  if (preferred && source.includes(preferred) && target.includes(preferred)) return preferred;
  for (const op of ["move", "copy", "link"] as const) {
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
