// Decode any ores.dnd/v1 declaration by name — the runtime adapter used to
// produce tjsv runtime evidence and to replay the shared instance corpus.
import { validateEnvelope, type DndDropResult, type DndItem, type DndTelemetryEvent } from "./codec.js";
import { REJECT_CODES, validatePolicy } from "./policy.js";
import type { DndSessionInput, DndSessionSnapshot, DndSessionTrace } from "./session.js";

export const DECLARATIONS = [
  "DndOperation",
  "DndItemKind",
  "DndLifecyclePhase",
  "DndRejectCode",
  "DndSessionState",
  "DndSessionInputKind",
  "DndItem",
  "DndEnvelope",
  "DndDropResult",
  "DndTelemetryEvent",
  "DndDropPolicy",
  "DndSessionInput",
  "DndSessionSnapshot",
  "DndSessionTrace",
] as const;

export type Declaration = (typeof DECLARATIONS)[number];

const ENUMS: Record<string, readonly string[]> = {
  DndOperation: ["copy", "move", "link"],
  DndItemKind: ["text", "uri", "json", "bytes"],
  DndLifecyclePhase: ["drag-start", "drag-enter", "drag-over", "drag-leave", "drop", "drag-end"],
  DndRejectCode: REJECT_CODES,
  DndSessionState: ["idle", "dragging", "over-target", "dropped", "cancelled"],
  DndSessionInputKind: ["start", "enter", "leave", "drop", "cancel", "end"],
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function exactKeys(value: Record<string, unknown>, allowed: readonly string[], label: string): void {
  const unknown = Object.keys(value).filter((key) => !allowed.includes(key));
  if (unknown.length > 0) throw new Error(`${label} contains unsupported properties: ${unknown.join(", ")}`);
}

function str(value: unknown, label: string): string {
  if (typeof value !== "string") throw new Error(`${label} must be a string`);
  return value;
}

function optStr(value: unknown, label: string): string | undefined {
  return value === undefined ? undefined : str(value, label);
}

function enumValue<T extends string>(name: string, value: unknown): T {
  const members = ENUMS[name];
  if (!members || typeof value !== "string" || !members.includes(value)) throw new Error(`unsupported ${name}: ${String(value)}`);
  return value as T;
}

function optEnum<T extends string>(name: string, value: unknown): T | undefined {
  return value === undefined ? undefined : enumValue<T>(name, value);
}

export function decodeItem(value: unknown): DndItem {
  if (!isRecord(value)) throw new Error("drag item must be an object");
  exactKeys(value, ["kind", "mediaType", "data", "name"], "drag item");
  const item: DndItem = { kind: enumValue("DndItemKind", value.kind), mediaType: str(value.mediaType, "mediaType"), data: str(value.data, "data") };
  const name = optStr(value.name, "name");
  if (name !== undefined) item.name = name;
  return item;
}

export function decodeDropResult(value: unknown): DndDropResult {
  if (!isRecord(value)) throw new Error("drop result must be an object");
  exactKeys(value, ["dragId", "accepted", "operation", "targetId", "errorCode"], "drop result");
  if (typeof value.accepted !== "boolean") throw new Error("accepted must be a boolean");
  const result: DndDropResult = { dragId: str(value.dragId, "dragId"), accepted: value.accepted };
  const operation = optEnum<DndDropResult["operation"] & string>("DndOperation", value.operation);
  const targetId = optStr(value.targetId, "targetId");
  const errorCode = optStr(value.errorCode, "errorCode");
  if (operation !== undefined) result.operation = operation;
  if (targetId !== undefined) result.targetId = targetId;
  if (errorCode !== undefined) result.errorCode = errorCode;
  return result;
}

export function decodeTelemetryEvent(value: unknown): DndTelemetryEvent {
  if (!isRecord(value)) throw new Error("telemetry event must be an object");
  exactKeys(value, ["phase", "dragId", "sourceRuntime", "itemCount", "operation", "targetId"], "telemetry event");
  if (typeof value.itemCount !== "number" || !Number.isInteger(value.itemCount) || value.itemCount < 0 || value.itemCount > 2147483647) {
    throw new Error("itemCount must be an int32 >= 0");
  }
  const event: DndTelemetryEvent = {
    phase: enumValue("DndLifecyclePhase", value.phase),
    dragId: str(value.dragId, "dragId"),
    sourceRuntime: str(value.sourceRuntime, "sourceRuntime"),
    itemCount: value.itemCount,
  };
  const operation = optEnum<DndTelemetryEvent["operation"] & string>("DndOperation", value.operation);
  const targetId = optStr(value.targetId, "targetId");
  if (operation !== undefined) event.operation = operation;
  if (targetId !== undefined) event.targetId = targetId;
  return event;
}

export function decodeSessionInput(value: unknown): DndSessionInput {
  if (!isRecord(value)) throw new Error("session input must be an object");
  exactKeys(value, ["kind", "envelope", "targetId", "policy", "preferredOperation"], "session input");
  const input: DndSessionInput = { kind: enumValue("DndSessionInputKind", value.kind) };
  if (value.envelope !== undefined) input.envelope = validateEnvelope(value.envelope, {}, "structural");
  const targetId = optStr(value.targetId, "targetId");
  if (targetId !== undefined) input.targetId = targetId;
  if (value.policy !== undefined) input.policy = validatePolicy(value.policy);
  const preferred = optEnum<DndSessionInput["preferredOperation"] & string>("DndOperation", value.preferredOperation);
  if (preferred !== undefined) input.preferredOperation = preferred;
  return input;
}

export function decodeSessionSnapshot(value: unknown): DndSessionSnapshot {
  if (!isRecord(value)) throw new Error("session snapshot must be an object");
  exactKeys(value, ["state", "dragId", "targetId", "operation", "errorCode"], "session snapshot");
  const snapshot: DndSessionSnapshot = { state: enumValue("DndSessionState", value.state) };
  const dragId = optStr(value.dragId, "dragId");
  const targetId = optStr(value.targetId, "targetId");
  const operation = optEnum<DndSessionSnapshot["operation"] & string>("DndOperation", value.operation);
  const errorCode = optEnum<DndSessionSnapshot["errorCode"] & string>("DndRejectCode", value.errorCode);
  if (dragId !== undefined) snapshot.dragId = dragId;
  if (targetId !== undefined) snapshot.targetId = targetId;
  if (operation !== undefined) snapshot.operation = operation;
  if (errorCode !== undefined) snapshot.errorCode = errorCode;
  return snapshot;
}

export function decodeSessionTrace(value: unknown): DndSessionTrace {
  if (!isRecord(value)) throw new Error("session trace must be an object");
  exactKeys(value, ["id", "description", "inputs", "expected"], "session trace");
  if (!Array.isArray(value.inputs) || !Array.isArray(value.expected)) throw new Error("inputs and expected must be arrays");
  const trace: DndSessionTrace = {
    id: str(value.id, "id"),
    inputs: value.inputs.map(decodeSessionInput),
    expected: value.expected.map(decodeSessionSnapshot),
  };
  const description = optStr(value.description, "description");
  if (description !== undefined) trace.description = description;
  if (trace.inputs.length !== trace.expected.length) throw new Error("trace inputs and expected must have the same length");
  return trace;
}

/**
 * Structural decode (closed enums, no unknown properties, contract bounds) for
 * the named declaration; `DndEnvelope` additionally runs semantic validation.
 * Throws on rejection.
 */
export function decodeDeclaration(declaration: string, json: string): unknown {
  const value: unknown = JSON.parse(json);
  const name = declaration.includes(".") ? declaration.slice(declaration.lastIndexOf(".") + 1) : declaration;
  if (name in ENUMS) return enumValue(name, value);
  switch (name) {
    case "DndItem":
      return decodeItem(value);
    case "DndEnvelope":
      return validateEnvelope(value);
    case "DndDropResult":
      return decodeDropResult(value);
    case "DndTelemetryEvent":
      return decodeTelemetryEvent(value);
    case "DndDropPolicy":
      return validatePolicy(value);
    case "DndSessionInput":
      return decodeSessionInput(value);
    case "DndSessionSnapshot":
      return decodeSessionSnapshot(value);
    case "DndSessionTrace":
      return decodeSessionTrace(value);
    default:
      throw new Error(`unknown declaration: ${declaration}`);
  }
}
