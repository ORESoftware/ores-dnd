//! Wire types and codec for the `ores.dnd/v1` envelope.
//!
//! Everything here mirrors `contracts/main.tsp` / `contracts/authored.schema.json`
//! field-for-field. Unknown properties, operations, kinds and protocol versions
//! fail closed.

use crate::wire;
use serde::{Deserialize, Serialize};
use std::{error::Error, fmt};

pub const ORES_DND_PROTOCOL: &str = "ores.dnd/v1";
pub const ORES_DND_MIME: &str = "application/vnd.ores.dnd+json";
pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
pub const DEFAULT_MAX_ITEMS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DndOperation {
    Copy,
    Move,
    Link,
}

impl DndOperation {
    /// Deterministic negotiation order shared by every runtime.
    pub const NEGOTIATION_ORDER: [DndOperation; 3] =
        [DndOperation::Move, DndOperation::Copy, DndOperation::Link];

    pub const fn wire(self) -> &'static str {
        match self {
            DndOperation::Copy => "copy",
            DndOperation::Move => "move",
            DndOperation::Link => "link",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "copy" => Some(DndOperation::Copy),
            "move" => Some(DndOperation::Move),
            "link" => Some(DndOperation::Link),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DndItemKind {
    Text,
    Uri,
    Json,
    Bytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub traceparent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DndDropResult {
    pub drag_id: String,
    pub accepted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<DndOperation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
}

impl DndDropResult {
    pub fn structural(&self) -> Result<(), DndError> {
        wire::check_safe_id(&self.drag_id, "dragId")?;
        wire::check_opt_safe_id(self.target_id.as_deref(), "targetId")?;
        match self.error_code.as_deref() {
            Some(code) if !wire::is_error_code(code) => Err(DndError("errorCode must be lowercase kebab-case (1..=64)".into())),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DndTelemetryEvent {
    pub phase: DndLifecyclePhase,
    pub drag_id: String,
    pub source_runtime: String,
    pub item_count: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<DndOperation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
}

impl DndTelemetryEvent {
    pub fn structural(&self) -> Result<(), DndError> {
        wire::check_safe_id(&self.drag_id, "dragId")?;
        wire::check_safe_id(&self.source_runtime, "sourceRuntime")?;
        wire::check_opt_safe_id(self.target_id.as_deref(), "targetId")?;
        if self.item_count < 0 {
            return Err(DndError("itemCount must be >= 0".into()));
        }
        Ok(())
    }
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

impl DndItem {
    /// The structural rules both schema authorities check for an item.
    pub fn structural(&self) -> Result<(), DndError> {
        if !wire::is_media_type(&self.media_type) {
            return Err(DndError("mediaType must be a canonical lowercase type/subtype".into()));
        }
        if self.data.chars().count() > wire::ITEM_DATA_MAX_CHARS {
            return Err(DndError("data exceeds the contract maximum length".into()));
        }
        if let Some(name) = self.name.as_deref() {
            if name.is_empty() || name.chars().count() > wire::ITEM_NAME_MAX {
                return Err(DndError("name must be 1..=255 characters".into()));
            }
        }
        Ok(())
    }
}

impl DndEnvelope {
    /// The structural rules both schema authorities check (bounded ids,
    /// protocol shape, media types, traceparent, array bounds) — everything
    /// except the protocol *version* this runtime accepts.
    pub fn structural(&self) -> Result<(), DndError> {
        if !wire::is_protocol_id(&self.protocol) {
            return Err(DndError(format!("malformed drag protocol tag: {}", self.protocol)));
        }
        wire::check_safe_id(&self.drag_id, "dragId")?;
        wire::check_safe_id(&self.source_runtime, "sourceRuntime")?;
        wire::check_len(self.allowed_operations.len(), 1, wire::OPERATIONS_MAX, "allowedOperations")?;
        wire::check_len(self.items.len(), 1, wire::ENVELOPE_ITEMS_MAX, "items")?;
        for (index, item) in self.items.iter().enumerate() {
            item.structural().map_err(|e| DndError(format!("items[{index}]: {e}")))?;
        }
        if let Some(traceparent) = self.traceparent.as_deref() {
            if !wire::is_traceparent(traceparent) {
                return Err(DndError("traceparent must be a W3C trace-context value".into()));
            }
        }
        wire::check_opt_safe_id(self.form_id.as_deref(), "formId")
    }

    /// Semantic validation on top of the structural (serde) decode: the
    /// structural rules plus the protocol version and the host's item limit.
    pub fn validate(&self, options: ValidationOptions) -> Result<(), DndError> {
        self.structural()?;
        if self.protocol != ORES_DND_PROTOCOL {
            return Err(DndError(format!("unsupported drag protocol: {}", self.protocol)));
        }
        if self.items.len() > options.max_items {
            return Err(DndError(format!(
                "too many drag items: {} > {}",
                self.items.len(),
                options.max_items
            )));
        }
        Ok(())
    }

    /// Total UTF-8 byte length of all item data (the `maxTotalBytes` measure).
    pub fn total_data_bytes(&self) -> usize {
        self.items.iter().map(|item| item.data.len()).sum()
    }

    /// The plain-text fallback emitted next to the ores MIME type, if any.
    pub fn text_fallback(&self) -> Option<&str> {
        self.items
            .iter()
            .find(|item| item.kind == DndItemKind::Text && item.media_type == "text/plain")
            .map(|item| item.data.as_str())
    }
}

pub fn decode_envelope_json(input: &str, options: ValidationOptions) -> Result<DndEnvelope, DndError> {
    if input.len() > options.max_payload_bytes {
        return Err(DndError(format!(
            "drag payload too large: {} > {} bytes",
            input.len(),
            options.max_payload_bytes
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
            json.len(),
            options.max_payload_bytes
        )));
    }
    Ok(json)
}

/// The preferred operation when both sides allow it, else the first of
/// move → copy → link allowed by both, else `None`.
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
    DndOperation::NEGOTIATION_ORDER
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

/// The HTML5 `effectAllowed` keyword for a set of operations.
pub fn effect_allowed_for(ops: &[DndOperation]) -> &'static str {
    let copy = ops.contains(&DndOperation::Copy);
    let mv = ops.contains(&DndOperation::Move);
    let link = ops.contains(&DndOperation::Link);
    match (copy, mv, link) {
        (true, true, true) => "all",
        (true, true, false) => "copyMove",
        (true, false, true) => "copyLink",
        (false, true, true) => "linkMove",
        (true, false, false) => "copy",
        (false, true, false) => "move",
        (false, false, true) => "link",
        (false, false, false) => "none",
    }
}
