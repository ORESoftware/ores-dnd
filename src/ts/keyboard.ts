// Keyboard-accessible drag/drop adapter over the canonical DndSession.
//
// This is intentionally an input adapter, not a second lifecycle model. Hosts
// own focus order, visual presentation and live-region wording; this module
// emits content-free state announcements and the same start/enter/leave/drop/
// cancel inputs used by pointer, HTML5, Flutter and Rust adapters.
import {
  telemetryFor,
  type DndDropResult,
  type DndEnvelope,
  type DndOperation,
  type OresOtelPort,
} from "./codec.js";
import { validatePolicy, type DndDropPolicy, type DndRejectCode } from "./policy.js";
import { DndSession, inputs, isTerminal, type DndSessionSnapshot } from "./session.js";

export type KeyboardAnnouncementKind =
  | "started"
  | "target-accepted"
  | "target-rejected"
  | "target-required"
  | "dropped"
  | "cancelled";

/** Content-free status suitable for mapping onto an ARIA live region. */
export interface KeyboardDndAnnouncement {
  kind: KeyboardAnnouncementKind;
  dragId?: string;
  targetId?: string;
  operation?: DndOperation;
  errorCode?: DndRejectCode;
}

export interface KeyboardDndTarget {
  policy: DndDropPolicy;
}

export interface KeyboardDndOptions {
  /** Content-free lifecycle telemetry. Never receives item data. */
  otel?: OresOtelPort;
  /** Map this structured status to host-specific live-region wording. */
  announce?: (announcement: KeyboardDndAnnouncement) => void;
  /** Host hook for focus/visual navigation; the core never moves focus itself. */
  onTargetChange?: (targetId: string, snapshot: DndSessionSnapshot) => void;
  onDrop?: (envelope: DndEnvelope, result: DndDropResult) => void | Promise<void>;
  onReject?: (envelope: DndEnvelope | null, result: DndDropResult) => void;
}

/** Modifier preference shared with native file-manager conventions. */
export function keyboardPreferredOperation(event: {
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
  metaKey?: boolean;
}): DndOperation | undefined {
  const ctrl = Boolean(event.ctrlKey || event.altKey);
  const shift = Boolean(event.shiftKey);
  if (event.metaKey || (ctrl && shift)) return "link";
  if (ctrl) return "copy";
  if (shift) return "move";
  return undefined;
}

function emit(
  options: KeyboardDndOptions,
  phase: Parameters<typeof telemetryFor>[0],
  envelope: DndEnvelope | null,
  operation?: DndOperation,
  targetId?: string,
): void {
  if (!options.otel || !envelope) return;
  void options.otel.emitDndEvent(telemetryFor(phase, envelope, operation, targetId));
}

function dragIdFields(dragId: string | undefined): { dragId: string } | Record<never, never> {
  return dragId === undefined ? {} : { dragId };
}

/**
 * Framework-neutral keyboard navigation for one DndSession.
 *
 * Targets are kept in host-provided order. `move(+1)` / `move(-1)` wrap; focus
 * remains host-owned through `onTargetChange`. Policy evaluation is performed
 * by DndSession itself, so keyboard behavior cannot bypass cross-runtime rules.
 */
export class KeyboardDndController {
  readonly session: DndSession;
  readonly #targets: DndDropPolicy[];
  readonly #options: KeyboardDndOptions;
  #targetIndex = -1;

  constructor(
    targets: readonly KeyboardDndTarget[],
    session: DndSession = new DndSession(),
    options: KeyboardDndOptions = {},
  ) {
    this.session = session;
    this.#options = options;
    this.#targets = targets.map((target) => validatePolicy(target.policy));
    const ids = new Set<string>();
    for (const target of this.#targets) {
      if (ids.has(target.targetId)) throw new Error(`duplicate keyboard DnD target: ${target.targetId}`);
      ids.add(target.targetId);
    }
  }

  get targetIndex(): number {
    return this.#targetIndex;
  }

  get activeTarget(): DndDropPolicy | null {
    return this.#targetIndex < 0 ? null : (this.#targets[this.#targetIndex] ?? null);
  }

