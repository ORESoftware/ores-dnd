// HTML5 drag-and-drop bindings: translate DOM drag events into session inputs
// and keep `data-ores-dnd-state` in sync for CSS. Mirrors
// src/rust-leptos/src/browser.rs so Rust islands and TypeScript pages interoperate.
import {
  ORES_DND_MIME,
  ORES_DND_PROTOCOL,
  decodeEnvelope,
  encodeEnvelope,
  effectAllowedFor,
  telemetryFor,
  type DndDropResult,
  type DndEnvelope,
  type DndItem,
  type DndItemKind,
  type DndOperation,
  type OresOtelPort,
  type ValidationOptions,
} from "./codec.js";
import { validatePolicy, type DndDropPolicy } from "./policy.js";
import { DndSession, inputs, type DndSessionSnapshot } from "./session.js";
import { ENVELOPE_ITEMS_MAX, isMediaType } from "./wire.js";

/** Attribute names shared with `ores_dnd_core::bindings` and `ores-dnd-mash`. */
export const ATTR_ZONE = "data-ores-dnd-zone";
export const ATTR_POLICY = "data-ores-dnd-policy";
export const ATTR_STATE = "data-ores-dnd-state";
export const ATTR_SOURCE = "data-ores-dnd-source";
export const ATTR_COMMIT = "data-ores-dnd-commit";
export const ATTR_SWAP = "data-ores-dnd-swap";
/** CustomEvent dispatched on a zone after a drop (`detail: { envelope, result }`). */
export const EVENT_DROP = "ores-dnd:drop";
/** CustomEvent dispatched on a zone when the session snapshot changes (`detail: snapshot`). */
export const EVENT_STATE = "ores-dnd:state";
/** CustomEvent dispatched on a zone after a server commit (`detail: { envelope, result }`). */
export const EVENT_COMMITTED = "ores-dnd:committed";

export type ZoneStateAttribute = "idle" | "dragging" | "accepting" | "rejecting" | "dropped";

/** The `data-ores-dnd-state` value for a zone: reflects the session only while this zone is the active target. */
export function zoneStateAttribute(snapshot: DndSessionSnapshot, targetId: string): ZoneStateAttribute {
  const mine = snapshot.targetId === targetId;
  switch (snapshot.state) {
    case "over-target":
      return mine ? "accepting" : "dragging";
    case "dragging":
      return mine ? "rejecting" : "dragging";
    case "dropped":
      return mine ? "dropped" : "idle";
    default:
      return "idle";
  }
}

/** Modifier keys express a preference like native file managers: Ctrl/⌥ → copy, Shift → move, Ctrl+Shift/⌘ → link. */
export function preferredOperation(event: { ctrlKey?: boolean; altKey?: boolean; shiftKey?: boolean; metaKey?: boolean }): DndOperation | undefined {
  const ctrl = Boolean(event.ctrlKey || event.altKey);
  const shift = Boolean(event.shiftKey);
  if (event.metaKey || (ctrl && shift)) return "link";
  if (ctrl) return "copy";
  if (shift) return "move";
  return undefined;
}

/**
 * The item kind and canonical media type implied by a DataTransfer format while
 * its data is unreadable. Browser formats are not always media types (`Files`,
 * `downloadurl`, …): anything that is not a canonical `type/subtype` is reported
 * as `application/octet-stream` bytes. The ores MIME itself maps to null.
 */
export function kindForType(format: string): { kind: DndItemKind; mediaType: string } | null {
  const media = (format.split(";")[0] ?? "").trim().toLowerCase();
  if (media === ORES_DND_MIME) return null;
  if (!isMediaType(media)) return { kind: "bytes", mediaType: "application/octet-stream" };
  if (media === "text/uri-list") return { kind: "uri", mediaType: media };
  if (media === "application/json") return { kind: "json", mediaType: media };
  if (media.startsWith("text/")) return { kind: "text", mediaType: media };
  return { kind: "bytes", mediaType: media };
}

let externalCounter = 0;

/**
 * An envelope describing an external drag by its advertised types only (data
 * is empty until `drop`). Lets a zone evaluate kind/media-type rules during
 * `dragover`; byte limits are re-checked definitively on drop.
 */
