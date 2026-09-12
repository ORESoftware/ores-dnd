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

Use `writeToDataTransfer()` in `dragstart`, `readFromDataTransfer()` in `drop`, and `negotiateOperation()` against the target's allowed operations. HTML-first MASH pages can load the compiled TypeScript adapter as a tiny module; no React/JSX is required.

The TypeScript `DropCommitPorts` map directly onto the fleet components:

- `OresFormsPort.applyAcceptedDrop` → `ores-forms/ores-forms-clients` form action/field adapter.
- `OptoSyncPort.persistAcceptedDrop` → `opto-sync/opto-sync-clients` IndexedDB/SQLite mutation + sync path.
- `OresOtelPort.emitDndEvent` → `ores-otel/ores-otel-clients` trace/log/metric bridge.

The core calls them only after decode, validation, operation negotiation, and positive acceptance.

The additive `@oresoftware/ores-dnd/reactive` entrypoint uses RxJS and extends the Opto-Sync/ORES-OTel ports with explicit Supabase methods. `commitAcceptedDropReactive()` calls those methods; it does not import a Supabase client itself.

## Rust desktop + MASH / Leptos / Dioxus

`ores-dnd-core` is UI-framework neutral. Feature modules expose stable binding metadata for MASH, Leptos, and Dioxus, while `ores-dnd-wasm` owns the browser/WASM codec.

This split is deliberate: Dioxus and Leptos can move release versions without forcing a protocol release, and MASH can stay HTML/HTMX-first. Desktop webviews can invoke the same WASM exports used by browser clients:

- `normalize_envelope_json`
- `validate_envelope_json`
- `negotiate_operation_json`
- `protocol_version`
- `mime_type`

Native desktop hosts that do not use a DOM can use `ores-dnd-core` directly and map OS drag/drop events into `DndEnvelope`.

The Rust `reactive_sync` module uses RxRust for functional-reactive composition while keeping scheduler choice with the host. Implement `DndReactiveSink` with a host-owned `Local::subject()` for UI/WASM or `Shared::subject()` where cross-thread fan-out is required.

## Flutter / Dart + WASM

`OresDraggable` serializes `DndEnvelope` as its `Draggable<String>.data`. `OresDragTarget` decodes and negotiates the operation before invoking application code.

The Dart `OresDndWasmPort` is dependency-injected so Flutter Web can call the `wasm-bindgen` JS glue while native Flutter desktop/mobile can use a Wasmtime/Wasmer/FFI host if desired. The app's existing wasm loader remains responsible for loading/caching/disposing modules.

`package:ores_dnd/reactive_sync.dart` adds the RxDart `DndReactiveBus`; the Flutter package re-exports it so mobile and desktop UI can use the same stream vocabulary without a second drag/drop protocol.

## opto-sync

Accepted drop state should be represented as an application entity mutation, not as a transport-specific drag event. The recommended flow is:

1. validate envelope and target policy;
2. apply an explicit `ores-forms` field/action mutation when relevant;
3. persist the resulting entity mutation through opto-sync's local IndexedDB/SQLite path;
4. call the injected Opto-Sync Supabase method so the concrete host adapter can enqueue/reconcile the remote write;
5. construct content-free `ores-otel` lifecycle telemetry;
6. emit it through the local ORES-OTel adapter;
7. call the injected ORES-OTel Supabase method for the host's telemetry sink.

This keeps drag/drop idempotency and offline behavior aligned with the app's normal sync model while ensuring both requested Supabase paths are explicit and testable.

`ores-dnd` intentionally does not declare a direct native dependency on `opto-sync-clients`: the Opto-Sync package currently keeps cross-organization client dependencies injected until immutable public package resolution is available. The port is the stable integration boundary rather than a second sync engine or fragile git dependency.

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

The reactive Supabase path receives only `DndTelemetryEvent`; provider failures are reduced to the generic `sync-failed` receipt before entering RxJS/RxDart/RxRust streams.

See [`reactive-sync.md`](./reactive-sync.md) for the full stream and Supabase adapter contract.

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
