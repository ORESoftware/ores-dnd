// Pointer-events fallback for touch surfaces and webviews without native HTML5
// drag-and-drop. Produces exactly the same session inputs as dom.ts, so zones
// bound with bindDropZone and zones registered here share one state machine.
import type { DndDropResult, DndEnvelope, OresOtelPort } from "./codec.js";
import { telemetryFor } from "./codec.js";
import { validatePolicy, type DndDropPolicy } from "./policy.js";
import { inputs, type DndSession } from "./session.js";
import { ATTR_POLICY, ATTR_STATE, ATTR_ZONE, EVENT_DROP, preferredOperation, zoneStateAttribute } from "./dom.js";

export interface RegisteredZone {
  el: Element;
  policy: DndDropPolicy;
}

/** Drop zones reachable by pointer hit-testing. */
export class ZoneRegistry {
  readonly #zones = new Map<Element, RegisteredZone>();

  register(el: Element, policy: DndDropPolicy): () => void {
    const validated = validatePolicy(policy);
    el.setAttribute(ATTR_ZONE, validated.targetId);
    if (!el.hasAttribute(ATTR_POLICY)) el.setAttribute(ATTR_POLICY, JSON.stringify(validated));
    if (!el.hasAttribute(ATTR_STATE)) el.setAttribute(ATTR_STATE, "idle");
    this.#zones.set(el, { el, policy: validated });
    return () => {
      this.#zones.delete(el);
    };
  }

  get size(): number {
    return this.#zones.size;
  }

  /** The innermost registered zone containing `el`, if any. */
  zoneFor(el: Element | null): RegisteredZone | null {
    for (let node: Element | null = el; node; node = node.parentElement) {
      const zone = this.#zones.get(node);
      if (zone) return zone;
    }
    return null;
  }

  /** Hit-test a point (`elementFromPoint` on `doc`). */
  zoneAt(doc: Document, x: number, y: number): RegisteredZone | null {
    return this.zoneFor(doc.elementFromPoint(x, y));
  }

  /** Reflect a snapshot onto every registered zone's state attribute. */
  syncStates(session: DndSession): void {
    for (const zone of this.#zones.values()) {
      zone.el.setAttribute(ATTR_STATE, zoneStateAttribute(session.snapshot, zone.policy.targetId));
    }
  }
}

export interface PointerDragOptions {
  /** Pixels the pointer must travel before a drag starts (default 8). */
  threshold?: number;
  doc?: Document;
  otel?: OresOtelPort;
  onDrop?: (envelope: DndEnvelope, result: DndDropResult, zone: RegisteredZone) => void | Promise<void>;
  onReject?: (result: DndDropResult) => void;
  /** Attribute toggled on the source while dragging (default `data-ores-dnd-dragging`). */
  draggingAttribute?: string;
}

/**
 * Make `el` a pointer-driven drag source for `envelope`, dropping onto zones
 * in `registry`. Returns an unbind function.
 */
export function bindPointerDragSource(
  el: HTMLElement,
  envelope: DndEnvelope | (() => DndEnvelope),
  session: DndSession,
  registry: ZoneRegistry,
  options: PointerDragOptions = {},
): () => void {
  const threshold = options.threshold ?? 8;
  const draggingAttribute = options.draggingAttribute ?? "data-ores-dnd-dragging";
  let pointerId: number | null = null;
  let startX = 0;
  let startY = 0;
  let dragging = false;
  let currentZone: RegisteredZone | null = null;

  const emit = (phase: Parameters<typeof telemetryFor>[0], operation?: DndDropResult["operation"], targetId?: string): void => {
    if (options.otel && session.envelope) void options.otel.emitDndEvent(telemetryFor(phase, session.envelope, operation, targetId));
  };
  const doc = (): Document => options.doc ?? el.ownerDocument;
  const finish = (): void => {
    dragging = false;
    pointerId = null;
    currentZone = null;
    el.removeAttribute(draggingAttribute);
    registry.syncStates(session);
  };

  const onDown = (event: PointerEvent): void => {
    if (event.button !== 0 || pointerId !== null) return;
    pointerId = event.pointerId;
    startX = event.clientX;
    startY = event.clientY;
    dragging = false;
    el.setPointerCapture?.(event.pointerId);
  };
  const onMove = (event: PointerEvent): void => {
    if (event.pointerId !== pointerId) return;
    if (!dragging) {
      if (Math.hypot(event.clientX - startX, event.clientY - startY) < threshold) return;
      const current = typeof envelope === "function" ? envelope() : envelope;
      const snapshot = session.apply(inputs.start(current));
      if (snapshot.state !== "dragging") {
        pointerId = null;
        return;
      }
      dragging = true;
      el.setAttribute(draggingAttribute, "true");
      emit("drag-start");
    }
    event.preventDefault();
    const zone = registry.zoneAt(doc(), event.clientX, event.clientY);
    if (zone !== currentZone) {
      if (currentZone) {
        const wasAccepting = session.snapshot.state === "over-target";
        session.apply(inputs.leave(currentZone.policy.targetId));
        if (wasAccepting) emit("drag-leave", undefined, currentZone.policy.targetId);
      }
      currentZone = zone;
      if (zone) {
        const next = session.apply(inputs.enter(zone.policy, preferredOperation(event)));
        if (next.state === "over-target") emit("drag-enter", next.operation, zone.policy.targetId);
      }
    }
    registry.syncStates(session);
  };
  const onUp = (event: PointerEvent): void => {
    if (event.pointerId !== pointerId) return;
    el.releasePointerCapture?.(event.pointerId);
    if (!dragging) {
      pointerId = null;
      return;
    }
    const zone = currentZone;
    if (zone) {
      const snapshot = session.apply(inputs.drop(zone.policy.targetId));
      const result = session.result;
      if (snapshot.state === "dropped" && result && session.envelope) {
        emit("drop", result.operation, zone.policy.targetId);
        zone.el.dispatchEvent(new CustomEvent(EVENT_DROP, { detail: { envelope: session.envelope, result }, bubbles: true }));
        void options.onDrop?.(session.envelope, result, zone);
      } else if (result) {
        options.onReject?.(result);
      }
    } else {
      session.apply(inputs.end());
      emit("drag-end");
      if (session.result) options.onReject?.(session.result);
    }
    finish();
  };
  const onCancel = (event: PointerEvent): void => {
    if (event.pointerId !== pointerId) return;
    if (dragging) {
      session.apply(inputs.cancel());
      emit("drag-end");
    }
    finish();
  };

  el.addEventListener("pointerdown", onDown);
  el.addEventListener("pointermove", onMove);
  el.addEventListener("pointerup", onUp);
  el.addEventListener("pointercancel", onCancel);
  el.style.touchAction = "none";
  return () => {
    el.removeEventListener("pointerdown", onDown);
    el.removeEventListener("pointermove", onMove);
    el.removeEventListener("pointerup", onUp);
    el.removeEventListener("pointercancel", onCancel);
  };
}
