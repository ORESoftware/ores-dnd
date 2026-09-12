# Guarded reactive WASM host

The Rust/WASM target now exposes the guarded `ores.dnd/v1` lifecycle without exporting rxRust subject internals across the JavaScript boundary.

## Host model

`DndLifecycleHandle` is an opaque per-drag-machine handle. JavaScript, browser webviews, and Flutter-WASM hosts create one handle and call `emit_json` with:

- lifecycle phase;
- encoded `DndEnvelope` JSON;
- optional accepted/preview operation;
- optional target ID.

The handle owns `DndLifecycleGuard`, so the same strict or external-one-shot lifecycle rules used by native Rust are enforced before a projection is returned.

## Returned data

`emit_json` returns JSON containing only:

```json
{
  "state": { "active": true, "phase": "drag-over", "dragId": "...", "sourceRuntime": "...", "itemCount": 1 },
  "telemetry": { "phase": "drag-over", "dragId": "...", "sourceRuntime": "...", "itemCount": 1 }
}
```

Optional operation/target metadata may also be present. The projection never returns `items`, `DndItem.data`, clipboard/file contents, or the raw envelope. The incoming envelope is decoded and validated inside WASM only.

Invalid phase/operation diagnostics intentionally do not echo the untrusted input value. Hosts may log the generic error category, but should not reflect rejected payload text into telemetry or crash reports.

## Strict and external-drop modes

`DndLifecycleHandle(false)` uses strict mode: an in-process drag starts with `drag-start` and ends with `drag-end`.

`DndLifecycleHandle(true)` additionally permits an idle one-shot `drop` for OS/browser drags whose source is outside ores-dnd. That one-shot projection remains `active: false`; it does not manufacture a synthetic ongoing drag.

## Why the handle is opaque

Do not expose a raw rxRust `Subject` through `wasm-bindgen` or WIT. Subscription ownership, scheduler types, and Rust interior mutability are runtime implementation details. The stable host boundary is validated lifecycle input plus metadata-only projections. Native Rust consumers remain free to use the rxRust subjects/operators directly.

## Testing

The WASM CI lane performs two distinct checks:

1. native `cargo test -p ores-dnd-wasm` for guarded lifecycle, external-drop semantics, redaction, and non-echoing parser errors;
2. `cargo check -p ores-dnd-wasm --target wasm32-unknown-unknown` to prove the same boundary compiles for the actual WASM target.

The existing WIT file still describes the codec-level component boundary. A future WIT resource for the stateful lifecycle handle should land only with a component-model/WIT validation lane; the current `wasm-bindgen` API is the reviewed stateful host surface.