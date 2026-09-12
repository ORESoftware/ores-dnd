// Wire-level (structural) rules shared by every declaration: the bounded
// scalars of contracts/main.tsp and the array/length bounds — exactly what
// both schema authorities check. Mirrors src/rust/src/wire.rs.

export const SAFE_ID_MAX = 128;
export const MEDIA_TYPE_MAX = 255;
export const ERROR_CODE_MAX = 64;
export const ITEM_NAME_MAX = 255;
export const ITEM_DATA_MAX_CHARS = 1_048_576;
export const ENVELOPE_ITEMS_MAX = 64;
export const OPERATIONS_MAX = 3;
export const KINDS_MAX = 4;
export const MEDIA_PATTERNS_MAX = 64;
export const POLICY_MAX_ITEMS_MAX = 64;
export const TRACE_ID_MAX = 128;
export const TRACE_DESCRIPTION_MAX = 512;
export const TRACE_STEPS_MAX = 256;

const SAFE_ID = /^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$/;
const PROTOCOL_ID = /^ores\.dnd\/v[1-9][0-9]{0,2}$/;
const MEDIA_TYPE = /^[a-z0-9][a-z0-9!#$&^_.+-]{0,126}\/[a-z0-9][a-z0-9!#$&^_.+-]{0,126}$/;
const MEDIA_TYPE_PATTERN = /^[a-z0-9][a-z0-9!#$&^_.+-]{0,126}\/(\*|[a-z0-9][a-z0-9!#$&^_.+-]{0,126})$/;
const TRACEPARENT = /^[0-9a-f]{2}-[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{2}$/;
const ERROR_CODE = /^[a-z0-9][a-z0-9-]{0,63}$/;
const TRACE_ID = /^[a-z0-9][a-z0-9._-]{0,127}$/;

/** JSON Schema `maxLength` counts code points, not UTF-16 units. */
export function codePoints(value: string): number {
  let n = 0;
  for (const _ of value) n += 1;
  return n;
}

export const isSafeId = (v: unknown): v is string => typeof v === "string" && SAFE_ID.test(v);
export const isProtocolId = (v: unknown): v is string => typeof v === "string" && PROTOCOL_ID.test(v);
export const isMediaType = (v: unknown): v is string => typeof v === "string" && v.length <= MEDIA_TYPE_MAX && MEDIA_TYPE.test(v);
export const isMediaTypePattern = (v: unknown): v is string => typeof v === "string" && v.length <= MEDIA_TYPE_MAX && MEDIA_TYPE_PATTERN.test(v);
export const isTraceparent = (v: unknown): v is string => typeof v === "string" && TRACEPARENT.test(v);
export const isErrorCode = (v: unknown): v is string => typeof v === "string" && ERROR_CODE.test(v);
export const isTraceId = (v: unknown): v is string => typeof v === "string" && TRACE_ID.test(v);

export function requireSafeId(value: unknown, label: string): string {
  if (!isSafeId(value)) throw new Error(`${label} must match ^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$`);
  return value;
}

export function optionalSafeId(value: unknown, label: string): string | undefined {
  return value === undefined ? undefined : requireSafeId(value, label);
}

export function checkLength(length: number, min: number, max: number, label: string): void {
  if (length < min || length > max) throw new Error(`${label} must have between ${min} and ${max} entries`);
}
