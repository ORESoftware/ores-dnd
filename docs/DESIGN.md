# ores-dnd design — one drag-and-drop semantics, many runtimes

`ores-dnd` exists so that every ORESoftware application answers the question
"how do we do drag-and-drop here?" the same way, whether the surface is an
HTMX page rendered by a `*-web-server.rs`, a Leptos or Dioxus island, a Rust
`*-desktop-app.rs`, a Flutter client, or a JS client talking to a WASM module.
It is a long-lived protocol plus thin adapters — not a widget library.

## 1. Layering

```
┌───────────────────────────────────────────────────────────────────────┐
│ 4. Application wiring   (*-web-server.rs, *-desktop-app.rs, *-flutter)│
│    owns state, authorization, storage, UI composition                 │
├───────────────────────────────────────────────────────────────────────┤
│ 3. Runtime adapters     ts/dom  ts/pointer  ts/htmx  rust-leptos      │
│                         rust-dioxus  rust-mash  flutter  rust-wasm    │
│    translate native events → session inputs, snapshots → native UI    │
├───────────────────────────────────────────────────────────────────────┤
│ 2. Core semantics       codec · drop policy · session state machine   │
│    implemented three times (Rust, TypeScript, Dart), proven equal by  │
│    the shared trace corpus                                            │
├───────────────────────────────────────────────────────────────────────┤
│ 1. Contract             contracts/main.tsp ⇄ contracts/authored.json  │
│    independent peer authorities, tjsv parity + runtime evidence       │
└───────────────────────────────────────────────────────────────────────┘
```

Rule of thumb: anything that would make two runtimes disagree about *whether*
a drop is allowed or *which* operation it performs belongs in layer 2 and gets
a trace. Anything about *how it looks* belongs in layer 3 or 4.

## 2. Contract (`ores.dnd/v1`)

Declared in `contracts/main.tsp` and `contracts/authored.schema.json`. The two
files are authored independently; `tjsv check` compares them structurally,
generates a witness schema from the TypeSpec, and executes both authorities as
validators over `contracts/instances/**` (valid must be accepted, invalid must
be rejected by *both*). A change to one authority without the matching change
to the other fails the build.

| Declaration | Role |
| --- | --- |
| `DndEnvelope`, `DndItem` | the dragged payload: versioned, size-bounded, item-typed |
| `DndDropPolicy` | what a target accepts (operations, kinds, media types, limits, form binding) |
| `DndSessionInput` | what a host feeds the state machine (`start`, `enter`, `leave`, `drop`, `cancel`, `end`) |
| `DndSessionSnapshot` | what the state machine reports after each input |
| `DndSessionTrace` | executable specification: inputs and the exact expected snapshots |
| `DndDropResult` | the outcome handed to the commit ports |
| `DndTelemetryEvent` | content-free lifecycle telemetry for ores-otel |
| `DndRejectCode` | the standard reasons a target refuses a payload |

## 3. Session state machine

A session is the life of one drag, from `start` to a terminal `dropped` or
`cancelled`. It is a pure function `apply(snapshot, input) → snapshot`; hosts
keep the snapshot wherever their framework keeps state (a signal, a
`ValueNotifier`, a plain variable).

### States

| State | Fields | Meaning |
| --- | --- | --- |
| `idle` | `errorCode?` | no drag; `errorCode = invalid-envelope` after a refused `start` |
| `dragging` | `dragId`, `targetId?`, `errorCode?` | drag in flight; when `targetId` is set the pointer is over a target that **rejected** the payload for `errorCode` |
| `over-target` | `dragId`, `targetId`, `operation` | over a target that accepts; `operation` is negotiated |
| `dropped` | `dragId`, `targetId`, `operation` | terminal, accepted |
| `cancelled` | `dragId`, `targetId?`, `errorCode` | terminal, refused or abandoned |

### Transitions

| From | Input | To |
| --- | --- | --- |
| any | `start(envelope)` valid | `dragging{dragId}` — always a fresh session |
| any | `start(envelope)` invalid | `idle{invalid-envelope}` |
| `idle` | anything else | unchanged |
| `dragging` / `over-target` | `enter(t, policy, preferred?)` accepts | `over-target{t, op}` |
| `dragging` / `over-target` | `enter(t, policy)` rejects with `code` | `dragging{t, code}` |
| `over-target{t}` / `dragging{t}` | `leave(t)` | `dragging` (target cleared) |
| any non-terminal | `leave(other)` | unchanged |
| `over-target{t, op}` | `drop(t)` | `dropped{t, op}` |
| `dragging{t, code}` | `drop(t)` | `cancelled{t, code}` |
| `dragging` (no target) | `drop(t)` | `cancelled{t, no-active-target}` |
| `over-target{t}` / `dragging{t}` | `drop(u ≠ t)` | `cancelled{u, target-mismatch}` |
| any non-terminal | `cancel` / `end` | `cancelled{cancelled}` |
| `dropped` / `cancelled` | anything but `start` | unchanged |

`end` is the host's "the native drag finished" signal (`dragend`, Flutter
`onDragEnd`, an OS drag session ending). Sending it after `drop` is a no-op, so
adapters can always send it.

### Policy evaluation

`evaluate(envelope, policy, preferred?)` runs in this fixed order and stops at
the first failure, so every runtime reports the same reject code:

