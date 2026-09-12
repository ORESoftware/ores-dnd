//! `web_sys` glue shared by the Leptos components: DataTransfer read/write,
//! provisional envelopes for external drags, modifier-key preferences.
//! Mirrors `src/ts/dom.ts` so a Rust island and a TypeScript page interoperate.

use ores_dnd_core::{
    decode_envelope_json, effect_allowed_for, encode_envelope_json, DndEnvelope, DndError, DndItem, DndItemKind,
    DndOperation, ValidationOptions, ORES_DND_MIME, ORES_DND_PROTOCOL,
};
use std::sync::atomic::{AtomicU64, Ordering};

static EXTERNAL_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The item kind and canonical media type implied by a DataTransfer format
/// while the data is unreadable. Browser formats are not always media types
/// (`Files`, `downloadurl`, …): anything that is not a canonical `type/subtype`
/// is reported as `application/octet-stream` bytes. The ores MIME itself maps
/// to `None` (its items are only known once the payload is readable).
pub fn kind_for_type(format: &str) -> Option<(DndItemKind, String)> {
    let media = format.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if media == ORES_DND_MIME {
        return None;
    }
    if !ores_dnd_core::wire::is_media_type(&media) {
        return Some((DndItemKind::Bytes, "application/octet-stream".to_owned()));
    }
    let kind = match media.as_str() {
        "text/uri-list" => DndItemKind::Uri,
        "application/json" => DndItemKind::Json,
        m if m.starts_with("text/") => DndItemKind::Text,
        _ => DndItemKind::Bytes,
    };
    Some((kind, media))
}

/// An envelope describing an external drag by its advertised types only
/// (data is empty until `drop`). Lets a zone evaluate kind/media-type rules
/// during `dragover`; byte limits are re-checked definitively on drop.
pub fn provisional_envelope(types: &[String]) -> DndEnvelope {
    let n = EXTERNAL_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    let mut items: Vec<DndItem> = types
        .iter()
        .filter_map(|t| kind_for_type(t).map(|(kind, media_type)| DndItem { kind, media_type, data: String::new(), name: None }))
        .collect();
    items.truncate(ores_dnd_core::wire::ENVELOPE_ITEMS_MAX);
    if items.is_empty() {
        items.push(DndItem { kind: DndItemKind::Text, media_type: "text/plain".into(), data: String::new(), name: None });
    }
    DndEnvelope {
        protocol: ORES_DND_PROTOCOL.to_owned(),
        drag_id: format!("external-{n}"),
        source_runtime: "external-browser".to_owned(),
        allowed_operations: vec![DndOperation::Copy],
        items,
        traceparent: None,
        form_id: None,
    }
}

pub fn data_transfer_types(dt: &web_sys::DataTransfer) -> Vec<String> {
    let list = dt.types();
    (0..list.length()).filter_map(|i| list.get(i).as_string()).collect()
}

/// Read the ores envelope, or synthesize one from a `text/plain` drop.
pub fn read_envelope(dt: &web_sys::DataTransfer, options: ValidationOptions) -> Result<DndEnvelope, DndError> {
    if let Ok(json) = dt.get_data(ORES_DND_MIME) {
        if !json.is_empty() {
            return decode_envelope_json(&json, options);
        }
    }
    let text = dt.get_data("text/plain").unwrap_or_default();
    if text.is_empty() {
        return Err(DndError("no ores.dnd payload or text/plain fallback in DataTransfer".into()));
    }
    let n = EXTERNAL_COUNTER.fetch_add(1, Ordering::Relaxed) + 1;
    let envelope = DndEnvelope {
        protocol: ORES_DND_PROTOCOL.to_owned(),
        drag_id: format!("external-text-{n}"),
        source_runtime: "external-browser".to_owned(),
        allowed_operations: vec![DndOperation::Copy],
        items: vec![DndItem { kind: DndItemKind::Text, media_type: "text/plain".into(), data: text, name: None }],
        traceparent: None,
        form_id: None,
    };
    envelope.validate(options)?;
    Ok(envelope)
}

/// Write the ores MIME payload, `effectAllowed`, and a `text/plain` fallback.
pub fn write_envelope(dt: &web_sys::DataTransfer, envelope: &DndEnvelope) -> Result<(), DndError> {
    let json = encode_envelope_json(envelope, ValidationOptions::default())?;
    dt.set_data(ORES_DND_MIME, &json).map_err(|_| DndError("DataTransfer.setData failed".into()))?;
    dt.set_effect_allowed(effect_allowed_for(&envelope.allowed_operations));
    if let Some(text) = envelope.text_fallback() {
        let _ = dt.set_data("text/plain", text);
    }
    Ok(())
}

/// HTML5 `dropEffect` keyword for a negotiated operation.
pub const fn drop_effect_for(op: DndOperation) -> &'static str {
    op.wire()
}

/// Modifier keys express a preference the same way native file managers do:
/// Ctrl/⌥ → copy, Shift → move, Ctrl+Shift/⌘ → link.
pub fn preferred_operation(ev: &web_sys::DragEvent) -> Option<DndOperation> {
    match (ev.ctrl_key() || ev.alt_key(), ev.shift_key(), ev.meta_key()) {
        (true, true, _) | (_, _, true) => Some(DndOperation::Link),
        (true, false, false) => Some(DndOperation::Copy),
        (false, true, false) => Some(DndOperation::Move),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provisional_envelope_reflects_advertised_types() {
        let env = provisional_envelope(&["text/uri-list".into(), ORES_DND_MIME.into(), "Files".into()]);
        assert_eq!(env.items.len(), 2);
        assert_eq!(env.items[0].kind, DndItemKind::Uri);
        assert_eq!(env.items[1].kind, DndItemKind::Bytes);
        assert_eq!(env.items[1].media_type, "application/octet-stream", "browser formats that are not media types are canonicalised");
        assert!(env.validate(ValidationOptions::default()).is_ok());
        assert!(env.drag_id.starts_with("external-"));
        let empty = provisional_envelope(&[]);
        assert_eq!(empty.items[0].kind, DndItemKind::Text);
    }

    #[test]
    fn drop_effects_match_operation_wire_names() {
        assert_eq!(drop_effect_for(DndOperation::Move), "move");
        assert_eq!(kind_for_type("TEXT/Markdown; charset=utf-8"), Some((DndItemKind::Text, "text/markdown".to_owned())));
        assert_eq!(kind_for_type("downloadurl"), Some((DndItemKind::Bytes, "application/octet-stream".to_owned())));
        assert_eq!(kind_for_type(ORES_DND_MIME), None);
    }
}
