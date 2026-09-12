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

export interface DndReactiveEvent {
  readonly phase: DndLifecyclePhase;
  readonly envelope: DndEnvelope;
  readonly operation: DndOperation | null;
  readonly targetId: string | null;
}

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

export type DndLifecycleMode = "strict" | "external-drop-compatible";

/** Stateful validator for one drag lifecycle at a time. */
export class DndLifecycleGuard {
  #activeDragId: string | null = null;
  #dropped = false;

  constructor(readonly mode: DndLifecycleMode = "strict") {}

  get active(): boolean {
    return this.#activeDragId !== null;
  }

  get activeDragId(): string | null {
    return this.#activeDragId;
  }

  accept(phase: DndLifecyclePhase, dragId: string): void {
    if (this.#activeDragId === null) {
      if (phase === "drag-start") {
        this.#activeDragId = dragId;
        this.#dropped = false;
        return;
      }
      if (phase === "drop" && this.mode === "external-drop-compatible") {
        return;
      }
      throw new Error(`${phase} requires an active drag-start`);
    }

    if (dragId !== this.#activeDragId) {
      throw new Error(`reactive lifecycle dragId changed before drag-end`);
    }
    if (this.#dropped) {
      if (phase !== "drag-end") {
        throw new Error(`${phase} is invalid after drop; expected drag-end`);
      }
      this.#activeDragId = null;
      this.#dropped = false;
      return;
    }

    switch (phase) {
      case "drag-start":
        throw new Error("duplicate drag-start before drag-end");
      case "drag-enter":
      case "drag-over":
      case "drag-leave":
        return;
      case "drop":
        this.#dropped = true;
        return;
      case "drag-end":
        this.#activeDragId = null;
        this.#dropped = false;
        return;
    }
}

export interface EmitDndReactiveOptions {
  readonly operation?: DndOperation;
  readonly targetId?: string;
}

export interface OresDndReactiveBusOptions {
  readonly lifecycleMode?: DndLifecycleMode;
}

export class OresDndReactiveBus {
  readonly #events = new Subject<DndReactiveEvent>();
  readonly #state = new BehaviorSubject<DndReactiveState>(IDLE_DND_REACTIVE_STATE);
  readonly #guard: DndLifecycleGuard;

  constructor(options: OresDndReactiveBusOptions = {}) {
    this.#guard = new DndLifecycleGuard(options.lifecycleMode ?? "strict");
  }

  readonly events$: Observable<DndReactiveEvent> = this.#events.asObservable();
  readonly state$: Observable<DndReactiveState> = this.#state.asObservable();
  readonly active$: Observable<boolean> = this.state$.pipe(
    map((state) => state.active),
    distinctUntilChanged(),
  );
  readonly drops$: Observable<DndReactiveEvent> = this.events$.pipe(
    filter((event) => event.phase === "drop"),
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
    if (options.operation !== undefined && !safeEnvelope.allowedOperations.includes(options.operation)) {
      throw new Error("reactive event operation is not source-allowed");
    }
    if (options.targetId !== undefined && options.targetId.length === 0) {
      throw new Error("reactive event targetId must be a non-empty string");
    }
    this.#guard.accept(phase, safeEnvelope.dragId);

    const event: DndReactiveEvent = Object.freeze({
      phase,
      envelope: safeEnvelope,
      operation: options.operation ?? null,
      targetId: options.targetId ?? null,
    });
    this.#events.next(event);
    const projected = reactiveStateFor(event);
    this.#state.next(Object.freeze({ ...projected, active: this.#guard.active }));
  }

  complete(): void {
    this.#events.complete();
    this.#state.complete();
  }
}