1. `negotiate(envelope.allowedOperations, policy.allowedOperations, preferred)`
   — the preferred operation if both sides allow it, else the first of
   `move`, `copy`, `link` allowed by both; none → `no-common-operation`
2. every item kind ∈ `acceptedKinds`, else `item-kind-not-accepted`
3. if `acceptedMediaTypes` present: every item media type matches an entry
   exactly or by `type/*` wildcard (case-insensitive), else `media-type-not-accepted`
4. `items.length ≤ maxItems`, else `too-many-items`
5. Σ UTF-8 bytes of `item.data` ≤ `maxTotalBytes`, else `payload-too-large`
6. if both `policy.formId` and `envelope.formId` are present they are equal,
   else `form-mismatch`

### Traces

`contracts/instances/DndSessionTrace/valid/*.json` hold the executable
specification. They are validated as contract instances by tjsv **and**
replayed by the Rust, TypeScript and Dart cores in their test suites. Add a
trace first; a runtime that disagrees fails. `scripts/gen-traces.py` is the
source of the corpus.

## 4. Runtime mapping

| Runtime | Native mechanism | Adapter | Session inputs come from |
| --- | --- | --- | --- |
| Browser / webview (TS) | HTML5 DnD (`DataTransfer`, `application/vnd.ores.dnd+json`) | `src/ts` `dom.ts` | `dragstart→start`, `dragenter/dragover→enter`, `dragleave→leave`, `drop→drop`, `dragend→end` |
| Touch / webviews without native DnD (TS) | Pointer events + hit testing | `src/ts` `pointer.ts` | `pointerdown+move threshold→start`, hit-test change→`enter/leave`, `pointerup→drop/end` |
| MASH (maud + axum + htmx) | HTML-first: zones carry `data-ores-dnd-*` attributes; the TS adapter posts `DndDropResult` to an `hx-post` endpoint | `src/rust-mash` + `src/ts` `htmx.ts` | same as browser; server re-validates with the Rust core |
| Leptos islands | `on:dragstart`/`on:drop` handlers, snapshot in a signal | `src/rust-leptos` | same as browser, decoded through `ores-dnd-wasm` or the core |
| Dioxus (web + desktop) | `ondragenter`/`ondrop` handlers, snapshot in a `Signal` | `src/rust-dioxus` | intra-app envelope registry (native drag events do not expose `DataTransfer` portably) |
| Rust desktop (`*-desktop-app.rs`) | OS drag session (winit/tao/wry/egui events) | `ores-dnd-core` directly | host maps OS events to inputs |
| Flutter (iOS/Android/desktop/web) | `Draggable<String>` / `DragTarget<String>` | `src/flutter` | `onWillAcceptWithDetails→enter`, `onLeave→leave`, `onAcceptWithDetails→drop`, `onDragEnd→end` |
| WASM consumers (JS, Flutter web, Rust webviews) | `wasm-bindgen` exports of the Rust core | `src/rust-wasm` | `WasmDndSession.apply(inputJson)` |

Framework crates depend on `ores-dnd-core`, never the other way round, so a
Leptos or Dioxus major release is a change to one adapter crate and not to the
protocol.

## 5. Integration ports

The core never touches application state. After a session reaches `dropped`,
the host calls `commitAcceptedDrop(envelope, result, ports)` which runs the
opt-in ports in a fixed order:

1. **ores-forms** `applyAcceptedDrop` — map the payload onto an explicitly
   allowed form field/action (the policy's `formId` names it);
2. **opto-sync** `persistAcceptedDrop` — persist the resulting entity mutation
   through the app's IndexedDB/SQLite path so it replicates like any other
   write;
3. **ores-otel** `emitDndEvent` — content-free telemetry (`phase`, `dragId`,
   `sourceRuntime`, `itemCount`, `operation`, `targetId`, `traceparent`
   propagation when policy allows).

A decoded drop never mutates storage by itself; a port that throws aborts the
remaining ports.

## 6. Cross-runtime enforcement

Two gates, both fail-closed:

- **Parity** — `npm run contracts:check` (tjsv): TypeSpec and JSON Schema
  agree, and every instance under `contracts/instances` is accepted/rejected
  by both authorities as declared.
- **Runtime evidence** — `npm run conformance`: the parity receipt is turned
  into a Contract IR; each language runs the same corpus through its own
  decoder and writes tjsv `runtime-evidence`; the verifier admits the
  evidence only when it names the exact Contract IR id, the exact parity run
  id and the exact corpus digest, and every required adapter (`typescript`,
  `rust`, `dart`) accepted/rejected every case exactly as declared. The session
  traces are additionally replayed by each language's test suite.

## 7. Security defaults

- Payloads are size-bounded before parsing; unknown properties, operations,
  kinds and protocol versions are rejected.
- A target only ever sees a payload that passed its own policy; the policy is
  data, so a server (MASH) can re-evaluate it for a client-reported drop.
- Telemetry never carries `DndItem.data`, file contents or secret-bearing URIs.
- URI/file items are data only; opening or fetching them is a host decision.

## 8. Versioning

`ores.dnd/v1` is additive-only. New optional fields, enum members that a
runtime can reject (`unevaluatedProperties: false` and closed enums make old
runtimes fail closed on new inputs), and new adapters do not bump the protocol.
Removing or re-typing a field is `ores.dnd/v2` and a new envelope `protocol`
value; runtimes list the versions they accept.
