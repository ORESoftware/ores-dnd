//! `ores-dnd-wasm` — the WASM boundary of the `ores.dnd/v1` core for JS
//! clients, Flutter web bridges and Rust desktop webviews.
//!
//! Every export speaks JSON strings so the ABI is identical for
//! `wasm-bindgen` glue, the WIT world in `wit/ores-dnd.wit`, and any host that
//! only has string passing. Errors are thrown as JS strings.

use ores_dnd_core::{
    corpus::decode_declaration, decode_envelope_json, encode_envelope_json, evaluate_policy,
    negotiate_operation, DndDropPolicy, DndOperation, DndSession, DndSessionInput, DndSessionTrace,
    ValidationOptions, ORES_DND_MIME, ORES_DND_PROTOCOL,
};
use wasm_bindgen::prelude::*;

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
}

/// Host-neutral implementations (string errors) so they can be unit-tested on
/// native targets; the `#[wasm_bindgen]` exports below are thin wrappers.
pub mod api {
    use super::*;

    pub fn evaluate_policy_json(envelope_json: &str, policy_json: &str, preferred: Option<&str>) -> Result<String, String> {
        let envelope = decode_envelope_json(envelope_json, ValidationOptions::default()).map_err(|e| e.to_string())?;
        let policy: DndDropPolicy = serde_json::from_str(policy_json).map_err(|e| e.to_string())?;
        policy.validate().map_err(|code| format!("invalid policy: {}", code.wire()))?;
        let preferred = match preferred {
            Some(value) => Some(DndOperation::parse(value).ok_or_else(|| format!("unsupported drag operation: {value}"))?),
            None => None,
        };
        let verdict = match evaluate_policy(&envelope, &policy, preferred) {
            Ok(operation) => serde_json::json!({ "accepted": true, "operation": operation.wire() }),
            Err(code) => serde_json::json!({ "accepted": false, "errorCode": code.wire() }),
        };
        Ok(verdict.to_string())
    }

    pub fn negotiate_operation_json(source_json: &str, target_json: &str, preferred: Option<&str>) -> Result<Option<String>, String> {
        let source: Vec<DndOperation> = serde_json::from_str(source_json).map_err(|e| e.to_string())?;
        let target: Vec<DndOperation> = serde_json::from_str(target_json).map_err(|e| e.to_string())?;
        let preferred = match preferred {
            Some(value) => Some(DndOperation::parse(value).ok_or_else(|| format!("unsupported drag operation: {value}"))?),
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
            let input: DndSessionInput = serde_json::from_str(input_json).map_err(|e| e.to_string())?;
            serde_json::to_string(self.inner.apply(&input)).map_err(|e| e.to_string())
        }
        pub fn snapshot(&self) -> Result<String, String> {
            serde_json::to_string(self.inner.snapshot()).map_err(|e| e.to_string())
        }
        pub fn envelope(&self) -> Result<Option<String>, String> {
            self.inner.envelope().map(|e| serde_json::to_string(e).map_err(|e| e.to_string())).transpose()
        }
        pub fn result(&self) -> Result<Option<String>, String> {
            self.inner.snapshot().result().map(|r| serde_json::to_string(&r).map_err(|e| e.to_string())).transpose()
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
        Self { inner: api::JsonSession::default() }
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

#[cfg(test)]
mod tests {
    use super::*;

    const VALID: &str = include_str!("../../../contracts/instances/DndEnvelope/valid/text-copy.json");

    #[test]
    fn wasm_session_speaks_json_end_to_end() {
        let mut session = api::JsonSession::default();
        let start = format!("{{\"kind\":\"start\",\"envelope\":{VALID}}}");
        let snapshot = session.apply(&start).unwrap();
        assert!(snapshot.contains("\"state\":\"dragging\""));
        let enter = r#"{"kind":"enter","targetId":"zone-a","policy":{"targetId":"zone-a","allowedOperations":["copy"],"acceptedKinds":["text"]}}"#;
        assert!(session.apply(enter).unwrap().contains("\"operation\":\"copy\""));
        assert!(session.result().unwrap().is_none());
        session.apply(r#"{"kind":"drop","targetId":"zone-a"}"#).unwrap();
        assert!(session.result().unwrap().unwrap().contains("\"accepted\":true"));
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
        assert_eq!(api::negotiate_operation_json(r#"["copy","move"]"#, r#"["move"]"#, None).unwrap().as_deref(), Some("move"));
        let trace = include_str!("../../../contracts/instances/DndSessionTrace/valid/basic-drop.json");
        assert_eq!(api::replay_trace_json(trace).unwrap(), None);
    }
}
