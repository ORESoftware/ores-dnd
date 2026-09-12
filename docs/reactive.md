# Reactive / FRP drag-and-drop surfaces

`ores-dnd` exposes the same drag/drop lifecycle as hot ReactiveX-style streams in the three primary client runtimes:

| Runtime | Dependency | ORES surface |
| --- | --- | --- |
| TypeScript | RxJS 7.8.2 | `@oresoftware/ores-dnd/reactive` |
| Dart / Flutter | RxDart 0.28.x | `package:ores_dnd/ores_dnd_reactive.dart` |
| Rust / WASM | rxRust 1.0.0-rc.5 | `ores_dnd_core::reactive` |

“RxTS” in ORES architecture means the typed TypeScript ReactiveX surface backed by RxJS; there is no second competing TypeScript stream runtime or duplicate event schema.

## One semantic model

All runtimes distinguish two stream classes:

1. **Raw lifecycle events** carry a validated `DndEnvelope`. They are process-local, hot, and non-replaying. A late subscriber never receives an earlier dragged payload.
2. **Current reactive state and telemetry** contain metadata only: lifecycle phase, drag id, source runtime, item count, negotiated operation, and target id. They never contain `DndItem.data`.

The lifecycle phases remain the authored `ores.dnd/v1` phases:

`drag-start → drag-enter / drag-over / drag-leave → drop → drag-end`

`drag-end` is the state transition that sets `active = false`. `drop` can still be observed while the drag is active because native runtimes commonly deliver a final drag-end after the drop callback.

## Side-effect boundary

Reactive emission does **not** persist data or mutate forms. Storage and form effects continue to flow only through the explicit accepted-drop commit ports:

- `OresFormsPort`
- `OptoSyncPort`
- `OresOtelPort`

This keeps stream composition pure by default. Consumers may transform, combine, filter, or observe events without silently writing IndexedDB, SQLite, Postgres, form state, analytics, or telemetry.

## Replay and privacy rules

- Never use an unbounded replay subject for `DndEnvelope` events.
- Never place `items[].data` in a `BehaviorSubject`, `ReplaySubject`, state store, trace, metric, log, crash report, or analytics event.
- Replay/current-value streams may store only the metadata-only reactive state.
- Telemetry derives from the existing content-free `DndTelemetryEvent` projection.
- Validate/decode before emitting into the bus; a malformed external payload is a codec failure, not a reason to terminate the long-lived UI event stream.

## High-frequency drag-over events

`drag-over` can fire much faster than a UI should repaint. Consumers may sample/debounce/throttle **derived presentation streams**, but should not mutate the canonical raw event bus.

Recommended split:

- raw lifecycle stream: lossless within the process;
- presentation/hover stream: may use `auditTime`, `sample`, `throttle`, or the runtime equivalent;
- `drop` and `drag-end`: never throttled away;
- persistence: only after an accepted drop through `commitAcceptedDrop`.

This keeps pointer-frequency noise from driving expensive work without making terminal lifecycle events disappear.

## TypeScript

```ts
import { OresDndReactiveBus } from "@oresoftware/ores-dnd/reactive";

const dnd = new OresDndReactiveBus();
const stateSubscription = dnd.state$.subscribe(renderDropState);
const dropSubscription = dnd.drops$.subscribe(handleDropIntent);

// UI adapter only emits. It does not persist anything by itself.
dnd.emit("drag-over", envelope, { operation: "copy", targetId: "files" });

stateSubscription.unsubscribe();
dropSubscription.unsubscribe();
dnd.complete();
```

## Dart / Flutter

```dart
import 'package:ores_dnd/ores_dnd_reactive.dart';

final dnd = OresDndReactiveBus();
final stateSub = dnd.state.listen(renderDropState);
final dropSub = dnd.drops.listen(handleDropIntent);

// ...
await stateSub.cancel();
await dropSub.cancel();
await dnd.dispose();
```

Own subscriptions at the widget/controller lifecycle boundary and always cancel them before disposal.

## Rust / WASM

```rust
use ores_dnd_core::reactive::{local_event_subject, reactive_state_for, rx::*};

let mut dnd = local_event_subject();
let state_subscription = dnd
    .clone()
    .map(|event| reactive_state_for(&event))
    .subscribe(render_drop_state);

// Keep `state_subscription` owned by the view/controller lifetime.
```

Use rxRust `Local` for WASM/UI-thread work. Choose `Shared` only when a native application intentionally crosses threads; do not add locking merely to imitate another runtime.
