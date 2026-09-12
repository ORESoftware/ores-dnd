import {
  BehaviorSubject,
  Subject,
  distinctUntilChanged,
  filter,
  map,
  type Observable,
} from "rxjs";

import {
  type DndEnvelope,
  type DndLifecyclePhase,
  type DndOperation,
  type DndTelemetryEvent,
  telemetryFor,
  validateEnvelope,
} from "./index.js";

/**
 * Process-local reactive event. The envelope may contain dragged data, so this
 * type must not be sent to telemetry, persisted as replay history, or exposed
 * through analytics/crash-reporting adapters.
 */
export interface DndReactiveEvent {
  readonly phase: DndLifecyclePhase;
  readonly envelope: DndEnvelope;
  readonly operation: DndOperation | null;
  readonly targetId: string | null;
}

/**
 * Replay-safe reactive state. Deliberately contains metadata only: no item
 * payload, media contents, form values, or other dragged `data` field.
 */
export interface DndReactiveState {
  readonly active: boolean;
  readonly phase: DndLifecyclePhase | null;
  readonly dragId: string | null;
  readonly sourceRuntime: string | null;
  readonly itemCount: number;
  readonly operation: DndOperation | null;
  readonly targetId: string | null;
}

export const IDLE_DND_REACTIVE_STATE: DndReactiveState = Object.freeze({
  active: false,
  phase: null,
  dragId: null,
  sourceRuntime: null,
  itemCount: 0,
  operation: null,
  targetId: null,
});

const START_PHASES: ReadonlySet<DndLifecyclePhase> = new Set(["drag-start", "drag-enter"]);
const LOSSLESS_PHASES: ReadonlySet<DndLifecyclePhase> = new Set(["drop", "drag-end"]);

/** `drag-over` may be sampled/coalesced for presentation work. */
export function isHighFrequencyLifecyclePhase(phase: DndLifecyclePhase): boolean {
  return phase === "drag-over";
}

/** `drop` and `drag-end` are terminal-significant and must never be throttled away. */
export function isLosslessLifecyclePhase(phase: DndLifecyclePhase): boolean {
  return LOSSLESS_PHASES.has(phase);
}

export function reactiveStateFor(event: DndReactiveEvent): DndReactiveState {
  return Object.freeze({
    active: event.phase !== "drag-end",
    phase: event.phase,
    dragId: event.envelope.dragId,
    sourceRuntime: event.envelope.sourceRuntime,
    itemCount: event.envelope.items.length,
    operation: event.operation,
    targetId: event.targetId,
  });
}

function assertReactiveEventSemantics(event: DndReactiveEvent): void {
  if (event.operation !== null && !event.envelope.allowedOperations.includes(event.operation)) {
    throw new Error("reactive event operation is not source-allowed");
  }
  if (event.targetId !== null && event.targetId.length === 0) {
    throw new Error("reactive event targetId must be a non-empty string");
  }
  if (event.phase === "drop") {
    if (event.operation === null) {
      throw new Error("drop event requires a negotiated operation");
    }
    if (event.targetId === null) {
      throw new Error("drop event requires a targetId");
    }
  }
}

function canTransition(
  previous: DndLifecyclePhase | null,
  next: DndLifecyclePhase,
): boolean {
  if (previous === null) return START_PHASES.has(next);
  switch (previous) {
    case "drag-start":
      return next === "drag-enter" || next === "drag-over" || next === "drop" || next === "drag-end";
    case "drag-enter":
      return next === "drag-over" || next === "drag-leave" || next === "drop" || next === "drag-end";
    case "drag-over":
      return next === "drag-over" || next === "drag-leave" || next === "drop" || next === "drag-end";
    case "drag-leave":
      return next === "drag-enter" || next === "drag-over" || next === "drag-end";
    case "drop":
      return next === "drag-end";
    case "drag-end":
      return START_PHASES.has(next);
  }
}

/**
 * Fail-closed lifecycle tracker shared by browser/webview consumers.
 *
 * A session may begin with `drag-start` for an in-app drag or `drag-enter` for
 * an external/system drag. Once active, events may not silently switch drag IDs.
 * `drop` must be followed only by `drag-end` before a new session begins.
 */
export class DndLifecycleTracker {
  #dragId: string | null = null;
  #phase: DndLifecyclePhase | null = null;

  get dragId(): string | null {
    return this.#dragId;
  }

  get phase(): DndLifecyclePhase | null {
    return this.#phase;
  }

  accept(event: DndReactiveEvent): DndReactiveState {
    assertReactiveEventSemantics(event);

    if (this.#dragId !== null && event.envelope.dragId !== this.#dragId) {
      throw new Error(`reactive dragId switched before drag-end: ${this.#dragId} -> ${event.envelope.dragId}`);
    }
    if (!canTransition(this.#phase, event.phase)) {
      throw new Error(`invalid reactive lifecycle transition: ${this.#phase ?? "idle"} -> ${event.phase}`);
    }

    const state = reactiveStateFor(event);
    if (event.phase === "drag-end") {
      this.#dragId = null;
      this.#phase = null;
    } else {
      this.#dragId = event.envelope.dragId;
      this.#phase = event.phase;
    }
    return state;
  }

  reset(): void {
    this.#dragId = null;
    this.#phase = null;
  }
}

export interface EmitDndReactiveOptions {
  readonly operation?: DndOperation;
  readonly targetId?: string;
}

/**
 * RxJS-backed hot event bus for drag/drop lifecycles.
 *
 * Raw events use a plain Subject and therefore do not replay dragged payloads.
 * Only the metadata-only state stream is replayed with a BehaviorSubject.
 * Persistence/forms remain explicit `commitAcceptedDrop` effects and are never
 * triggered by `emit`.
 */
export class OresDndReactiveBus {
  readonly #events = new Subject<DndReactiveEvent>();
  readonly #state = new BehaviorSubject<DndReactiveState>(IDLE_DND_REACTIVE_STATE);
  readonly #tracker = new DndLifecycleTracker();

  readonly events$: Observable<DndReactiveEvent> = this.#events.asObservable();
  readonly state$: Observable<DndReactiveState> = this.#state.asObservable();
  readonly active$: Observable<boolean> = this.state$.pipe(
    map((state) => state.active),
    distinctUntilChanged(),
  );
  readonly drops$: Observable<DndReactiveEvent> = this.events$.pipe(
    filter((event) => event.phase === "drop"),
  );
  readonly dragOvers$: Observable<DndReactiveEvent> = this.events$.pipe(
    filter((event) => isHighFrequencyLifecyclePhase(event.phase)),
  );
  readonly lossless$: Observable<DndReactiveEvent> = this.events$.pipe(
    filter((event) => isLosslessLifecyclePhase(event.phase)),
  );
  readonly telemetry$: Observable<DndTelemetryEvent> = this.events$.pipe(
    map((event) => telemetryFor(
      event.phase,
      event.envelope,
      event.operation ?? undefined,
      event.targetId ?? undefined,
    )),
  );

  emit(
    phase: DndLifecyclePhase,
    envelope: DndEnvelope,
    options: EmitDndReactiveOptions = {},
  ): void {
    const safeEnvelope = validateEnvelope(envelope);
    const event: DndReactiveEvent = Object.freeze({
      phase,
      envelope: safeEnvelope,
      operation: options.operation ?? null,
      targetId: options.targetId ?? null,
    });
    const state = this.#tracker.accept(event);
    this.#events.next(event);
    this.#state.next(state);
  }

  complete(): void {
    this.#events.complete();
    this.#state.complete();
    this.#tracker.reset();
  }
}
