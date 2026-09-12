use ores_dnd_core::{
    decode_envelope_json, encode_envelope_json, negotiate_operation, DndOperation,
    ValidationOptions, ORES_DND_MIME, ORES_DND_PROTOCOL,
};
use wasm_bindgen::prelude::*;

fn js_error(error: impl ToString) -> JsValue {
    JsValue::from_str(&error.to_string())
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
    let preferred = match preferred {
        Some(value) => {
            Some(serde_json::from_str::<DndOperation>(&format!("\"{value}\"")).map_err(js_error)?)
        }
        None => None,
    };
    Ok(
        negotiate_operation(&source, &target, preferred).map(|op| match op {
            DndOperation::Copy => "copy".to_owned(),
            DndOperation::Move => "move".to_owned(),
            DndOperation::Link => "link".to_owned(),
        }),
    )
}
