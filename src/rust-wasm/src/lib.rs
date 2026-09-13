//! `ores-dnd-wasm` — the WASM boundary of the `ores.dnd/v1` core for JS
//! clients, Flutter web bridges and Rust desktop webviews.
//!
//! Every export speaks JSON strings so the ABI is identical for
//! `wasm-bindgen` glue, the WIT world in `wit/ores-dnd.wit`, and any host that
//! only has string passing. Errors are thrown as JS strings.

use ores_dnd_core::{
    corpus::decode_declaration,
    decode_envelope_json, encode_envelope_json, evaluate_policy, negotiate_operation,
    reactive::{
        reactive_state_for, reactive_telemetry_for, DndLifecycleGuard, DndLifecycleMode,
        DndReactiveEvent,
    },
    DndDropPolicy, DndError, DndLifecyclePhase, DndOperation, DndSession, DndSessionInput,
    DndSessionTrace, ValidationOptions, ORES_DND_MIME, ORES_DND_PROTOCOL,
};
use wasm_bindgen::prelude::*;

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Host-neutral implementations (string errors) so they can be unit-tested on
/// native targets; the `#[wasm_bindgen]` exports below are thin wrappers.
pub mod api {
    use super::*;

    pub fn evaluate_policy_json(
        envelope_json: &str,
        policy_json: &str,
        preferred: Option<&str>,
    ) -> Result<String, String> {
        let envelope = decode_envelope_json(envelope_json, ValidationOptions::default())
            .map_err(|e| e.to_string())?;
        let policy: DndDropPolicy = serde_json::from_str(policy_json).map_err(|e| e.to_string())?;
        policy
            .validate()
            .map_err(|code| format!("invalid policy: {}", code.wire()))?;
        let preferred = match preferred {
            Some(value) => Some(
                DndOperation::parse(value)
                    .ok_or_else(|| "unsupported drag operation".to_owned())?,
            ),
            None => None,
        };
        let verdict = match evaluate_policy(&envelope, &policy, preferred) {
            Ok(operation) => serde_json::json!({ "accepted": true, "operation": operation.wire() }),
            Err(code) => serde_json::json!({ "accepted": false, "errorCode": code.wire() }),
        };
        Ok(verdict.to_string())
    }

    pub fn negotiate_operation_json(
        source_json: &str,
        target_json: &str,
        preferred: Option<&str>,
    ) -> Result<Option<String>, String> {
        let source: Vec<DndOperation> =
            serde_json::from_str(source_json).map_err(|e| e.to_string())?;
        let target: Vec<DndOperation> =
            serde_json::from_str(target_json).map_err(|e| e.to_string())?;
        let preferred = match preferred {
            Some(value) => Some(
                DndOperation::parse(value)
                    .ok_or_else(|| "unsupported drag operation".to_owned())?,
            ),
            None => None,
        };
        Ok(negotiate_operation(&source, &target, preferred).map(|op| op.wire().to_owned()))
    }

    pub fn replay_trace_json(trace_json: &str) -> Result<Option<String>, String> {
        let trace: DndSessionTrace = serde_json::from_str(trace_json).map_err(|e| e.to_string())?;
        Ok(DndSession::replay(&trace).err().map(|d| d.to_string()))
    }

    /// The session core shared by [`super::WasmDndSession`] and native hosts.
    #[derive(Default)]
    pub struct JsonSession {
        inner: DndSession,
    }

    impl JsonSession {
        pub fn apply(&mut self, input_json: &str) -> Result<String, String> {
            let input: DndSessionInput =
                serde_json::from_str(input_json).map_err(|e| e.to_string())?;
            serde_json::to_string(self.inner.apply(&input)).map_err(|e| e.to_string())
        }
        pub fn snapshot(&self) -> Result<String, String> {
            serde_json::to_string(self.inner.snapshot()).map_err(|e| e.to_string())
        }
        pub fn envelope(&self) -> Result<Option<String>, String> {
            self.inner
                .envelope()
                .map(|e| serde_json::to_string(e).map_err(|e| e.to_string()))
                .transpose()
        }
        pub fn result(&self) -> Result<Option<String>, String> {
            self.inner
                .snapshot()
                .result()
                .map(|r| serde_json::to_string(&r).map_err(|e| e.to_string()))
                .transpose()
        }
    }
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

/// Structural + semantic decode of any contract declaration by name; the
/// verdict used for tjsv runtime evidence from a WASM host.
#[wasm_bindgen]
pub fn validate_declaration_json(declaration: &str, input: &str) -> Result<(), JsValue> {
    decode_declaration(declaration, input).map_err(js_error)
}

#[wasm_bindgen]
pub fn negotiate_operation_json(
    source_json: &str,
    target_json: &str,
    preferred: Option<String>,
) -> Result<Option<String>, JsValue> {
    api::negotiate_operation_json(source_json, target_json, preferred.as_deref()).map_err(js_error)
}

/// Evaluate a `DndDropPolicy` (JSON) against a `DndEnvelope` (JSON).
/// Returns `{"accepted":true,"operation":"move"}` or
/// `{"accepted":false,"errorCode":"item-kind-not-accepted"}`.
#[wasm_bindgen]
pub fn evaluate_policy_json(
    envelope_json: &str,
    policy_json: &str,
    preferred: Option<String>,
) -> Result<String, JsValue> {
    api::evaluate_policy_json(envelope_json, policy_json, preferred.as_deref()).map_err(js_error)
}

/// Replay a `DndSessionTrace` (JSON); returns `null` on success or the
/// divergence description.
#[wasm_bindgen]
pub fn replay_trace_json(trace_json: &str) -> Result<Option<String>, JsValue> {
    api::replay_trace_json(trace_json).map_err(js_error)
}

/// A drag session driven over the WASM boundary: feed `DndSessionInput` JSON,
/// read `DndSessionSnapshot` JSON back.
#[wasm_bindgen]
pub struct WasmDndSession {
    inner: api::JsonSession,
}

impl Default for WasmDndSession {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl WasmDndSession {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: api::JsonSession::default(),
        }
    }

