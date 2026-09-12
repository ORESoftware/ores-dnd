// Drop policies: what a target accepts, evaluated in the fixed order every
// runtime shares (docs/DESIGN.md §Policy evaluation).
import { negotiateOperation, type DndEnvelope, type DndItemKind, type DndOperation } from "./codec.js";

export type DndRejectCode =
  | "invalid-envelope"
  | "no-common-operation"
  | "item-kind-not-accepted"
  | "media-type-not-accepted"
  | "too-many-items"
  | "payload-too-large"
  | "form-mismatch"
  | "no-active-target"
  | "target-mismatch"
  | "cancelled";

export const REJECT_CODES: readonly DndRejectCode[] = [
  "invalid-envelope",
  "no-common-operation",
  "item-kind-not-accepted",
  "media-type-not-accepted",
  "too-many-items",
  "payload-too-large",
  "form-mismatch",
  "no-active-target",
  "target-mismatch",
  "cancelled",
];

export interface DndDropPolicy {
  targetId: string;
  allowedOperations: DndOperation[];
  acceptedKinds: DndItemKind[];
  /** Exact media types or `type/*` wildcards. Absent means any media type. */
  acceptedMediaTypes?: string[];
  maxItems?: number;
  maxTotalBytes?: number;
  /** ores-forms binding; when both policy and envelope carry a formId they must match. */
  formId?: string;
}

const OPERATIONS: readonly DndOperation[] = ["copy", "move", "link"];
const ITEM_KINDS: readonly DndItemKind[] = ["text", "uri", "json", "bytes"];
const INT32_MAX = 2147483647;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isInt(value: unknown, min: number): value is number {
  return typeof value === "number" && Number.isInteger(value) && value >= min && value <= INT32_MAX;
}

/** Structural + bounds validation of a policy (unknown properties fail closed). */
export function validatePolicy(value: unknown): DndDropPolicy {
  if (!isRecord(value)) throw new Error("drop policy must be an object");
  const allowed = ["targetId", "allowedOperations", "acceptedKinds", "acceptedMediaTypes", "maxItems", "maxTotalBytes", "formId"];
  const unknown = Object.keys(value).filter((key) => !allowed.includes(key));
  if (unknown.length > 0) throw new Error(`drop policy contains unsupported properties: ${unknown.join(", ")}`);
  if (typeof value.targetId !== "string" || value.targetId.length === 0) throw new Error("targetId must be a non-empty string");
  if (!Array.isArray(value.allowedOperations) || !value.allowedOperations.every((op) => OPERATIONS.includes(op as DndOperation))) {
    throw new Error("allowedOperations must be an array of copy|move|link");
  }
  if (!Array.isArray(value.acceptedKinds) || !value.acceptedKinds.every((kind) => ITEM_KINDS.includes(kind as DndItemKind))) {
    throw new Error("acceptedKinds must be an array of text|uri|json|bytes");
  }
  const policy: DndDropPolicy = {
    targetId: value.targetId,
    allowedOperations: [...(value.allowedOperations as DndOperation[])],
    acceptedKinds: [...(value.acceptedKinds as DndItemKind[])],
  };
  if (value.acceptedMediaTypes !== undefined) {
    if (!Array.isArray(value.acceptedMediaTypes) || !value.acceptedMediaTypes.every((t) => typeof t === "string")) {
      throw new Error("acceptedMediaTypes must be an array of strings");
    }
    policy.acceptedMediaTypes = [...(value.acceptedMediaTypes as string[])];
  }
  if (value.maxItems !== undefined) {
    if (!isInt(value.maxItems, 1)) throw new Error("maxItems must be an integer >= 1");
    policy.maxItems = value.maxItems;
  }
  if (value.maxTotalBytes !== undefined) {
    if (!isInt(value.maxTotalBytes, 1)) throw new Error("maxTotalBytes must be an integer >= 1");
    policy.maxTotalBytes = value.maxTotalBytes;
  }
  if (value.formId !== undefined) {
    if (typeof value.formId !== "string" || value.formId.length === 0) throw new Error("formId must be a non-empty string");
    policy.formId = value.formId;
  }
  return policy;
}

/** `pattern` is an exact media type or a `type/*` wildcard; ASCII case-insensitive, parameters ignored. */
export function mediaTypeMatches(pattern: string, mediaType: string): boolean {
  const media = (mediaType.split(";")[0] ?? "").trim().toLowerCase();
  const p = pattern.trim().toLowerCase();
  if (p.endsWith("/*")) {
    const slash = media.indexOf("/");
    return slash > 0 && media.slice(0, slash) === p.slice(0, -2);
  }
  return media === p;
}

const encoder = new TextEncoder();

/** Total UTF-8 byte length of all item data (the `maxTotalBytes` measure). */
export function totalDataBytes(envelope: DndEnvelope): number {
  return envelope.items.reduce((sum, item) => sum + encoder.encode(item.data).byteLength, 0);
}

export type PolicyVerdict = { accepted: true; operation: DndOperation } | { accepted: false; errorCode: DndRejectCode };

/** Evaluate `policy` against `envelope`: operation → kind → media type → count → bytes → form. */
export function evaluatePolicy(envelope: DndEnvelope, policy: DndDropPolicy, preferred?: DndOperation): PolicyVerdict {
  const operation = negotiateOperation(envelope.allowedOperations, policy.allowedOperations, preferred);
  if (!operation) return { accepted: false, errorCode: "no-common-operation" };
  if (envelope.items.some((item) => !policy.acceptedKinds.includes(item.kind))) {
    return { accepted: false, errorCode: "item-kind-not-accepted" };
  }
  const patterns = policy.acceptedMediaTypes;
  if (patterns && !envelope.items.every((item) => patterns.some((pattern) => mediaTypeMatches(pattern, item.mediaType)))) {
    return { accepted: false, errorCode: "media-type-not-accepted" };
  }
  if (policy.maxItems !== undefined && envelope.items.length > policy.maxItems) {
    return { accepted: false, errorCode: "too-many-items" };
  }
  if (policy.maxTotalBytes !== undefined && totalDataBytes(envelope) > policy.maxTotalBytes) {
    return { accepted: false, errorCode: "payload-too-large" };
  }
  if (policy.formId !== undefined && envelope.formId !== undefined && policy.formId !== envelope.formId) {
    return { accepted: false, errorCode: "form-mismatch" };
  }
  return { accepted: true, operation };
}
