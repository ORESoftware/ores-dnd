# Native desktop drag-and-drop bridge

`ores-dnd-native` is the framework-neutral adapter for native Rust desktop shells such as GPUI, Slint, Qt/QML FFI, winit/tao and similar host event loops. It deliberately does **not** depend on one UI toolkit.

## One state machine

Native hosts translate their callbacks into the same `ores_dnd_core::DndSession` used by the browser, Flutter, WASM and framework adapters.

- in-process drag start: `NativeDndBridge::start(envelope)`;
- ordinary target movement: `enter(policy, preferred)` / `leave(target_id)`;
- accepted current payload: `drop_current(target_id)`;
- cancellation/window loss: `cancel()` or `end()`;
- external/system drag enter: `external_enter(provisional, policy, preferred)`;
- external/system drop: `external_drop(definitive, policy, preferred)`.

The external methods are important. A native toolkit may report an external `drag-enter` while no ORES session exists. The bridge first emits canonical `start`, then `enter`; it never teaches the core state machine that idle `enter` is valid. On drop, the definitive envelope starts a fresh evaluation, so a provisional MIME/kind acceptance cannot bypass final item-count, byte, media-type, form or operation checks.

## Host-owned OS resources

The bridge does not open files, resolve local paths, accept sandbox bookmarks, create temp files, fulfill file promises, monitor the clipboard or invoke platform share sheets. Those lifecycles differ by OS and toolkit and remain explicit host adapters.

Recommended boundaries:

- use protected/provisional metadata only for hover admission;
- acquire real data only after the host receives the definitive drop callback;
- do not put credentials, durable signed URLs or local filesystem paths into telemetry;
- prefer bounded bytes for small payloads and an application-owned opaque/versioned handoff manifest for larger resources;
- keep temp-file/file-promise ownership long enough for the receiving application, then clean it according to the platform lifecycle;
- never treat a provisional envelope as authorization to open a file or network URL.

`DndTelemetryEvent` remains content-free. Hosts can call the core telemetry projection with lifecycle metadata after each accepted transition without including item `data`.

## Toolkit sketch

```rust
use ores_dnd_native::NativeDndBridge;

let mut dnd = NativeDndBridge::new();

// OS says an external image is hovering. Build a provisional envelope from
// metadata only; do not read the file yet.
let snapshot = dnd.external_enter(provisional, image_policy.clone(), None);

// Later, the OS delivers the real drop. Materialize/authorize the payload in
// the host, then let the canonical policy re-evaluate it.
let result = dnd.external_drop(definitive, image_policy, None);
```

The host owns focus, cursors, drag images, accessibility narration and window/toolkit event registration. The bridge owns only canonical session semantics.