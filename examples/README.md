# Examples

| Example | Runtime | Shows |
| --- | --- | --- |
| `mash-kanban/` | Rust — maud + axum + htmx | The canonical MASH wiring: HTML-first zones/sources rendered by `ores-dnd-mash`, the browser adapter booted by `boot_script`, and the hardened drop-commit endpoint re-verifying every drop against server-side policies. `cargo run -p ores-dnd-example-mash-kanban` (serve `src/ts/dist` at `/vendor/ores-dnd/`), tested end to end with tower. |
| `web-vanilla/` | TypeScript — plain DOM | One `DndSession`, three policies, HTML5 DnD plus the pointer fallback on the same zones, ores-forms/opto-sync/ores-otel ports as in-page loggers, external drags from other apps. No build step beyond `npm --prefix src/ts run build`. |

Leptos and Dioxus usage is shown in the crate docs (`src/rust-leptos`, `src/rust-dioxus`); Flutter usage in `src/flutter/test`. All examples share the same policies, reject codes and session semantics — that is the point.