export function provisionalEnvelope(types: readonly string[]): DndEnvelope {
  externalCounter += 1;
  const items: DndItem[] = [];
  for (const type of types) {
    const mapped = kindForType(type);
    if (mapped && items.length < ENVELOPE_ITEMS_MAX) items.push({ kind: mapped.kind, mediaType: mapped.mediaType, data: "" });
  }
  if (items.length === 0) items.push({ kind: "text", mediaType: "text/plain", data: "" });
  return { protocol: ORES_DND_PROTOCOL, dragId: `external-${externalCounter}`, sourceRuntime: "external-browser", allowedOperations: ["copy"], items };
}

/** Read the ores envelope, or synthesize one from a `text/plain` drop. Returns null when neither is present. */
export function readEnvelope(transfer: DataTransfer, options: ValidationOptions = {}): DndEnvelope | null {
  const json = transfer.getData(ORES_DND_MIME);
  if (json) return decodeEnvelope(json, options);
  const text = transfer.getData("text/plain");
  if (!text) return null;
  externalCounter += 1;
  return {
    protocol: ORES_DND_PROTOCOL,
    dragId: `external-text-${externalCounter}`,
    sourceRuntime: "external-browser",
    allowedOperations: ["copy"],
    items: [{ kind: "text", mediaType: "text/plain", data: text }],
  };
}

/** Write the ores MIME payload, `effectAllowed`, and a `text/plain` fallback. */
export function writeEnvelope(transfer: DataTransfer, envelope: DndEnvelope, options: ValidationOptions = {}): void {
  const json = encodeEnvelope(envelope, options);
  transfer.setData(ORES_DND_MIME, json);
  transfer.effectAllowed = effectAllowedFor(envelope.allowedOperations);
  const text = envelope.items.find((item) => item.kind === "text" && item.mediaType === "text/plain");
  if (text) transfer.setData("text/plain", text.data);
}

export interface BindOptions {
  validation?: ValidationOptions;
  /** Content-free lifecycle telemetry (ores-otel). */
  otel?: OresOtelPort;
}

export interface ZoneHandlers {
  /** Called after an accepted drop with the verified envelope and terminal result. */
  onDrop?: (envelope: DndEnvelope, result: DndDropResult) => void | Promise<void>;
  /** Called after a drop that ended the session without acceptance. */
  onReject?: (envelope: DndEnvelope | null, result: DndDropResult) => void;
}

function emit(options: BindOptions | undefined, phase: Parameters<typeof telemetryFor>[0], envelope: DndEnvelope | null, operation?: DndOperation, targetId?: string): void {
  if (!options?.otel || !envelope) return;
  void options.otel.emitDndEvent(telemetryFor(phase, envelope, operation, targetId));
}

/** Make `el` a drag source for `envelope` (or a factory called on each dragstart). Returns an unbind function. */
export function bindDragSource(
  el: HTMLElement,
  envelope: DndEnvelope | (() => DndEnvelope),
  session: DndSession,
  options: BindOptions = {},
): () => void {
  el.setAttribute("draggable", "true");
  const onDragStart = (event: DragEvent): void => {
    const current = typeof envelope === "function" ? envelope() : envelope;
    if (!event.dataTransfer) return;
    try {
      writeEnvelope(event.dataTransfer, current, options.validation);
    } catch {
      event.preventDefault();
      return;
    }
    session.apply(inputs.start(current));
    emit(options, "drag-start", session.envelope);
  };
  const onDragEnd = (): void => {
    const envelopeBefore = session.envelope;
    session.apply(inputs.end());
    emit(options, "drag-end", envelopeBefore);
  };
  el.addEventListener("dragstart", onDragStart);
  el.addEventListener("dragend", onDragEnd);
  return () => {
    el.removeEventListener("dragstart", onDragStart);
    el.removeEventListener("dragend", onDragEnd);
  };
}

