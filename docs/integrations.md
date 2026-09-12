# Integration model

`ores-dnd` owns drag/drop semantics and codecs. Product repositories own state, authorization, storage, and UI composition.

## zed-pkg

A `*-pub-lib-core` consumer adds one shared dependency edge:

```toml
[dependencies]
"oresoftware/ores-dnd" = "^0.1.0"
```

zed-pkg then exposes the runtime target appropriate to the consuming app (`rust`, `wasm`, `typescript`, `dart`, or `contracts`). App repositories should not fork the protocol or copy fixture files.

## TypeScript / browser / webview

`@oresoftware/ores-dnd` (`src/ts`) is ESM with subpath exports; no React/JSX, no runtime dependencies.

| Module | What it gives a `*-web-server.rs` page or JS client |
| --- | --- |
| `codec` | `validateEnvelope`, `encodeEnvelope`/`decodeEnvelope`, `writeToDataTransfer`/`readFromDataTransfer`, `negotiateOperation`, `telemetryFor`, `commitAcceptedDrop` |
| `policy` | `DndDropPolicy`, `validatePolicy`, `evaluatePolicy` (fleet-wide order), `mediaTypeMatches` |
| `session` | `DndSession` (`apply`, `snapshot`, `envelope`, `result`, `subscribe`), `inputs.*`, `replayTrace` |
| `dom` | `bindDragSource(el, envelope, session)`, `bindDropZone(el, policy, session, { onDrop })`, `autoBind(root)` for HTML-first pages; keeps `data-ores-dnd-state` (`idle` / `dragging` / `accepting` / `rejecting` / `dropped`) on every zone and dispatches `ores-dnd:state` / `ores-dnd:drop` |
| `pointer` | `ZoneRegistry` + `bindPointerDragSource` — the same session inputs from pointer events for touch surfaces and webviews without native DnD |
| `htmx` | `commitDrop(url, envelope, result)` / `commitFromZone(zone, …)` — POST the `DropCommitRequest` to the `ores-dnd-mash` endpoint, take the **server's** verdict (JSON) or swap an HTML partial and `htmx.process` it |
| `wasm` | `WasmSession`, `evaluatePolicyWasm`, `crossCheckPolicy`, `crossCheckTrace` over the `ores-dnd-wasm` exports |

Minimal wiring:

```ts
import { DndSession, bindDragSource, bindDropZone, commitAcceptedDrop } from "@oresoftware/ores-dnd";

const session = new DndSession();
bindDragSource(card, envelope, session, { otel });
bindDropZone(list, { targetId: "list", allowedOperations: ["move"], acceptedKinds: ["json"] }, session, {
  onDrop: (envelope, result) => commitAcceptedDrop(envelope, result, { forms, optoSync, otel }),
});
```

External drags (from another page or app) are evaluated provisionally on `DataTransfer.types` during `dragover` — browsers hide the data then — and definitively on `drop`, when the real payload replaces the provisional session before the drop is applied. Modifier keys map to a preferred operation the way native file managers do (Ctrl/⌥ copy, Shift move, Ctrl+Shift/⌘ link).

For MASH pages nothing is hand-wired: the server renders zones/sources with `ores-dnd-mash` and `autoBind(document)` (loaded by `boot_script`) binds them, commits accepted drops to `data-ores-dnd-commit` and lets the server decide.

The `DropCommitPorts` map directly onto the fleet components:

- `OresFormsPort.applyAcceptedDrop` → `ores-forms/ores-forms-clients` form action/field adapter.
- `OptoSyncPort.persistAcceptedDrop` → `opto-sync/opto-sync-clients` IndexedDB/SQLite mutation + sync path.
- `OresOtelPort.emitDndEvent` → `ores-otel/ores-otel-clients` trace/log/metric bridge.

The core calls them only after decode, validation, operation negotiation, and positive acceptance.

## Rust desktop + MASH / Leptos / Dioxus

`ores-dnd-core` is UI-framework neutral and owns three things every adapter reuses: the codec, `evaluate_policy` (the fixed-order accept/reject rules) and `DndSession` (the state machine). Native desktop hosts (`*-desktop-app.rs`) map OS drag events straight onto `DndSessionInput`s.

Three adapter crates sit on top and never leak upward:

- **`ores-dnd-mash`** — `html::drop_zone(policy, wiring, inner)` and `html::drag_source(envelope, …)` render maud markup with the stable `data-ores-dnd-*` attributes; `html::boot_script(url)` loads the TypeScript adapter (`autoBind`) that drives the drag in the browser; `server::router(backend, path)` mounts `POST /ores-dnd/drop`, which **re-runs the policy on the server** (`verify_drop_commit`) before calling the app's `DropCommitBackend::commit` — the browser reports, the server decides.
- **`ores-dnd-leptos`** — `use_dnd_session()` / `provide_dnd_session()` put the snapshot in a signal; `<DropZone handle policy on_drop>` and `<DragSource handle envelope>` bind `on:dragenter/dragover/dragleave/drop/dragstart/dragend`; `browser::*` is the `web_sys` DataTransfer glue (mirrors `src/ts/dom.ts`).
- **`ores-dnd-dioxus`** — same shape (`use_dnd_session`, `DropZone`, `DragSource`) over Dioxus' portable `DataTransfer`, so one adapter serves web, desktop and mobile.

Desktop webviews and JS clients can instead call the `ores-dnd-wasm` exports, all JSON strings so the ABI is identical across hosts:

- `normalize_envelope_json`, `validate_envelope_json`, `validate_declaration_json`
- `negotiate_operation_json`, `evaluate_policy_json`, `replay_trace_json`
- `WasmDndSession` (`apply`, `snapshot`, `envelope`, `result`)
- `protocol_version`, `mime_type`

## Flutter / Dart + WASM

`OresDraggable` serializes `DndEnvelope` as its `Draggable<String>.data`. `OresDragTarget` decodes and negotiates the operation before invoking application code.

The Dart `OresDndWasmPort` is dependency-injected so Flutter Web can call the `wasm-bindgen` JS glue while native Flutter desktop/mobile can use a Wasmtime/Wasmer/FFI host if desired. The app's existing wasm loader remains responsible for loading/caching/disposing modules.

## opto-sync

Accepted drop state should be represented as an application entity mutation, not as a transport-specific drag event. The recommended flow is:

1. validate envelope and target policy;
2. apply an explicit `ores-forms` field/action mutation when relevant;
3. persist the resulting entity mutation through opto-sync's local IndexedDB/SQLite path;
4. let opto-sync replicate the canonical entity state to Postgres/Supabase/Neon;
5. emit content-free `ores-otel` lifecycle telemetry.

This keeps drag/drop idempotency and offline behavior aligned with the app's normal sync model.

## ores-forms

A form must explicitly opt a field/action into a drop. Do not map arbitrary `DndItem.data` into arbitrary fields. Recommended policies include accepted item kinds, accepted media types, max item count, max total bytes, and allowed copy/move/link operations.

## ores-otel

Telemetry can include:

- lifecycle phase;
- `dragId`;
- source runtime;
- item count;
- negotiated operation;
- target identifier;
- traceparent propagation when already present and allowed by application policy.

Telemetry must never include `DndItem.data`, credentials, local file contents, pasted form values, or full URIs containing secrets/query tokens.

## Contract verification

Every contract change must run:

```bash
npx tjsv check \
  --typespec=contracts/main.tsp \
  --schema=contracts/authored.schema.json \
  --instances=contracts/instances \
  --report=artifacts/schema-parity.json
```

Both authorities must accept the valid fixture and reject the invalid fixture. Runtime tests additionally load the same fixture corpus.
