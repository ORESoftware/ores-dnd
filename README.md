# ores-dnd

`ores-dnd` is the fleet-wide drag-and-drop contract and adapter library for browser/webview TypeScript, Rust desktop/WASM, portable WASM consumers, and Dart/Flutter.

The goal is not a single UI widget. The goal is one long-lived protocol for drag payloads, copy/move/link negotiation, drop results, persistence hooks, telemetry, and functional-reactive lifecycle streams across runtimes so each application can keep native UI while sharing semantics.

## Contract authorities

`contracts/main.tsp` and `contracts/authored.schema.json` are independent, human-authored peer authorities. `@oresoftware/typespec-json-schema-validator` (`tjsv`) generates a comparison witness only and fails closed when the authorities or instance corpus diverge.

```bash
npm install
npm run contracts:check
```

Generated validator output belongs under `.typespec-json-schema-validator/generated/` and is evidence, never a third authored authority.

## Runtime targets

| Target | Path | Intended consumers |
| --- | --- | --- |
| TypeScript + DOM/WebView | `src/ts` | `*-web-server.rs` HTML/HTMX/webviews and JS clients |
| Rust core | `src/rust` | `*-desktop-app.rs`, shared `*-pub-lib-core`, MASH/Leptos/Dioxus wiring |
| Rust → WASM | `src/rust-wasm` + `wit/` | JS clients, Flutter web bridges, Rust desktop webviews |
| Dart/Flutter | `src/dart` | `*-flutter` iOS/Android/desktop/web clients |

The browser MIME type is `application/vnd.ores.dnd+json`. Plain-text fallback is emitted only for text items.

## Functional-reactive streams

The three primary client runtimes expose native ReactiveX-style lifecycle surfaces:

- TypeScript: RxJS-backed typed surface at `@oresoftware/ores-dnd/reactive`.
- Dart/Flutter: RxDart surface at `package:ores_dnd/ores_dnd_reactive.dart`.
- Rust/WASM: rxRust surface at `ores_dnd_core::reactive`, with `Local` preferred for UI/WASM and `Shared` available for intentional cross-thread native flows.

Raw DnD events are hot and non-replaying because they may contain dragged payload data. Only metadata-only state may use replay/current-value semantics. Persistence and forms are still explicit accepted-drop effects; observing or transforming a stream never writes application state by itself.

See `docs/reactive.md` for lifecycle, privacy, throttling/backpressure, and subscription-ownership rules.

## Integrations

The core libraries intentionally use narrow ports instead of importing application state directly:

- `opto-sync`: persist accepted drop mutations to IndexedDB/SQLite and sync them through the app's existing data plane.
- `ores-forms`: map accepted drops into explicitly allowed form fields/actions.
- `ores-otel`: emit content-free lifecycle telemetry (phase, operation, item count, runtime, target), never dragged data or credentials.
- `zed-pkg`: publish/install `oresoftware/ores-dnd` as a multi-target dependency so `*-pub-lib-core` repos can expose the same semantics to web, desktop, and Flutter apps.

See `docs/integrations.md` and `rollout/fleet.toml`.

## Security defaults

- Payloads are versioned and size-bounded before parsing.
- Unknown operations, item kinds, and protocol versions are rejected.
- Telemetry and replayable reactive state never contain item `data`.
- Raw envelope streams do not replay to late subscribers.
- URI/file drops are data only; consumers decide whether a URI/path is trusted and may be opened.
- Persistence and form application are opt-in ports; a decoded or emitted drop does not mutate storage by itself.
- Copy/move/link is negotiated against both source and target capabilities.

## Development

```bash
# TypeScript / RxJS
npm --prefix src/ts install
npm --prefix src/ts run build
npm --prefix src/ts test

# Rust / rxRust
cargo test -p ores-dnd-core --all-features
rustup target add wasm32-unknown-unknown
cargo check -p ores-dnd-wasm --target wasm32-unknown-unknown

# Dart / RxDart
cd src/dart
dart pub get
dart analyze --fatal-infos
dart test
```

CI runs all four lanes plus contract parity and shared fixture conformance.
