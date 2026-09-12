import "@typespec/json-schema";
using TypeSpec.JsonSchema;

namespace OresDnd;

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
