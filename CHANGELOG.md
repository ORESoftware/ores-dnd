# Changelog

All notable changes to `ores-dnd`. The protocol tag stays `ores.dnd/v1`; every
entry below is additive or tightens what was already invalid.

## Unreleased

### Contract
- Bounded scalars in both authorities: `SafeId` (ids), `ProtocolId`,
  `MediaType` / `MediaTypePattern` (canonical lowercase, no parameters),
  `Traceparent` (W3C), `ErrorCode` (kebab-case); array bounds on operations,
  kinds, items, media patterns and trace steps; `data ≤ 1 048 576` chars.
- `DndDropPolicy`, `DndRejectCode`, the session state machine
  (`DndSessionInput` → `DndSessionSnapshot`) and `DndSessionTrace`.
- `itemCount` bounds stated in both authorities (fixes the parity finding on
  the first release).

### Runtimes
- Rust: `ores-dnd-core` modularised (`envelope`, `policy`, `session`, `ports`,
  `bindings`, `corpus`, `wire`); new `ores-dnd-mash`, `ores-dnd-leptos`,
  `ores-dnd-dioxus`; `ores-dnd-wasm` session ABI.
- TypeScript: `policy`, `session`, `corpus`, `dom`, `pointer`, `htmx`, `wasm`,
  `wire` modules.
- Dart/Flutter: `ores_dnd` policy/session/corpus/wire; `ores_dnd_flutter`
  controller-driven widgets.

### Enforcement
- tjsv runtime-conformance gate (`npm run conformance`) across all three
  languages, retained as a CI artifact; 23 session traces replayed by every
  core; 136-instance corpus.

## 0.1.0 — 2026-09-11
- First release: codec, ports and DataTransfer/Flutter adapters (PR #1).