    /// Apply one `DndSessionInput` (JSON) and return the new snapshot (JSON).
    pub fn apply(&mut self, input_json: &str) -> Result<String, JsValue> {
        self.inner.apply(input_json).map_err(js_error)
    }

    /// The current `DndSessionSnapshot` (JSON).
    pub fn snapshot(&self) -> Result<String, JsValue> {
        self.inner.snapshot().map_err(js_error)
    }

    /// The running session's envelope (JSON) or `null`.
    pub fn envelope(&self) -> Result<Option<String>, JsValue> {
        self.inner.envelope().map_err(js_error)
    }

    /// The terminal `DndDropResult` (JSON) or `null` while the session runs.
    pub fn result(&self) -> Result<Option<String>, JsValue> {
        self.inner.result().map_err(js_error)
    }
}

// ---------------------------------------------------------------------------
// Guarded reactive lifecycle for WASM hosts (DEN-3926)
// ---------------------------------------------------------------------------

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

/// Untrusted values are never echoed into error text.
fn parse_operation(value: Option<String>) -> Result<Option<DndOperation>, DndError> {
    value
        .map(|value| {
            DndOperation::parse(&value)
                .ok_or_else(|| DndError("unsupported drag operation".to_owned()))
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
        project_reactive_event(&mut self.guard, phase, envelope_json, operation, target_id)
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

    const VALID: &str =
        include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");

    #[test]
    fn wasm_session_speaks_json_end_to_end() {
        let mut session = api::JsonSession::default();
        let start = format!("{{\"kind\":\"start\",\"envelope\":{VALID}}}");
        let snapshot = session.apply(&start).unwrap();
        assert!(snapshot.contains("\"state\":\"dragging\""));
        let enter = r#"{"kind":"enter","targetId":"zone-a","policy":{"targetId":"zone-a","allowedOperations":["copy"],"acceptedKinds":["text"]}}"#;
        assert!(session
            .apply(enter)
            .unwrap()
            .contains("\"operation\":\"copy\""));
        assert!(session.result().unwrap().is_none());
        session
            .apply(r#"{"kind":"drop","targetId":"zone-a"}"#)
            .unwrap();
        assert!(session
            .result()
            .unwrap()
            .unwrap()
            .contains("\"accepted\":true"));
        assert!(session.envelope().unwrap().unwrap().contains("drag-0001"));
    }

    #[test]
    fn policy_evaluation_is_exposed() {
        let policy = r#"{"targetId":"z","allowedOperations":["link"],"acceptedKinds":["text"]}"#;
        assert_eq!(
            api::evaluate_policy_json(VALID, policy, None).unwrap(),
            r#"{"accepted":false,"errorCode":"no-common-operation"}"#
        );
        assert!(api::evaluate_policy_json(VALID, policy, Some("teleport")).is_err());
        assert!(decode_declaration("DndDropPolicy", policy).is_ok());
        assert!(decode_declaration("DndDropPolicy", r#"{"targetId":"z"}"#).is_err());
        assert_eq!(
            api::negotiate_operation_json(r#"["copy","move"]"#, r#"["move"]"#, None)
                .unwrap()
                .as_deref(),
            Some("move")
        );
        let trace =
            include_str!("../../../contracts/instances/DndSessionTrace/valid/basic-drop.json");
        assert_eq!(api::replay_trace_json(trace).unwrap(), None);
    }

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
        let operation =
            parse_operation(Some("secret-op-value".to_owned())).expect_err("unsupported operation");
        assert!(!operation.0.contains("secret-op-value"));
        let negotiated =
            api::negotiate_operation_json("[\"copy\"]", "[\"copy\"]", Some("secret-op-value"))
                .expect_err("unsupported preferred operation");
        assert!(!negotiated.contains("secret-op-value"));
    }
}
