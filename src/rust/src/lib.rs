use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

#[cfg(feature = "reactive")]
pub mod reactive;
#[cfg(feature = "reactive")]
pub mod reactive_effects;

pub const ORES_DND_PROTOCOL: &str = "ores.dnd/v1";
pub const ORES_DND_MIME: &str = "application/vnd.ores.dnd+json";
pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
pub const DEFAULT_MAX_ITEMS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DndOperation {
    Copy,
    Move,
    Link,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DndItemKind {
    Text,
    Uri,
    Json,
    Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DndLifecyclePhase {
    DragStart,
    DragEnter,
    DragOver,
    DragLeave,
    Drop,
    DragEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DndItem {
    pub kind: DndItemKind,
    pub media_type: String,
    pub data: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DndEnvelope {
    pub protocol: String,
    pub drag_id: String,
    pub source_runtime: String,
    pub allowed_operations: Vec<DndOperation>,
    pub items: Vec<DndItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub form_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DndDropResult {
    pub drag_id: String,
    pub accepted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<DndOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DndTelemetryEvent {
    pub phase: DndLifecyclePhase,
    pub drag_id: String,
    pub source_runtime: String,
    pub item_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<DndOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct ValidationOptions {
    pub max_payload_bytes: usize,
    pub max_items: usize,
}

impl Default for ValidationOptions {
    fn default() -> Self {
        Self {
            max_payload_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
            max_items: DEFAULT_MAX_ITEMS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DndError(pub String);

impl fmt::Display for DndError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for DndError {}

impl From<serde_json::Error> for DndError {
    fn from(value: serde_json::Error) -> Self {
        Self(format!("invalid drag payload JSON: {value}"))
    }
}

fn require_non_empty(value: &str, label: &str) -> Result<(), DndError> {
    if value.is_empty() {
        Err(DndError(format!("{label} must be a non-empty string")))
    } else {
        Ok(())
    }
}

impl DndEnvelope {
    pub fn validate(&self, options: ValidationOptions) -> Result<(), DndError> {
        if self.protocol != ORES_DND_PROTOCOL {
            return Err(DndError(format!("unsupported drag protocol: {}", self.protocol)));
        }
        require_non_empty(&self.drag_id, "dragId")?;
        require_non_empty(&self.source_runtime, "sourceRuntime")?;
        if self.allowed_operations.is_empty() {
            return Err(DndError("allowedOperations must contain at least one operation".into()));
        }
        if self.items.is_empty() {
            return Err(DndError("items must contain at least one drag item".into()));
        }
        if self.items.len() > options.max_items {
            return Err(DndError(format!(
                "too many drag items: {} > {}",
                self.items.len(), options.max_items
            )));
        }
        for (index, item) in self.items.iter().enumerate() {
            require_non_empty(&item.media_type, &format!("items[{index}].mediaType"))?;
            if let Some(name) = item.name.as_deref() {
                require_non_empty(name, &format!("items[{index}].name"))?;
            }
        }
        if let Some(traceparent) = self.traceparent.as_deref() {
            require_non_empty(traceparent, "traceparent")?;
        }
        if let Some(form_id) = self.form_id.as_deref() {
            require_non_empty(form_id, "formId")?;
        }
        Ok(())
    }
}

pub fn decode_envelope_json(input: &str, options: ValidationOptions) -> Result<DndEnvelope, DndError> {
    if input.len() > options.max_payload_bytes {
        return Err(DndError(format!(
            "drag payload too large: {} > {} bytes",
            input.len(), options.max_payload_bytes
        )));
    }
    let envelope: DndEnvelope = serde_json::from_str(input)?;
    envelope.validate(options)?;
    Ok(envelope)
}

pub fn encode_envelope_json(
    envelope: &DndEnvelope,
    options: ValidationOptions,
) -> Result<String, DndError> {
    envelope.validate(options)?;
    let json = serde_json::to_string(envelope)?;
    if json.len() > options.max_payload_bytes {
        return Err(DndError(format!(
            "drag payload too large: {} > {} bytes",
            json.len(), options.max_payload_bytes
        )));
    }
    Ok(json)
}

pub fn negotiate_operation(
    source: &[DndOperation],
    target: &[DndOperation],
    preferred: Option<DndOperation>,
) -> Option<DndOperation> {
    if let Some(op) = preferred {
        if source.contains(&op) && target.contains(&op) {
            return Some(op);
        }
    }
    [DndOperation::Move, DndOperation::Copy, DndOperation::Link]
        .into_iter()
        .find(|op| source.contains(op) && target.contains(op))
}

pub fn telemetry_for(
    phase: DndLifecyclePhase,
    envelope: &DndEnvelope,
    operation: Option<DndOperation>,
    target_id: Option<String>,
) -> DndTelemetryEvent {
    DndTelemetryEvent {
        phase,
        drag_id: envelope.drag_id.clone(),
        source_runtime: envelope.source_runtime.clone(),
        item_count: envelope.items.len().min(i32::MAX as usize) as i32,
        operation,
        target_id,
    }
}

pub trait OresOtelPort {
    fn emit_dnd_event(&self, event: &DndTelemetryEvent) -> Result<(), DndError>;
}

pub trait OptoSyncPort {
    fn persist_accepted_drop(
        &self,
        envelope: &DndEnvelope,
        result: &DndDropResult,
    ) -> Result<(), DndError>;
}

pub trait OresFormsPort {
    fn apply_accepted_drop(
        &self,
        envelope: &DndEnvelope,
        result: &DndDropResult,
    ) -> Result<(), DndError>;
}

pub struct DropCommitPorts<'a> {
    pub otel: Option<&'a dyn OresOtelPort>,
    pub opto_sync: Option<&'a dyn OptoSyncPort>,
    pub forms: Option<&'a dyn OresFormsPort>,
}

pub fn commit_accepted_drop(
    envelope: &DndEnvelope,
    result: &DndDropResult,
    ports: DropCommitPorts<'_>,
) -> Result<(), DndError> {
    envelope.validate(ValidationOptions::default())?;
    if result.drag_id != envelope.drag_id {
        return Err(DndError("drop result dragId does not match envelope".into()));
    }
    if !result.accepted {
        return Ok(());
    }
    let operation = result
        .operation
        .ok_or_else(|| DndError("accepted drop requires an operation".into()))?;
    if !envelope.allowed_operations.contains(&operation) {
        return Err(DndError("accepted drop operation is not source-allowed".into()));
    }
    if let Some(forms) = ports.forms {
        forms.apply_accepted_drop(envelope, result)?;
    }
    if let Some(opto_sync) = ports.opto_sync {
        opto_sync.persist_accepted_drop(envelope, result)?;
    }
    if let Some(otel) = ports.otel {
        let event = telemetry_for(
            DndLifecyclePhase::Drop,
            envelope,
            Some(operation),
            result.target_id.clone(),
        );
        otel.emit_dnd_event(&event)?;
    }
    Ok(())
}

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
    fn new(zone_id: impl Into<String>) -> Self {
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
    /// Consumers bind on:dragstart/on:dragover/on:drop and call the wasm/core codec.
    pub fn drop_zone(zone_id: impl Into<String>) -> DomBinding {
        DomBinding::new(zone_id)
    }
}

#[cfg(feature = "dioxus")]
pub mod dioxus {
    use super::DomBinding;

    /// Framework-version-neutral binding metadata for Dioxus desktop/web event handlers.
    /// This avoids tying the protocol crate to a Dioxus release while preserving one codec.
    pub fn drop_zone(zone_id: impl Into<String>) -> DomBinding {
        DomBinding::new(zone_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");
    const INVALID_OP: &str = include_str!("../../../contracts/instances/DndEnvelope/invalid/unknown-op.json");

    #[test]
    fn shared_fixture_round_trips() {
        let env = decode_envelope_json(VALID, ValidationOptions::default()).unwrap();
        let encoded = encode_envelope_json(&env, ValidationOptions::default()).unwrap();
        let decoded = decode_envelope_json(&encoded, ValidationOptions::default()).unwrap();
        assert_eq!(decoded, env);
    }

    #[test]
    fn unknown_operation_fails_closed() {
        assert!(decode_envelope_json(INVALID_OP, ValidationOptions::default()).is_err());
    }

    #[test]
    fn unknown_property_fails_closed() {
        let with_unknown = VALID.replacen("\"protocol\"", "\"secret\":\"x\",\"protocol\"", 1);
        assert!(decode_envelope_json(&with_unknown, ValidationOptions::default()).is_err());
    }

    #[test]
    fn payload_limit_is_checked_before_parse() {
        let options = ValidationOptions { max_payload_bytes: 8, max_items: 64 };
        assert!(decode_envelope_json("this is not json", options).unwrap_err().0.contains("too large"));
    }

    #[test]
    fn negotiation_is_deterministic() {
        assert_eq!(
            negotiate_operation(
                &[DndOperation::Copy, DndOperation::Move],
                &[DndOperation::Copy, DndOperation::Move],
                None,
            ),
            Some(DndOperation::Move)
        );
        assert_eq!(
            negotiate_operation(&[DndOperation::Copy], &[DndOperation::Move], None),
            None
        );
    }

    #[test]
    fn telemetry_does_not_contain_item_data() {
        let env = decode_envelope_json(VALID, ValidationOptions::default()).unwrap();
        let event = telemetry_for(DndLifecyclePhase::Drop, &env, Some(DndOperation::Copy), None);
        let json = serde_json::to_string(&event).unwrap();
        assert!(!json.contains("hello"));
        assert_eq!(event.item_count, 1);
    }

    #[test]
    fn all_framework_bindings_share_the_same_mime() {
        #[cfg(feature = "mash")]
        assert_eq!(mash::drop_zone("z").mime_type, ORES_DND_MIME);
        #[cfg(feature = "leptos")]
        assert_eq!(leptos::drop_zone("z").mime_type, ORES_DND_MIME);
        #[cfg(feature = "dioxus")]
        assert_eq!(dioxus::drop_zone("z").mime_type, ORES_DND_MIME);
    }
}
