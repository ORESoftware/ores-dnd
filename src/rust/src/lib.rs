//! `ores-dnd-core` — the `ores.dnd/v1` codec, drop policy and drag session
//! state machine shared by every ORESoftware runtime.
//!
//! - [`envelope`]: wire types, codec, operation negotiation, telemetry shape
//! - [`policy`]: what a target accepts, evaluated in the fleet-wide order
//! - [`session`]: the pure state machine every runtime replays identically
//! - [`ports`]: ores-forms → opto-sync → ores-otel commit ports
//! - [`bindings`]: stable DOM attribute/event names for HTML-first adapters
//! - [`corpus`]: decode any declaration by name (tjsv runtime evidence)
//!
//! Framework crates (`ores-dnd-mash`, `ores-dnd-leptos`, `ores-dnd-dioxus`,
//! `ores-dnd-wasm`) depend on this crate, never the other way round.

pub mod bindings;
pub mod corpus;
pub mod envelope;
pub mod policy;
pub mod ports;
pub mod session;

pub use bindings::{DomBinding, ATTR_POLICY, ATTR_SOURCE, ATTR_STATE, ATTR_ZONE, EVENT_DROP, EVENT_STATE};
#[cfg(feature = "dioxus")]
pub use bindings::dioxus;
#[cfg(feature = "leptos")]
pub use bindings::leptos;
#[cfg(feature = "mash")]
pub use bindings::mash;
pub use envelope::{
    decode_envelope_json, effect_allowed_for, encode_envelope_json, negotiate_operation, telemetry_for,
    DndDropResult, DndEnvelope, DndError, DndItem, DndItemKind, DndLifecyclePhase, DndOperation,
    DndTelemetryEvent, ValidationOptions, DEFAULT_MAX_ITEMS, DEFAULT_MAX_PAYLOAD_BYTES, ORES_DND_MIME,
    ORES_DND_PROTOCOL,
};
pub use policy::{evaluate_policy, media_type_matches, DndDropPolicy, DndRejectCode};
pub use ports::{commit_accepted_drop, emit_phase, DropCommitPorts, OptoSyncPort, OresFormsPort, OresOtelPort};
pub use session::{
    DndSession, DndSessionInput, DndSessionInputKind, DndSessionSnapshot, DndSessionState, DndSessionTrace,
    TraceDivergence,
};
