// Decode any ores.dnd/v1 declaration by name — the runtime adapter used to
// produce tjsv runtime evidence and to replay the shared instance corpus.
import { validateEnvelope, type DndDropResult, type DndItem, type DndTelemetryEvent } from "./codec.js";
import { REJECT_CODES, validatePolicy } from "./policy.js";
import type { DndSessionInput, DndSessionSnapshot, DndSessionTrace } from "./session.js";
import {
  TRACE_DESCRIPTION_MAX, TRACE_STEPS_MAX, checkLength, codePoints, isErrorCode, isMediaType, isMediaTypePattern, isProtocolId, isSafeId,
  isTraceId, isTraceparent, optionalSafeId, requireSafeId,
} from "./wire.js";

export const DECLARATIONS = [
  "DndOperation",
  "DndItemKind",
  "DndLifecyclePhase",
  "DndRejectCode",
  "DndSessionState",
  "DndSessionInputKind",
  "SafeId",
  "ProtocolId",
  "MediaType",
  "MediaTypePattern",
  "Traceparent",
  "ErrorCode",
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

const SCALARS: Record<string, (v: unknown) => boolean> = {
  SafeId: isSafeId,
  ProtocolId: isProtocolId,
  MediaType: isMediaType,
  MediaTypePattern: isMediaTypePattern,
  Traceparent: isTraceparent,
  ErrorCode: isErrorCode,
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
  // the item rules are the envelope's item rules: reuse the envelope validator on a one-item shell
  const shell = validateEnvelope({ protocol: "ores.dnd/v1", dragId: "shell", sourceRuntime: "shell", allowedOperations: ["copy"], items: [value] }, {}, "structural");
  return shell.items[0]!;
}

export function decodeDropResult(value: unknown): DndDropResult {
  if (!isRecord(value)) throw new Error("drop result must be an object");
  exactKeys(value, ["dragId", "accepted", "operation", "targetId", "errorCode"], "drop result");
  if (typeof value.accepted !== "boolean") throw new Error("accepted must be a boolean");
  const result: DndDropResult = { dragId: requireSafeId(value.dragId, "dragId"), accepted: value.accepted };
  const operation = optEnum<DndDropResult["operation"] & string>("DndOperation", value.operation);
  const targetId = optionalSafeId(value.targetId, "targetId");
  if (value.errorCode !== undefined && !isErrorCode(value.errorCode)) throw new Error("errorCode must be lowercase kebab-case (1..=64)");
  if (operation !== undefined) result.operation = operation;
  if (targetId !== undefined) result.targetId = targetId;
  if (value.errorCode !== undefined) result.errorCode = value.errorCode as string;
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
    dragId: requireSafeId(value.dragId, "dragId"),
    sourceRuntime: requireSafeId(value.sourceRuntime, "sourceRuntime"),
    itemCount: value.itemCount,
  };
  const operation = optEnum<DndTelemetryEvent["operation"] & string>("DndOperation", value.operation);
  const targetId = optionalSafeId(value.targetId, "targetId");
  if (operation !== undefined) event.operation = operation;
  if (targetId !== undefined) event.targetId = targetId;
  return event;
}

export function decodeSessionInput(value: unknown): DndSessionInput {
  if (!isRecord(value)) throw new Error("session input must be an object");
  exactKeys(value, ["kind", "envelope", "targetId", "policy", "preferredOperation"], "session input");
  const input: DndSessionInput = { kind: enumValue("DndSessionInputKind", value.kind) };
  if (value.envelope !== undefined) input.envelope = validateEnvelope(value.envelope, {}, "structural");
  const targetId = optionalSafeId(value.targetId, "targetId");
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
  const dragId = optionalSafeId(value.dragId, "dragId");
  const targetId = optionalSafeId(value.targetId, "targetId");
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
  if (!isTraceId(value.id)) throw new Error("trace id must match ^[a-z0-9][a-z0-9._-]{0,127}$");
  checkLength(value.inputs.length, 1, TRACE_STEPS_MAX, "inputs");
  checkLength(value.expected.length, 1, TRACE_STEPS_MAX, "expected");
  const trace: DndSessionTrace = {
    id: value.id,
    inputs: value.inputs.map(decodeSessionInput),
    expected: value.expected.map(decodeSessionSnapshot),
  };
  const description = optStr(value.description, "description");
  if (description !== undefined) {
    if (codePoints(description) > TRACE_DESCRIPTION_MAX) throw new Error("description exceeds 512 characters");
    trace.description = description;
  }
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
  if (name in SCALARS) {
    if (!SCALARS[name]!(value)) throw new Error(`${name} rejected`);
    return value;
  }
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