  start(envelope: DndEnvelope): DndSessionSnapshot {
    this.#targetIndex = -1;
    const snapshot = this.session.apply(inputs.start(envelope));
    if (snapshot.state === "dragging" && this.session.envelope) {
      emit(this.#options, "drag-start", this.session.envelope);
      this.#options.announce?.({ kind: "started", ...dragIdFields(snapshot.dragId) });
    }
    return snapshot;
  }

  /** Select the next/previous target in host order and evaluate it. */
  move(step: 1 | -1, preferred?: DndOperation): DndSessionSnapshot {
    const current = this.session.snapshot;
    if (current.state !== "dragging" && current.state !== "over-target") return current;
    if (this.#targets.length === 0) {
      this.#options.announce?.({ kind: "target-required", ...dragIdFields(current.dragId) });
      return current;
    }
    const previous = this.activeTarget;
    if (previous && current.targetId === previous.targetId) this.session.apply(inputs.leave(previous.targetId));
    if (this.#targetIndex < 0) {
      this.#targetIndex = step > 0 ? 0 : this.#targets.length - 1;
    } else {
      this.#targetIndex = (this.#targetIndex + step + this.#targets.length) % this.#targets.length;
    }
    return this.#enterActive(preferred);
  }

  /** Select a target directly while preserving the same session/policy path. */
  select(targetId: string, preferred?: DndOperation): DndSessionSnapshot {
    const current = this.session.snapshot;
    if (current.state !== "dragging" && current.state !== "over-target") return current;
    const index = this.#targets.findIndex((target) => target.targetId === targetId);
    if (index < 0) return current;
    const previous = this.activeTarget;
    if (previous && current.targetId === previous.targetId && previous.targetId !== targetId) {
      this.session.apply(inputs.leave(previous.targetId));
    }
    this.#targetIndex = index;
    return this.#enterActive(preferred);
  }

  drop(): DndDropResult | null {
    const target = this.activeTarget;
    const envelopeBefore = this.session.envelope;
    if (!target) {
      this.#options.announce?.({ kind: "target-required", ...dragIdFields(this.session.snapshot.dragId) });
      return null;
    }
    const snapshot = this.session.apply(inputs.drop(target.targetId));
    const result = this.session.result;
    if (!result) return null;
    if (snapshot.state === "dropped" && envelopeBefore) {
      emit(this.#options, "drop", envelopeBefore, result.operation, target.targetId);
      this.#options.announce?.({
        kind: "dropped",
        dragId: result.dragId,
        targetId: target.targetId,
        ...(result.operation ? { operation: result.operation } : {}),
      });
      void this.#options.onDrop?.(envelopeBefore, result);
    } else {
      this.#options.announce?.({
        kind: "target-rejected",
        dragId: result.dragId,
        targetId: target.targetId,
        ...(result.errorCode ? { errorCode: result.errorCode as DndRejectCode } : {}),
      });
      this.#options.onReject?.(envelopeBefore, result);
    }
    return result;
  }

  cancel(): DndDropResult | null {
    const envelopeBefore = this.session.envelope;
    const before = this.session.snapshot;
    if (before.state === "idle" || isTerminal(before.state)) return this.session.result;
    this.session.apply(inputs.cancel());
    const result = this.session.result;
    if (result) {
      emit(this.#options, "drag-end", envelopeBefore);
      this.#options.announce?.({ kind: "cancelled", dragId: result.dragId, errorCode: "cancelled" });
      this.#options.onReject?.(envelopeBefore, result);
    }
    return result;
  }

  #enterActive(preferred?: DndOperation): DndSessionSnapshot {
    const target = this.activeTarget;
    if (!target) return this.session.snapshot;
    const snapshot = this.session.apply(inputs.enter(target, preferred));
    this.#options.onTargetChange?.(target.targetId, snapshot);
    if (snapshot.state === "over-target") {
      emit(this.#options, "drag-enter", this.session.envelope, snapshot.operation, target.targetId);
      this.#options.announce?.({
        kind: "target-accepted",
        ...dragIdFields(snapshot.dragId),
        targetId: target.targetId,
        ...(snapshot.operation ? { operation: snapshot.operation } : {}),
      });
    } else {
      this.#options.announce?.({
        kind: "target-rejected",
        ...dragIdFields(snapshot.dragId),
        targetId: target.targetId,
        ...(snapshot.errorCode ? { errorCode: snapshot.errorCode } : {}),
      });
    }
    return snapshot;
  }
}

export interface KeyboardBindOptions {
  /** Stop propagation in addition to preventDefault for handled keys. */
  stopPropagation?: boolean;
}

/**
 * Bind Space/Enter, arrows and Escape to a source element. The host controls
 * tabindex/focus styling and live-region rendering; no deprecated aria-grabbed
 * or aria-dropeffect attributes are written.
 */
export function bindKeyboardDragSource(
  el: HTMLElement,
  envelope: DndEnvelope | (() => DndEnvelope),
  controller: KeyboardDndController,
  options: KeyboardBindOptions = {},
): () => void {
  const onKeyDown = (event: KeyboardEvent): void => {
    let handled = true;
    const preferred = keyboardPreferredOperation(event);
    switch (event.key) {
      case " ":
      case "Enter": {
        const state = controller.session.snapshot.state;
        if (state === "idle" || isTerminal(state)) controller.start(typeof envelope === "function" ? envelope() : envelope);
        else if (state === "over-target") controller.drop();
        else controller.move(1, preferred);
        break;
      }
      case "ArrowRight":
      case "ArrowDown":
        controller.move(1, preferred);
        break;
      case "ArrowLeft":
      case "ArrowUp":
        controller.move(-1, preferred);
        break;
      case "Escape":
        controller.cancel();
        break;
      default:
        handled = false;
    }
    if (!handled) return;
    event.preventDefault();
    if (options.stopPropagation) event.stopPropagation();
  };
  el.addEventListener("keydown", onKeyDown);
  return () => el.removeEventListener("keydown", onKeyDown);
}