/** Make `el` a drop zone governed by `policy`. Returns an unbind function. */
export function bindDropZone(
  el: HTMLElement,
  policy: DndDropPolicy,
  session: DndSession,
  handlers: ZoneHandlers = {},
  options: BindOptions = {},
): () => void {
  const validated = validatePolicy(policy);
  const targetId = validated.targetId;
  el.setAttribute(ATTR_ZONE, targetId);
  if (!el.hasAttribute(ATTR_POLICY)) el.setAttribute(ATTR_POLICY, JSON.stringify(validated));
  el.setAttribute(ATTR_STATE, zoneStateAttribute(session.snapshot, targetId));
  const unsubscribe = session.subscribe((snapshot) => {
    el.setAttribute(ATTR_STATE, zoneStateAttribute(snapshot, targetId));
    el.dispatchEvent(new CustomEvent(EVENT_STATE, { detail: snapshot, bubbles: true }));
  });

  const enter = (event: DragEvent): void => {
    const preferred = preferredOperation(event);
    if (session.snapshot.state === "idle" && event.dataTransfer) {
      session.apply(inputs.start(provisionalEnvelope(Array.from(event.dataTransfer.types ?? []))));
    }
    const wasOver = session.snapshot.state === "over-target" && session.snapshot.targetId === targetId;
    const next = session.apply(inputs.enter(validated, preferred));
    if (next.state === "over-target" && next.targetId === targetId) {
      event.preventDefault();
      if (event.dataTransfer && next.operation) event.dataTransfer.dropEffect = next.operation;
      if (!wasOver) emit(options, "drag-enter", session.envelope, next.operation, targetId);
    }
  };
  const leave = (event: DragEvent): void => {
    const related = event.relatedTarget as Node | null;
    if (related && el.contains(related)) return; // moved into a child, still inside the zone
    const wasAccepting = session.snapshot.state === "over-target" && session.snapshot.targetId === targetId;
    session.apply(inputs.leave(targetId));
    if (wasAccepting) emit(options, "drag-leave", session.envelope, undefined, targetId);
  };
  const drop = (event: DragEvent): void => {
    event.preventDefault();
    const preferred = preferredOperation(event);
    if (event.dataTransfer) {
      let real: DndEnvelope | null = null;
      try {
        real = readEnvelope(event.dataTransfer, options.validation);
      } catch {
        real = null;
      }
      // The payload is readable now: replace a provisional session with the real one.
      if (real && session.envelope?.dragId !== real.dragId) {
        session.apply(inputs.start(real));
        session.apply(inputs.enter(validated, preferred));
      }
    }
    const snapshot = session.apply(inputs.drop(targetId));
    const result = session.result;
    if (!result) return;
    if (snapshot.state === "dropped" && session.envelope) {
      emit(options, "drop", session.envelope, result.operation, targetId);
      el.dispatchEvent(new CustomEvent(EVENT_DROP, { detail: { envelope: session.envelope, result }, bubbles: true }));
      void handlers.onDrop?.(session.envelope, result);
    } else {
      handlers.onReject?.(session.envelope, result);
    }
  };
  el.addEventListener("dragenter", enter);
  el.addEventListener("dragover", enter);
  el.addEventListener("dragleave", leave);
  el.addEventListener("drop", drop);
  return () => {
    unsubscribe();
    el.removeEventListener("dragenter", enter);
    el.removeEventListener("dragover", enter);
    el.removeEventListener("dragleave", leave);
    el.removeEventListener("drop", drop);
  };
}

export interface AutoBindOptions extends BindOptions {
  session?: DndSession;
  /** Called for zones carrying `data-ores-dnd-commit`; defaults to the htmx/fetch commit in `htmx.ts`. */
  commit?: (zone: HTMLElement, envelope: DndEnvelope, result: DndDropResult) => Promise<void>;
}

/**
 * Bind every `[data-ores-dnd-source]` and `[data-ores-dnd-zone][data-ores-dnd-policy]`
 * under `root` — the HTML-first entry point used by MASH pages
 * (`ores_dnd_mash::html::boot_script`). Returns an unbind function.
 */
export function autoBind(root: ParentNode, options: AutoBindOptions = {}): () => void {
  const session = options.session ?? new DndSession(options.validation);
  const unbinders: Array<() => void> = [];
  for (const el of Array.from(root.querySelectorAll<HTMLElement>(`[${ATTR_SOURCE}]`))) {
    try {
      const envelope = decodeEnvelope(el.getAttribute(ATTR_SOURCE) ?? "", options.validation);
      unbinders.push(bindDragSource(el, envelope, session, options));
    } catch {
      el.setAttribute("draggable", "false");
    }
  }
  for (const el of Array.from(root.querySelectorAll<HTMLElement>(`[${ATTR_ZONE}][${ATTR_POLICY}]`))) {
    try {
      const policy = validatePolicy(JSON.parse(el.getAttribute(ATTR_POLICY) ?? "null"));
      const handlers: ZoneHandlers = {};
      if (el.hasAttribute(ATTR_COMMIT)) {
        handlers.onDrop = async (envelope, result) => {
          const commit = options.commit ?? (await import("./htmx.js")).commitFromZone;
          await commit(el, envelope, result);
        };
      }
      unbinders.push(bindDropZone(el, policy, session, handlers, options));
    } catch {
      el.setAttribute(ATTR_STATE, "idle");
    }
  }
  return () => {
    for (const unbind of unbinders) unbind();
  };
}
