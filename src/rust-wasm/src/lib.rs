use ores_dnd_core::{
    decode_envelope_json, encode_envelope_json, negotiate_operation,
    reactive::{
        reactive_state_for, reactive_telemetry_for, DndLifecycleGuard, DndLifecycleMode,
        DndReactiveEvent,
    },
    DndError, DndLifecyclePhase, DndOperation, ValidationOptions, ORES_DND_MIME,
    ORES_DND_PROTOCOL,
};
use wasm_bindgen::prelude::*;

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}

fn parse_phase(value: &str) -> Result<DndLifecyclePhase, DndError> {
    match value {
        "drag-start" => Ok(DndLifecyclePhase::DragStart),
        "drag-enter" => Ok(DndLifecyclePhase::DragEnter),
        "drag-over" => Ok(DndLifecyclePhase::DragOver),
        "drag-leave" => Ok(DndLifecyclePhase::DragLeave),
        "drop" => Ok(DndLifecyclePhase::Drop),
        "drag-end" => Ok(DndLifecyclePhase::DragEnd),
        _ => Err(DndError("unsupported lifecycle phase".to_owned())),
    }
}

fn parse_operation(value: Option<String>) -> Result<Option<DndOperation>, DndError> {
    value
        .map(|value| match value.as_str() {
            "copy" => Ok(DndOperation::Copy),
            "move" => Ok(DndOperation::Move),
            "link" => Ok(DndOperation::Link),
            _ => Err(DndError("unsupported drag operation".to_owned())),
        })
        .transpose()
}

fn project_reactive_event(
    guard: &mut DndLifecycleGuard,
    phase: &str,
    envelope_json: &str,
    operation: Option<String>,
    target_id: Option<String>,
) -> Result<String, DndError> {
    let envelope = decode_envelope_json(envelope_json, ValidationOptions::default())?;
    let event = DndReactiveEvent::new(
        parse_phase(phase)?,
        envelope,
        parse_operation(operation)?,
        target_id,
    )?;
    guard.accept(&event)?;

    let mut state = reactive_state_for(&event);
    // The guard is authoritative for external one-shot drops: those are
    // terminal observations, not synthetic active drags.
    state.active = guard.active();
    let telemetry = reactive_telemetry_for(&event);
    serde_json::to_string(&serde_json::json!({
        "state": state,
        "telemetry": telemetry,
    }))
    .map_err(DndError::from)
}

#[wasm_bindgen]
pub fn protocol_version() -> String {
    ORES_DND_PROTOCOL.to_owned()
}

#[wasm_bindgen]
pub fn mime_type() -> String {
    ORES_DND_MIME.to_owned()
}

#[wasm_bindgen]
pub fn normalize_envelope_json(input: &str) -> Result<String, JsValue> {
    let options = ValidationOptions::default();
    let envelope = decode_envelope_json(input, options).map_err(js_error)?;
    encode_envelope_json(&envelope, options).map_err(js_error)
}

#[wasm_bindgen]
pub fn validate_envelope_json(input: &str) -> Result<(), JsValue> {
    decode_envelope_json(input, ValidationOptions::default())
        .map(|_| ())
        .map_err(js_error)
}

#[wasm_bindgen]
pub fn negotiate_operation_json(
    source_json: &str,
    target_json: &str,
    preferred: Option<String>,
) -> Result<Option<String>, JsValue> {
    let source: Vec<DndOperation> = serde_json::from_str(source_json).map_err(js_error)?;
    let target: Vec<DndOperation> = serde_json::from_str(target_json).map_err(js_error)?;
    let preferred = parse_operation(preferred).map_err(js_error)?;
    Ok(negotiate_operation(&source, &target, preferred).map(|op| match op {
        DndOperation::Copy => "copy".to_owned(),
        DndOperation::Move => "move".to_owned(),
        DndOperation::Link => "link".to_owned(),
    }))
}

/// Opaque guarded lifecycle state for JavaScript/WebView/Flutter-WASM hosts.
/// Raw dragged item data is accepted for validation but is never returned from
/// `emit_json`; only the sanitized reactive state and telemetry projections
/// cross the WASM boundary.
#[wasm_bindgen]
pub struct DndLifecycleHandle {
    guard: DndLifecycleGuard,
}

#[wasm_bindgen]
impl DndLifecycleHandle {
    #[wasm_bindgen(constructor)]
    pub fn new(external_drop_compatible: bool) -> Self {
        let mode = if external_drop_compatible {
            DndLifecycleMode::ExternalDropCompatible
        } else {
            DndLifecycleMode::Strict
        };
        Self {
            guard: DndLifecycleGuard::new(mode),
        }
    }

    #[wasm_bindgen]
    pub fn emit_json(
        &mut self,
        phase: &str,
        envelope_json: &str,
        operation: Option<String>,
        target_id: Option<String>,
    ) -> Result<String, JsValue> {
        project_reactive_event(
            &mut self.guard,
            phase,
            envelope_json,
            operation,
            target_id,
        )
        .map_err(js_error)
    }

    #[wasm_bindgen(getter)]
    pub fn active(&self) -> bool {
        self.guard.active()
    }

    #[wasm_bindgen]
    pub fn active_drag_id(&self) -> Option<String> {
        self.guard.active_drag_id().map(ToOwned::to_owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");

    #[test]
    fn strict_projection_rejects_out_of_order_event() {
        let mut guard = DndLifecycleGuard::new(DndLifecycleMode::Strict);
        let error = project_reactive_event(&mut guard, "drag-over", VALID, None, None)
            .expect_err("strict guard must reject drag-over before drag-start");
        assert!(error.0.contains("active drag-start"));
    }

    #[test]
    fn projections_never_return_dragged_payload_data() -> Result<(), DndError> {
        let mut guard = DndLifecycleGuard::new(DndLifecycleMode::Strict);
        let start = project_reactive_event(&mut guard, "drag-start", VALID, None, None)?;
        assert!(!start.contains("hello"));
        assert!(guard.active());

        let drop = project_reactive_event(
            &mut guard,
            "drop",
            VALID,
            Some("copy".to_owned()),
            Some("zone-a".to_owned()),
        )?;
        assert!(!drop.contains("hello"));
        assert!(guard.active());

        let end = project_reactive_event(&mut guard, "drag-end", VALID, None, None)?;
        assert!(!end.contains("hello"));
        assert!(!guard.active());
        let json: serde_json::Value = serde_json::from_str(&end)?;
        assert_eq!(json["state"]["active"], false);
        assert_eq!(json["telemetry"]["phase"], "drag-end");
        Ok(())
    }

    #[test]
    fn external_one_shot_drop_remains_inactive() -> Result<(), DndError> {
        let mut guard = DndLifecycleGuard::new(DndLifecycleMode::ExternalDropCompatible);
        let projection = project_reactive_event(
            &mut guard,
            "drop",
            VALID,
            Some("copy".to_owned()),
            Some("zone-a".to_owned()),
        )?;
        assert!(!guard.active());
        let json: serde_json::Value = serde_json::from_str(&projection)?;
        assert_eq!(json["state"]["active"], false);
        assert!(!projection.contains("hello"));
        Ok(())
    }

    #[test]
    fn parser_errors_do_not_echo_untrusted_values() {
        let phase = parse_phase("secret-phase-value").expect_err("unsupported phase");
        assert!(!phase.0.contains("secret-phase-value"));
        let operation = parse_operation(Some("secret-op-value".to_owned()))
            .expect_err("unsupported operation");
        assert!(!operation.0.contains("secret-op-value"));
    }
}
