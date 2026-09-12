# Reactive / FRP drag-and-drop surfaces

`ores-dnd` exposes the same drag/drop lifecycle as hot ReactiveX-style streams in the three primary client runtimes:

| Runtime | Dependency | ORES surface |
| --- | --- | --- |
| TypeScript | RxJS 7.8.2 | `@oresoftware/ores-dnd/reactive` / `@oresoftware/ores-dnd/rxts` |
| Dart / Flutter | RxDart 0.28.0 | `package:ores_dnd/ores_dnd_reactive.dart` |
| Rust / WASM | rxRust 1.0.0-rc.5 | `ores_dnd_core::reactive` |

“RxTS” in ORES architecture means the typed TypeScript ReactiveX surface backed by RxJS; there is no second competing TypeScript stream runtime or duplicate event schema.

## One semantic model

All runtimes distinguish two stream classes:

1. **Raw lifecycle events** carry a validated `DndEnvelope`. They are process-local, hot, and non-replaying. A late subscriber never receives an earlier dragged payload.
2. **Current reactive state and telemetry** contain metadata only: lifecycle phase, drag id, source runtime, item count, negotiated operation, and target id. They never contain `DndItem.data`.

The lifecycle phases remain the authored `ores.dnd/v1` phases, but the reactive layer now admits them through a fail-closed state machine rather than accepting arbitrary ordering.

A local/in-app session begins with `drag-start`. An external/system drag may begin with `drag-enter`. Once active, a session cannot silently switch `dragId`. The accepted transition graph is:

```text
idle       -> drag-start | drag-enter
drag-start -> drag-enter | drag-over | drop | drag-end
drag-enter -> drag-over | drag-leave | drop | drag-end
drag-over  -> drag-over | drag-leave | drop | drag-end
drag-leave -> drag-enter | drag-over | drag-end
drop       -> drag-end
```

`drop` must carry both a negotiated operation and a target id, and the operation must be present in the source envelope's `allowedOperations`. `drag-end` resets lifecycle admission to idle while the replayable state keeps the final metadata with `active = false`.

This state machine is deliberately stricter than raw DOM callbacks. Browser/native adapters normalize their callbacks into one ORES session before emitting. Invalid ordering is rejected at the reactive boundary instead of leaking ambiguous state into application logic.

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
- A rejected reactive event must not be emitted downstream and must not advance replayable state.

## High-frequency drag-over events

`drag-over` can fire much faster than a UI should repaint. Consumers may sample/debounce/throttle **derived presentation streams**, but must not mutate the canonical raw event bus.

The cross-runtime classification is explicit:

- **high-frequency/presentation:** `drag-over`;
- **lossless terminal-significant:** `drop`, `drag-end`.

Recommended split:

- raw lifecycle stream: lossless within the process;
- presentation/hover stream: may use `auditTime`, `sample`, `throttle`, or the runtime equivalent;
- lossless terminal stream: never debounced, sampled, or dropped;
- persistence: only after an accepted drop through `commitAcceptedDrop`.

Do not put `drop` and `drag-end` behind the same throttle/debounce operator used for `drag-over`. If a consumer needs frame-rate rendering, split the stream first and throttle only the presentation branch.

## Subscription ownership and reentrancy

UI/controller code owns its subscriptions and must unsubscribe/cancel them during lifecycle teardown. Rust/WASM consumers should retain rxRust subscription handles for the owning view/controller lifetime.

Do not synchronously feed an event back into the same reactive subject from inside its own callback. rxRust subjects reject re-entrant emission by design, and ORES treats that as the cross-runtime policy. If a workflow intentionally feeds back, create an explicit asynchronous boundary and document the bounded queue/scheduler involved.

## TypeScript

```ts
import { OresDndReactiveBus } from "@oresoftware/ores-dnd/reactive";

const dnd = new OresDndReactiveBus();
const stateSubscription = dnd.state$.subscribe(renderDropState);
const hoverSubscription = dnd.dragOvers$.subscribe(renderHoverState);
const terminalSubscription = dnd.lossless$.subscribe(handleTerminalLifecycle);

// A local session starts explicitly.
dnd.emit("drag-start", envelope);
dnd.emit("drag-over", envelope, { targetId: "files" });
dnd.emit("drop", envelope, { operation: "copy", targetId: "files" });
dnd.emit("drag-end", envelope, { operation: "copy", targetId: "files" });

stateSubscription.unsubscribe();
hoverSubscription.unsubscribe();
terminalSubscription.unsubscribe();
dnd.complete();
```

## Dart / Flutter

```dart
import 'package:ores_dnd/ores_dnd_reactive.dart';

final dnd = OresDndReactiveBus();
final stateSub = dnd.state.listen(renderDropState);
final hoverSub = dnd.dragOvers.listen(renderHoverState);
final terminalSub = dnd.lossless.listen(handleTerminalLifecycle);

// ...
await stateSub.cancel();
await hoverSub.cancel();
await terminalSub.cancel();
await dnd.dispose();
```

Own subscriptions at the widget/controller lifecycle boundary and always cancel them before disposal. Emitting after disposal fails explicitly.

## Rust / WASM

```rust
use ores_dnd_core::reactive::{
    is_lossless_lifecycle_phase, DndLocalReactiveBus, rx::*,
};

let mut dnd = DndLocalReactiveBus::new();
let terminal_subscription = dnd
    .events()
    .filter(|event| is_lossless_lifecycle_phase(event.phase))
    .subscribe(handle_terminal_lifecycle);

// Route validated DndReactiveEvent values through dnd.emit(...).
// Keep terminal_subscription owned by the view/controller lifetime.
```

Use rxRust `Local` for WASM/UI-thread work. Choose `Shared` only when a native application intentionally crosses threads; do not add locking merely to imitate another runtime.
