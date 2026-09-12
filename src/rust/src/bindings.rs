//! Stable DOM attribute/event names shared by the HTML-first adapters (MASH
//! pages, Leptos islands, Dioxus web) and the TypeScript `dom.ts` adapter.

use crate::envelope::ORES_DND_MIME;
use crate::policy::DndDropPolicy;

/// Attribute carrying the zone id on a drop target element.
pub const ATTR_ZONE: &str = "data-ores-dnd-zone";
/// Attribute carrying the JSON-encoded `DndDropPolicy` on a drop target element.
pub const ATTR_POLICY: &str = "data-ores-dnd-policy";
/// Attribute the adapters keep in sync with the session state for CSS hooks.
pub const ATTR_STATE: &str = "data-ores-dnd-state";
/// Attribute carrying the JSON-encoded `DndEnvelope` on a drag source element.
pub const ATTR_SOURCE: &str = "data-ores-dnd-source";
/// DOM CustomEvent name dispatched on a zone after an accepted drop
/// (`detail` = `{ envelope, result }`).
pub const EVENT_DROP: &str = "ores-dnd:drop";
/// DOM CustomEvent name dispatched on a zone when the session snapshot changes.
pub const EVENT_STATE: &str = "ores-dnd:state";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomBinding {
    pub zone_id: String,
    pub draggable_attribute: &'static str,
    pub mime_type: &'static str,
    pub drag_start_event: &'static str,
    pub drag_over_event: &'static str,
    pub drop_event: &'static str,
}

impl DomBinding {
    pub fn new(zone_id: impl Into<String>) -> Self {
        Self {
            zone_id: zone_id.into(),
            draggable_attribute: "true",
            mime_type: ORES_DND_MIME,
            drag_start_event: "dragstart",
            drag_over_event: "dragover",
            drop_event: "drop",
        }
    }
}

/// The `(name, value)` attributes a drop zone element carries so the TypeScript
/// adapter (`autoBind`) can drive it without any framework-specific code.
pub fn zone_attributes(policy: &DndDropPolicy) -> Result<Vec<(&'static str, String)>, serde_json::Error> {
    Ok(vec![
        (ATTR_ZONE, policy.target_id.clone()),
        (ATTR_POLICY, serde_json::to_string(policy)?),
        (ATTR_STATE, "idle".to_owned()),
    ])
}

#[cfg(feature = "mash")]
pub mod mash {
    use super::DomBinding;

    /// Maud/Axum/HTMX stays HTML-first. Render these stable attributes and let the
    /// tiny ores-dnd TypeScript/WASM adapter own DataTransfer serialization.
    pub fn drop_zone(zone_id: impl Into<String>) -> DomBinding {
        DomBinding::new(zone_id)
    }
}

#[cfg(feature = "leptos")]
pub mod leptos {
    use super::DomBinding;

    /// Framework-version-neutral binding metadata for Leptos event handlers.
    pub fn drop_zone(zone_id: impl Into<String>) -> DomBinding {
        DomBinding::new(zone_id)
    }
}

#[cfg(feature = "dioxus")]
pub mod dioxus {
    use super::DomBinding;

    /// Framework-version-neutral binding metadata for Dioxus desktop/web event handlers.
    pub fn drop_zone(zone_id: impl Into<String>) -> DomBinding {
        DomBinding::new(zone_id)
    }
}
