//! maud markup helpers. Attribute names are the shared constants from
//! `ores_dnd_core::bindings`, so the TypeScript adapter finds them by convention.

use maud::{html, Markup, PreEscaped};
use ores_dnd_core::bindings::{ATTR_POLICY, ATTR_SOURCE, ATTR_STATE, ATTR_ZONE};
use ores_dnd_core::{
    encode_envelope_json, DndDropPolicy, DndEnvelope, DndError, ValidationOptions,
};

/// Attribute carrying the URL an accepted drop is POSTed to (JSON `DropCommitRequest`).
pub const ATTR_COMMIT: &str = "data-ores-dnd-commit";
/// Optional attribute: CSS selector whose innerHTML is swapped with an HTML
/// commit response (then `htmx.process`ed) — the htmx-style partial update.
pub const ATTR_SWAP: &str = "data-ores-dnd-swap";

/// How a drop zone reports an accepted drop back to the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropZoneWiring<'a> {
    /// Endpoint accepting `DropCommitRequest` JSON (see [`crate::server`]).
    pub commit_url: &'a str,
    /// Optional CSS selector to swap an HTML response into.
    pub swap_target: Option<&'a str>,
    /// Extra classes for the zone element.
    pub class: Option<&'a str>,
}

impl<'a> DropZoneWiring<'a> {
    pub fn new(commit_url: &'a str) -> Self {
        Self {
            commit_url,
            swap_target: None,
            class: None,
        }
    }
}

/// Render a drop zone. `inner` is the zone's content.
///
/// ```html
/// <div data-ores-dnd-zone="zone-a" data-ores-dnd-policy='{"targetId":"zone-a",…}'
///      data-ores-dnd-state="idle" data-ores-dnd-commit="/drops">…</div>
/// ```
pub fn drop_zone(
    policy: &DndDropPolicy,
    wiring: &DropZoneWiring<'_>,
    inner: Markup,
) -> Result<Markup, DndError> {
    policy
        .validate()
        .map_err(|code| DndError(format!("invalid drop policy: {}", code.wire())))?;
    let policy_json = serde_json::to_string(policy)?;
    Ok(html! {
        div
            class=[wiring.class]
            data-ores-dnd-zone=(policy.target_id)
            data-ores-dnd-policy=(policy_json)
            data-ores-dnd-state="idle"
            data-ores-dnd-commit=(wiring.commit_url)
            data-ores-dnd-swap=[wiring.swap_target]
        { (inner) }
    })
}

/// Render a drag source: a `draggable` element carrying its envelope so the
/// browser adapter can write the ores MIME type into `DataTransfer` on `dragstart`.
pub fn drag_source(
    envelope: &DndEnvelope,
    class: Option<&str>,
    inner: Markup,
) -> Result<Markup, DndError> {
    let json = encode_envelope_json(envelope, ValidationOptions::default())?;
    Ok(html! {
        div draggable="true" class=[class] data-ores-dnd-source=(json) { (inner) }
    })
}

/// The `<script type="module">` tag that boots the browser adapter for every
/// zone/source on the page. `module_url` is wherever the app serves
/// `@oresoftware/ores-dnd` (zed-pkg installs it under `.vendor/.zed`).
pub fn boot_script(module_url: &str) -> Markup {
    let url = serde_json::to_string(module_url).unwrap_or_else(|_| "\"\"".to_owned());
    html! {
        script type="module" {
            (PreEscaped(format!("import {{ autoBind }} from {url}; autoBind(document);")))
        }
    }
}

/// The attribute names, for templates that render their own elements.
pub const ZONE_ATTRIBUTES: [&str; 5] = [ATTR_ZONE, ATTR_POLICY, ATTR_STATE, ATTR_COMMIT, ATTR_SWAP];
/// The drag-source attribute name.
pub const SOURCE_ATTRIBUTE: &str = ATTR_SOURCE;

#[cfg(test)]
mod tests {
    use super::*;
    use ores_dnd_core::{decode_envelope_json, DndItemKind, DndOperation};

    const VALID: &str =
        include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");

    #[test]
    fn drop_zone_carries_policy_and_wiring() {
        let policy = DndDropPolicy::new("zone-a", &[DndOperation::Copy], &[DndItemKind::Text])
            .with_max_items(3);
        let wiring = DropZoneWiring {
            commit_url: "/drops",
            swap_target: Some("#list"),
            class: Some("zone"),
        };
        let markup = drop_zone(&policy, &wiring, html! { p { "drop here" } })
            .unwrap()
            .into_string();
        assert!(markup.starts_with("<div class=\"zone\" data-ores-dnd-zone=\"zone-a\""));
        assert!(markup.contains("data-ores-dnd-policy=\"{&quot;targetId&quot;:&quot;zone-a&quot;"));
        assert!(markup.contains("&quot;maxItems&quot;:3"));
        assert!(markup.contains("data-ores-dnd-state=\"idle\" data-ores-dnd-commit=\"/drops\" data-ores-dnd-swap=\"#list\""));
        assert!(markup.ends_with("<p>drop here</p></div>"));
    }

    #[test]
    fn invalid_policy_is_refused() {
        let policy = DndDropPolicy::new("zone-a", &[DndOperation::Copy], &[DndItemKind::Text])
            .with_max_items(0);
        assert!(drop_zone(&policy, &DropZoneWiring::new("/drops"), html! {}).is_err());
    }

    #[test]
    fn drag_source_embeds_the_envelope_escaped() {
        let envelope = decode_envelope_json(VALID, ValidationOptions::default()).unwrap();
        let markup = drag_source(&envelope, None, html! { "card" })
            .unwrap()
            .into_string();
        assert!(markup.starts_with("<div draggable=\"true\" data-ores-dnd-source=\"{&quot;protocol&quot;:&quot;ores.dnd/v1&quot;"));
        assert!(!markup.contains("<script"));
    }

    #[test]
    fn boot_script_is_a_module_import() {
        let markup = boot_script("/vendor/ores-dnd/index.js").into_string();
        assert_eq!(markup, "<script type=\"module\">import { autoBind } from \"/vendor/ores-dnd/index.js\"; autoBind(document);</script>");
    }
}
