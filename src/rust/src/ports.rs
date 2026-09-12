//! Integration ports. The core never touches application state: a host wires
//! its ores-forms, opto-sync and ores-otel adapters here and the core calls
//! them — in that order — only for an accepted drop.

use crate::envelope::{
    telemetry_for, DndDropResult, DndEnvelope, DndError, DndLifecyclePhase, DndTelemetryEvent,
    ValidationOptions,
};

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

#[derive(Default, Clone, Copy)]
pub struct DropCommitPorts<'a> {
    pub otel: Option<&'a dyn OresOtelPort>,
    pub opto_sync: Option<&'a dyn OptoSyncPort>,
    pub forms: Option<&'a dyn OresFormsPort>,
}

/// Run the opt-in side effects for an accepted drop: forms → opto-sync → otel.
/// A rejected result is a no-op; a mismatched or unallowed result is an error.
pub fn commit_accepted_drop(
    envelope: &DndEnvelope,
    result: &DndDropResult,
    ports: DropCommitPorts<'_>,
) -> Result<(), DndError> {
    envelope.validate(ValidationOptions::default())?;
    if result.drag_id != envelope.drag_id {
        return Err(DndError(
            "drop result dragId does not match envelope".into(),
        ));
    }
    if !result.accepted {
        return Ok(());
    }
    let operation = result
        .operation
        .ok_or_else(|| DndError("accepted drop requires an operation".into()))?;
    if !envelope.allowed_operations.contains(&operation) {
        return Err(DndError(
            "accepted drop operation is not source-allowed".into(),
        ));
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

/// Emit a content-free lifecycle event for a non-drop phase (start, enter, …).
pub fn emit_phase(
    otel: Option<&dyn OresOtelPort>,
    phase: DndLifecyclePhase,
    envelope: &DndEnvelope,
    operation: Option<crate::envelope::DndOperation>,
    target_id: Option<String>,
) -> Result<(), DndError> {
    match otel {
        Some(port) => port.emit_dnd_event(&telemetry_for(phase, envelope, operation, target_id)),
        None => Ok(()),
    }
}
