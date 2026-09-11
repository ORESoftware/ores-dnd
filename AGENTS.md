# ores-dnd agent instructions

Read and follow `ORESoftware/my-ai/AGENTS.md` and `ORESoftware/my-ai/SHARED.md` before repository or fleet work.

Repository-specific invariants:

1. `contracts/main.tsp` and `contracts/authored.schema.json` are independent human-authored authorities. Never generate one from the other and commit it as authority.
2. Run `tjsv check` fail-closed before merging contract changes.
3. Every runtime must consume the shared `ores.dnd/v1` semantics and conformance fixtures; runtime-specific UI APIs may differ.
4. Never emit dragged `data` into logs, traces, analytics, crash reports, or form telemetry.
5. Storage/form side effects must pass through explicit opt-in adapters; decoding a drop never mutates application state by itself.
6. Prefer zed-pkg dependency edges from `*-pub-lib-core` and keep per-app wiring thin.
