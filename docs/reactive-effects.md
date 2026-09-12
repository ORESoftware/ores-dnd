# Retry-safe reactive effects

`ores-dnd` keeps drag/drop observation separate from accepted-drop effects. The reactive lifecycle buses remain hot/local stream surfaces; this document covers the optional effect pipeline that calls Ores Forms, Opto-Sync, Supabase-facing Opto-Sync adapters, ORES-OTel, and Supabase-facing ORES-OTel adapters.

## Stage order

For an accepted and validated drop the effect sequence is:

1. `forms`
2. `opto-local`
3. `opto-supabase`
4. `otel-local`
5. `otel-supabase`

The canonical `commitAcceptedDrop`/`commit_accepted_drop` path is invoked with no effect ports first, so `ores.dnd/v1`, drag ID, acceptance, and source-allowed operation validation still have one runtime authority.

## Retry contract

Each runtime exposes the same two recovery mechanisms:

- a deterministic logical operation/idempotency key derived from the drag ID, target ID, and accepted operation;
- a host-provided stage journal that records completed stages.

On retry, completed stages are emitted as `skipped` receipts and are not called again. A journal read failure fails closed before that stage executes.

Supabase-facing adapters receive the deterministic idempotency key and **must use it for provider-side upsert/deduplication**. This closes the important write-before-journal window where a remote write succeeds but marking the stage complete fails.

This is not a claim of universal exactly-once execution. Local Ores Forms / Opto-Sync / ORES-OTel adapters should be naturally idempotent or update their durable stage journal in the same local transaction/outbox boundary. A durable product implementation should bind the journal to Opto-Sync's SQLite/IndexedDB/Postgres-backed state rather than relying on the in-memory Rust helper.

## Privacy

Effect receipts contain only:

- idempotency key;
- drag ID;
- stage;
- status (`completed`, `skipped`, `failed`);
- optional target ID;
- generic `effect-failed` code.

They never contain `DndItem.data`, clipboard/file contents, provider diagnostics, credentials, Supabase URLs/keys, or full exception text. Provider errors are returned to the immediate caller for error handling but are never copied into receipt streams or telemetry.

## Runtime surfaces

| Runtime | Effect module | Stream library |
| --- | --- | --- |
| TypeScript | `@oresoftware/ores-dnd/reactive-effects` | RxJS |
| Dart / Flutter | `package:ores_dnd/ores_dnd_reactive_effects.dart` | RxDart |
| Rust / WASM | `ores_dnd_core::reactive_effects` | integrates with the existing rxRust reactive feature |

Flutter re-exports the Dart effect module. The core modules do not import a Supabase SDK; host applications implement the Supabase-facing ports through their reviewed Opto-Sync and ORES-OTel adapters.

## Terminal events and backpressure

Lifecycle presentation streams may sample/coalesce high-frequency `drag-over` updates, but effect execution starts only from an explicitly accepted drop. Never route `drop`, `drag-end`, stage-failure receipts, or stage-completion receipts through a lossy throttle/debounce operator.
