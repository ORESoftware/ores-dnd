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
// Payload
// ---------------------------------------------------------------------------

@id("DndItem")
model DndItem {
  kind: DndItemKind;
  mediaType: string;
  data: string;
  name?: string;
}

@id("DndEnvelope")
model DndEnvelope {
  protocol: string;
  dragId: string;
  sourceRuntime: string;
  allowedOperations: DndOperation[];
  items: DndItem[];
  traceparent?: string;
  formId?: string;
}

@id("DndDropResult")
model DndDropResult {
  dragId: string;
  accepted: boolean;
  operation?: DndOperation;
  targetId?: string;
  errorCode?: string;
}

@id("DndTelemetryEvent")
model DndTelemetryEvent {
  phase: DndLifecyclePhase;
  dragId: string;
  sourceRuntime: string;
  @minValue(0)
  itemCount: int32;
  operation?: DndOperation;
  targetId?: string;
}

// ---------------------------------------------------------------------------
// Target policy
// ---------------------------------------------------------------------------

/// What a drop target is willing to accept. Declared by the target, evaluated by
/// every runtime with the same rules, and never mutated by a drag.
@id("DndDropPolicy")
model DndDropPolicy {
  targetId: string;
  allowedOperations: DndOperation[];
  acceptedKinds: DndItemKind[];
  /// Exact media types or `type/*` wildcards. Absent means any media type.
  acceptedMediaTypes?: string[];
  @minValue(1)
  maxItems?: int32;
  @minValue(1)
  maxTotalBytes?: int32;
  /// ores-forms binding. When both the policy and the envelope carry a formId
  /// they must match.
  formId?: string;
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
  targetId?: string;
  /// Required for `enter`.
  policy?: DndDropPolicy;
  /// Optional for `enter` (for example a modifier key requesting copy).
  preferredOperation?: DndOperation;
}

@id("DndSessionSnapshot")
model DndSessionSnapshot {
  state: DndSessionState;
  dragId?: string;
  targetId?: string;
  operation?: DndOperation;
  errorCode?: DndRejectCode;
}

/// A replayable conformance trace: every runtime must produce exactly
/// `expected[i]` after applying `inputs[i]` in order from the idle state.
@id("DndSessionTrace")
model DndSessionTrace {
  id: string;
  description?: string;
  inputs: DndSessionInput[];
  expected: DndSessionSnapshot[];
}
