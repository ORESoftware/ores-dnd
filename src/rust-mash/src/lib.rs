//! `ores-dnd-mash` — drag-and-drop for MASH pages (maud + axum + htmx).
//!
//! MASH pages stay HTML-first: the server renders drop zones and drag sources
//! carrying stable `data-ores-dnd-*` attributes, the tiny `@oresoftware/ores-dnd`
//! TypeScript adapter (`htmx.ts` / `autoBind`) drives the drag session in the
//! browser, and an accepted drop is POSTed back as JSON to an axum endpoint
//! that re-runs the same policy evaluation with `ores-dnd-core` before
//! committing anything. The browser never decides on its own.

pub mod html;
#[cfg(feature = "axum")]
pub mod server;

pub use ores_dnd_core;
