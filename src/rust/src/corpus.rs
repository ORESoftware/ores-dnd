//! Decode any `ores.dnd/v1` declaration by name — the runtime adapter used to
//! produce tjsv runtime evidence and to replay the shared instance corpus.

use crate::envelope::{
    DndDropResult, DndEnvelope, DndError, DndItem, DndItemKind, DndLifecyclePhase, DndOperation,
    DndTelemetryEvent, ValidationOptions,
};
use crate::policy::{DndDropPolicy, DndRejectCode};
use crate::session::{
    DndSessionInput, DndSessionInputKind, DndSessionSnapshot, DndSessionState, DndSessionTrace,
};
use crate::wire;

/// Every declaration the contract admits, in TypeSpec order.
pub const DECLARATIONS: [&str; 20] = [
    "DndOperation",
    "DndItemKind",
    "DndLifecyclePhase",
    "DndRejectCode",
    "DndSessionState",
    "DndSessionInputKind",
    "SafeId",
    "ProtocolId",
    "MediaType",
    "MediaTypePattern",
    "Traceparent",
    "ErrorCode",
    "DndItem",
    "DndEnvelope",
    "DndDropResult",
    "DndTelemetryEvent",
    "DndDropPolicy",
    "DndSessionInput",
    "DndSessionSnapshot",
    "DndSessionTrace",
];

fn structural<T: serde::de::DeserializeOwned>(json: &str) -> Result<(), DndError> {
    serde_json::from_str::<T>(json)
        .map(|_| ())
        .map_err(DndError::from)
}

fn scalar(json: &str, label: &str, ok: impl Fn(&str) -> bool) -> Result<(), DndError> {
    let value: String = serde_json::from_str(json)?;
    if ok(&value) {
        Ok(())
    } else {
        Err(DndError(format!("{label} rejected: {value}")))
    }
}

/// Structural decode (closed enums, no unknown properties, contract bounds)
/// for the named declaration. `DndEnvelope` additionally runs semantic
/// validation so that a decoded envelope is always usable.
pub fn decode_declaration(declaration: &str, json: &str) -> Result<(), DndError> {
    match declaration {
        "DndOperation" => structural::<DndOperation>(json),
        "DndItemKind" => structural::<DndItemKind>(json),
        "DndLifecyclePhase" => structural::<DndLifecyclePhase>(json),
        "DndRejectCode" => structural::<DndRejectCode>(json),
        "DndSessionState" => structural::<DndSessionState>(json),
        "DndSessionInputKind" => structural::<DndSessionInputKind>(json),
        "SafeId" => scalar(json, "SafeId", wire::is_safe_id),
        "ProtocolId" => scalar(json, "ProtocolId", wire::is_protocol_id),
        "MediaType" => scalar(json, "MediaType", wire::is_media_type),
        "MediaTypePattern" => scalar(json, "MediaTypePattern", wire::is_media_type_pattern),
        "Traceparent" => scalar(json, "Traceparent", wire::is_traceparent),
        "ErrorCode" => scalar(json, "ErrorCode", wire::is_error_code),
        "DndItem" => serde_json::from_str::<DndItem>(json)?.structural(),
        "DndEnvelope" => {
            serde_json::from_str::<DndEnvelope>(json)?.validate(ValidationOptions::default())
        }
        "DndDropResult" => serde_json::from_str::<DndDropResult>(json)?.structural(),
        "DndTelemetryEvent" => serde_json::from_str::<DndTelemetryEvent>(json)?.structural(),
        "DndDropPolicy" => serde_json::from_str::<DndDropPolicy>(json)?.structural(),
        "DndSessionInput" => serde_json::from_str::<DndSessionInput>(json)?.structural(),
        "DndSessionSnapshot" => serde_json::from_str::<DndSessionSnapshot>(json)?.structural(),
        "DndSessionTrace" => serde_json::from_str::<DndSessionTrace>(json)?.structural(),
        other => Err(DndError(format!("unknown declaration: {other}"))),
    }
}
