# Adopting ores-dnd in an org

`ores-dnd` is meant to be pulled in once per org through the public core
library (`<org>-pub-lib-core`) and then used by every client of that org.
This is the runbook; `rollout/fleet.toml` lists the first cohort.

## 1. `<org>-pub-lib-core` — one dependency edge

```toml
# .zpkg.toml
[dependencies]
"oresoftware/ores-dnd" = "^0.1.0"
```

`zed install` materialises the package under `.vendor/.zed/oresoftware/ores-dnd`
with every target (`rust`, `wasm`, `mash`, `leptos`, `dioxus`, `typescript`,
`dart`, `flutter`, `contracts`). The pub-lib-core re-exports what its clients
need and adds the org's own **policies** — policies are data, so they belong
in the pub-lib-core (shared by browser, desktop and Flutter) and are re-read
by the server from the same module.

```rust
// <org>-pub-lib-core/src/dnd.rs
pub use ores_dnd_core::*;
pub fn inbox_policy() -> DndDropPolicy {
    DndDropPolicy::new("inbox", &[DndOperation::Move], &[DndItemKind::Json])
        .with_media_types(["application/vnd.<org>.item+json"])
        .with_max_items(1)
}
```

Do **not** copy contract files, fixtures or the trace corpus into the org:
the `contracts` target is the single source; `npm run conformance` in
ores-dnd is the gate that proves the runtimes agree.

## 2. Per app type

| App | Add | Wire |
| --- | --- | --- |
| `*-web-server.rs` (MASH) | `ores-dnd-mash` | render zones/sources with `html::drop_zone` / `drag_source`, emit `boot_script(url)`, `app.merge(server::router_with(backend, RouterOptions::default()))` behind shared-auth + ores-rate-limit; implement `DropCommitBackend` over the org's ORM (`*-lib-core`) with `already_committed` backed by the drops table |
| Leptos islands | `ores-dnd-leptos` | `provide_dnd_session()` at the island root, `<DropZone handle policy on_drop>`, `<DragSource handle envelope>`; `on_drop` → `commit_accepted_drop` with the org's ports |
| Dioxus (web/desktop/mobile) | `ores-dnd-dioxus` | same shape as Leptos over the portable `DataTransfer` |
| `*-desktop-app.rs` (native, no DOM) | `ores-dnd-core` | map OS drag events to `DndSessionInput`s; use `ores-dnd-wasm` only if the desktop hosts a webview |
| JS / TypeScript clients, webviews | `@oresoftware/ores-dnd` | `bindDragSource` / `bindDropZone` (+ `pointer` on touch surfaces); `autoBind(document)` for server-rendered pages; `htmx.commitFromZone` to the MASH endpoint |
| `*-flutter` | `ores_dnd` + `ores_dnd_flutter` | one `OresDndController` per screen, `OresDraggable` / `OresDragTarget(policy: …)`; Flutter web may inject `OresDndWasmPort` over the wasm-bindgen glue |

## 3. Ports

| Port | Fleet component | Contract |
| --- | --- | --- |
| `OresFormsPort.applyAcceptedDrop` | ores-forms | map the payload onto the field/action named by the policy's `formId`; refuse anything else |
| `OptoSyncPort.persistAcceptedDrop` | opto-sync | persist an entity mutation keyed by `dragId` (idempotent) through the local IndexedDB/SQLite path; replication is opto-sync's job |
| `OresOtelPort.emitDndEvent` | ores-otel | forward `DndTelemetryEvent` as a span event / log line; never add `data` |

Ports run only for a `dropped` session and only in this order; a port that
throws aborts the rest.

## 4. Rollout checklist

1. pub-lib-core: dependency edge, policies module, `zed install` green.
2. Server: endpoint mounted behind auth and rate limiting; `DropCommitBackend`
   implemented; drops table with a unique `drag_id`.
3. Each client: zones/sources bound; the zone's `data-ores-dnd-state` (or
   `OresZoneState`) drives styling — no custom accept/reject logic in the UI.
4. Telemetry: an ores-otel port wired; verify no `data` in emitted events.
5. Conformance: the org's CI runs ores-dnd's `npm run conformance` on the
   pinned version (or consumes its retained evidence artifact) so a bump is
   admitted only with matching runtime evidence.
6. Security review against `docs/SECURITY.md` T1–T12.

## 5. What not to do

- Do not decide accept/reject in the UI or trust the browser's verdict on
  the server; both sides run the same policy code and the server's answer wins.
- Do not put dragged `data` into logs, analytics or replayable streams.
- Do not fork the protocol per org; add a policy, a media type or an adapter.
- Do not bump `protocol` for additive changes (`docs/DESIGN.md` §8).
