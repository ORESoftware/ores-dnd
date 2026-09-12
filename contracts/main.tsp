import "@typespec/json-schema";
using TypeSpec.JsonSchema;

// ores.dnd/v1 — cross-runtime drag-and-drop contract.
//
// This file and contracts/authored.schema.json are independent, human-authored
// peer authorities. Neither is generated from the other. tjsv compares them and
// fails closed on any divergence.

namespace OresDnd;

// ---------------------------------------------------------------------------
// Wire enums
// ---------------------------------------------------------------------------

@id("DndOperation")
enum DndOperation {
  copy: "copy",
  move: "move",
  link: "link",
}

@id("DndItemKind")
enum DndItemKind {
  text: "text",
  uri: "uri",
  json: "json",
  bytes: "bytes",
}

@id("DndLifecyclePhase")
enum DndLifecyclePhase {
  dragStart: "drag-start",
  dragEnter: "drag-enter",
  dragOver: "drag-over",
  dragLeave: "drag-leave",
  drop: "drop",
  dragEnd: "drag-end",
}

/// Standard rejection codes every runtime emits in the same order of evaluation:
/// operation → item kind → media type → item count → total bytes → form binding.
@id("DndRejectCode")
enum DndRejectCode {
  invalidEnvelope: "invalid-envelope",
  noCommonOperation: "no-common-operation",
  itemKindNotAccepted: "item-kind-not-accepted",
  mediaTypeNotAccepted: "media-type-not-accepted",
  tooManyItems: "too-many-items",
  payloadTooLarge: "payload-too-large",
  formMismatch: "form-mismatch",
  noActiveTarget: "no-active-target",
  targetMismatch: "target-mismatch",
  cancelled: "cancelled",
}

/// Observable states of one drag session.
@id("DndSessionState")
enum DndSessionState {
  idle: "idle",
  dragging: "dragging",
  overTarget: "over-target",
  dropped: "dropped",
  cancelled: "cancelled",
}

/// Inputs a host runtime feeds into the session state machine.
@id("DndSessionInputKind")
enum DndSessionInputKind {
  start: "start",
  enter: "enter",
  leave: "leave",
  drop: "drop",
  cancel: "cancel",
  end: "end",
}

// ---------------------------------------------------------------------------
// Bounded scalars — every identifier that ends up in a DOM attribute, a log
// line or a telemetry field is length-bounded and drawn from a safe charset;
// media types are canonical lowercase without parameters.
// ---------------------------------------------------------------------------

/// Identifier safe for DOM attributes, logs and telemetry.
@id("SafeId")
@minLength(1)
@maxLength(128)
@pattern("^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$")
scalar SafeId extends string;

/// Protocol tag: `ores.dnd/v<major>`.
@id("ProtocolId")
@pattern("^ores\\.dnd/v[1-9][0-9]{0,2}$")
scalar ProtocolId extends string;

/// Canonical media type: lowercase `type/subtype`, no parameters.
@id("MediaType")
@minLength(3)
@maxLength(255)
@pattern("^[a-z0-9][a-z0-9!#$&^_.+-]{0,126}/[a-z0-9][a-z0-9!#$&^_.+-]{0,126}$")
scalar MediaType extends string;

/// Exact canonical media type or a `type/*` wildcard.
@id("MediaTypePattern")
@minLength(3)
@maxLength(255)
@pattern("^[a-z0-9][a-z0-9!#$&^_.+-]{0,126}/(\\*|[a-z0-9][a-z0-9!#$&^_.+-]{0,126})$")
scalar MediaTypePattern extends string;

/// W3C Trace Context `traceparent` header value.
@id("Traceparent")
@pattern("^[0-9a-f]{2}-[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{2}$")
scalar Traceparent extends string;

/// Machine-readable error code: lowercase kebab-case.
@id("ErrorCode")
@minLength(1)
@maxLength(64)
@pattern("^[a-z0-9][a-z0-9-]{0,63}$")
scalar ErrorCode extends string;

// ---------------------------------------------------------------------------
// Payload
// ---------------------------------------------------------------------------

@id("DndItem")
model DndItem {
  kind: DndItemKind;
  mediaType: MediaType;
  /// Item payload; the whole envelope is additionally size-bounded before parsing.
  @maxLength(1048576)
  data: string;
  @minLength(1)
  @maxLength(255)
  name?: string;
}

@id("DndEnvelope")
model DndEnvelope {
  protocol: ProtocolId;
  dragId: SafeId;
  sourceRuntime: SafeId;
  @minItems(1)
  @maxItems(3)
  allowedOperations: DndOperation[];
  @minItems(1)
  @maxItems(64)
  items: DndItem[];
  traceparent?: Traceparent;
  formId?: SafeId;
}

@id("DndDropResult")
model DndDropResult {
  dragId: SafeId;
  accepted: boolean;
  operation?: DndOperation;
  targetId?: SafeId;
  errorCode?: ErrorCode;
}

@id("DndTelemetryEvent")
model DndTelemetryEvent {
  phase: DndLifecyclePhase;
  dragId: SafeId;
  sourceRuntime: SafeId;
  @minValue(0)
  itemCount: int32;
  operation?: DndOperation;
  targetId?: SafeId;
}

// ---------------------------------------------------------------------------
// Target policy
// ---------------------------------------------------------------------------

/// What a drop target is willing to accept. Declared by the target, evaluated by
/// every runtime with the same rules, and never mutated by a drag.
@id("DndDropPolicy")
model DndDropPolicy {
  targetId: SafeId;
  @minItems(1)
  @maxItems(3)
  allowedOperations: DndOperation[];
  @minItems(1)
  @maxItems(4)
  acceptedKinds: DndItemKind[];
  /// Exact media types or `type/*` wildcards. Absent means any media type.
  @minItems(1)
  @maxItems(64)
  acceptedMediaTypes?: MediaTypePattern[];
  @minValue(1)
  @maxValue(64)
  maxItems?: int32;
  @minValue(1)
  maxTotalBytes?: int32;
  /// ores-forms binding. When both the policy and the envelope carry a formId
  /// they must match.
  formId?: SafeId;
}

// ---------------------------------------------------------------------------
// Session state machine
// ---------------------------------------------------------------------------

@id("DndSessionInput")
model DndSessionInput {
  kind: DndSessionInputKind;
  /// Required for `start`.
  envelope?: DndEnvelope;
  /// Required for `enter`, `leave` and `drop`.
  targetId?: SafeId;
  /// Required for `enter`.
  policy?: DndDropPolicy;
  /// Optional for `enter` (for example a modifier key requesting copy).
  preferredOperation?: DndOperation;
}

@id("DndSessionSnapshot")
model DndSessionSnapshot {
  state: DndSessionState;
  dragId?: SafeId;
  targetId?: SafeId;
  operation?: DndOperation;
  errorCode?: DndRejectCode;
}

/// A replayable conformance trace: every runtime must produce exactly
/// `expected[i]` after applying `inputs[i]` in order from the idle state.
@id("DndSessionTrace")
model DndSessionTrace {
  @minLength(1)
  @maxLength(128)
  @pattern("^[a-z0-9][a-z0-9._-]{0,127}$")
  id: string;
  @maxLength(512)
  description?: string;
  @minItems(1)
  @maxItems(256)
  inputs: DndSessionInput[];
  @minItems(1)
  @maxItems(256)
  expected: DndSessionSnapshot[];
}
