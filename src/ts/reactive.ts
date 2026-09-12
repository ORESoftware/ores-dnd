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
    const event: DndReactiveEvent = Object.freeze({
      phase,
      envelope: safeEnvelope,
      operation: options.operation ?? null,
      targetId: options.targetId ?? null,
    });
    this.#events.next(event);
    this.#state.next(reactiveStateFor(event));
  }

  complete(): void {
    this.#events.complete();
    this.#state.complete();
  }
}
