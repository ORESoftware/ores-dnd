# Reactive drag/drop and Supabase synchronization

`ores-dnd` exposes one drag/drop protocol (`ores.dnd/v1`) across TypeScript/webviews, Rust/native + WASM, Dart, and Flutter. Reactive composition is additive: it does not create a second wire protocol and it does not make decoding a drop mutate application state.

## Reactive libraries

| Runtime | Reactive library | ores-dnd surface |
| --- | --- | --- |
| TypeScript / browser / webview | RxJS 7.8.2 | `@oresoftware/ores-dnd/reactive` and `DndReactiveBus` |
| Rust native / WASM | RxRust 1.0.0-rc.5 | `reactive_sync`, `DndReactiveSink`, `observe_reactive_events` |
| Dart / Flutter | RxDart 0.28.0 | `package:ores_dnd/reactive_sync.dart` and `DndReactiveBus` |

Rust deliberately does not choose a scheduler or threading model. Desktop/WASM hosts can bridge `DndReactiveSink` into a host-owned `Local::subject()` or `Shared::subject()`; the library provides a scheduler-neutral sink plus a finite RxRust composition helper.

## Opto-Sync and ORES-OTel boundary

The base v1 API already has `OptoSyncPort` and `OresOtelPort`. The reactive layer extends them with host-provided Supabase calls:

- TypeScript: `OptoSyncSupabasePort.syncAcceptedDropToSupabase` and `OresOtelSupabasePort.syncDndEventToSupabase`.
- Dart: `OptoSyncSupabasePort.syncAcceptedDropToSupabase` and `OresOtelSupabasePort.syncDndEventToSupabase`.
- Rust: `OptoSyncSupabasePort::sync_accepted_drop_to_supabase` and `OresOtelSupabasePort::sync_dnd_event_to_supabase`.

These are intentionally abstract methods. `ores-dnd` **calls** them, but does not import a Supabase SDK, own a Supabase URL/key, select tables, or become a second synchronization engine. Concrete applications bind them to reviewed `opto-sync/opto-sync-clients` and `ores-otel/ores-otel-clients` adapters using runtime configuration and secret stores.

The accepted-drop order is:

1. validate `ores.dnd/v1` and reject mismatched drag IDs / unsupported operations;
2. optional `ores-forms` mutation;
3. Opto-Sync local-first persistence/queueing;
4. Opto-Sync Supabase synchronization;
5. construct a sanitized `DndTelemetryEvent`;
6. ORES-OTel local trace/log/metric emission;
7. ORES-OTel Supabase synchronization.

If an Opto-Sync Supabase call fails, the function fails closed before ORES-OTel is invoked. A reactive failure receipt contains `sync-failed`, never raw provider diagnostics.

## Privacy invariant

Reactive events are metadata-only. They may contain:

- `dragId`;
- accepted operation;
- target ID;
- source runtime;
- item count;
- sync channel (`opto-sync` or `ores-otel`);
- Supabase success/failure status.

They must **never** contain `DndItem.data`, clipboard/file content, Supabase credentials, access tokens, connection strings, or raw provider errors. Tests in TypeScript, Dart, and Rust enforce the payload-redaction invariant.

## Host adapter sketch

A product-specific adapter should implement both phases rather than bypass Opto-Sync or ORES-OTel with direct writes from UI code:

```text
accepted drop
  -> ores-dnd validation
  -> opto-sync local mutation / reconciliation queue
  -> OptoSyncSupabasePort.sync*()
       -> host-owned Supabase client
  -> sanitized ores-otel event
  -> OresOtelSupabasePort.sync*()
       -> host-owned telemetry sink / Supabase client
  -> Rx stream receipts for UI/domain composition
```

Credentials remain outside `ores-dnd`; use the consuming application's normal SOPS/runtime configuration path.
