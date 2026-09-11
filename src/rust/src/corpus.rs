//! Decode any `ores.dnd/v1` declaration by name — the runtime adapter used to
//! produce tjsv runtime evidence and to replay the shared instance corpus.

use crate::envelope::{
    DndDropResult, DndEnvelope, DndError, DndItem, DndItemKind, DndLifecyclePhase, DndOperation,
    DndTelemetryEvent, ValidationOptions,
};
use crate::policy::{DndDropPolicy, DndRejectCode};
use crate::session::{DndSessionInput, DndSessionInputKind, DndSessionSnapshot, DndSessionState, DndSessionTrace};

/// Every declaration the contract admits, in TypeSpec order.
pub const DECLARATIONS: [&str; 14] = [
    "DndOperation",
    "DndItemKind",
    "DndLifecyclePhase",
    "DndRejectCode",
    "DndSessionState",
    "DndSessionInputKind",
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
    serde_json::from_str::<T>(json).map(|_| ()).map_err(DndError::from)
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
        "DndItem" => structural::<DndItem>(json),
        "DndEnvelope" => {
            let envelope: DndEnvelope = serde_json::from_str(json)?;
            envelope.validate(ValidationOptions::default())
        }
        "DndDropResult" => structural::<DndDropResult>(json),
        "DndTelemetryEvent" => {
            let event: DndTelemetryEvent = serde_json::from_str(json)?;
            if event.item_count < 0 {
                return Err(DndError("itemCount must be >= 0".into()));
            }
            Ok(())
        }
        "DndDropPolicy" => {
            let policy: DndDropPolicy = serde_json::from_str(json)?;
            policy.validate().map_err(|code| DndError(format!("invalid policy: {}", code.wire())))
        }
        "DndSessionInput" => {
            let input: DndSessionInput = serde_json::from_str(json)?;
            if let Some(policy) = input.policy.as_ref() {
                policy.validate().map_err(|code| DndError(format!("invalid policy: {}", code.wire())))?;
            }
            Ok(())
        }
        "DndSessionSnapshot" => structural::<DndSessionSnapshot>(json),
        "DndSessionTrace" => {
            let trace: DndSessionTrace = serde_json::from_str(json)?;
            if trace.inputs.len() != trace.expected.len() {
                return Err(DndError("trace inputs and expected must have the same length".into()));
            }
            Ok(())
        }
        other => Err(DndError(format!("unknown declaration: {other}"))),
    }
}
