//! Drop policies: what a target accepts, evaluated in the fixed order every
//! runtime shares (see docs/DESIGN.md §Policy evaluation).

use crate::envelope::{negotiate_operation, DndEnvelope, DndError, DndItemKind, DndOperation};
use crate::wire;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DndRejectCode {
    InvalidEnvelope,
    NoCommonOperation,
    ItemKindNotAccepted,
    MediaTypeNotAccepted,
    TooManyItems,
    PayloadTooLarge,
    FormMismatch,
    NoActiveTarget,
    TargetMismatch,
    Cancelled,
}

impl DndRejectCode {
    pub const fn wire(self) -> &'static str {
        match self {
            DndRejectCode::InvalidEnvelope => "invalid-envelope",
            DndRejectCode::NoCommonOperation => "no-common-operation",
            DndRejectCode::ItemKindNotAccepted => "item-kind-not-accepted",
            DndRejectCode::MediaTypeNotAccepted => "media-type-not-accepted",
            DndRejectCode::TooManyItems => "too-many-items",
            DndRejectCode::PayloadTooLarge => "payload-too-large",
            DndRejectCode::FormMismatch => "form-mismatch",
            DndRejectCode::NoActiveTarget => "no-active-target",
            DndRejectCode::TargetMismatch => "target-mismatch",
            DndRejectCode::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DndDropPolicy {
    pub target_id: String,
    pub allowed_operations: Vec<DndOperation>,
    pub accepted_kinds: Vec<DndItemKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_media_types: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_items: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_bytes: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form_id: Option<String>,
}

impl DndDropPolicy {
    /// A policy accepting the given kinds with every operation and no limits.
    pub fn new(target_id: impl Into<String>, operations: &[DndOperation], kinds: &[DndItemKind]) -> Self {
        Self {
            target_id: target_id.into(),
            allowed_operations: operations.to_vec(),
            accepted_kinds: kinds.to_vec(),
            accepted_media_types: None,
            max_items: None,
            max_total_bytes: None,
            form_id: None,
        }
    }

    pub fn with_media_types<I: IntoIterator<Item = S>, S: Into<String>>(mut self, media_types: I) -> Self {
        self.accepted_media_types = Some(media_types.into_iter().map(Into::into).collect());
        self
    }

    pub fn with_max_items(mut self, max_items: i32) -> Self {
        self.max_items = Some(max_items);
        self
    }

    pub fn with_max_total_bytes(mut self, max_total_bytes: i32) -> Self {
        self.max_total_bytes = Some(max_total_bytes);
        self
    }

    pub fn with_form_id(mut self, form_id: impl Into<String>) -> Self {
        self.form_id = Some(form_id.into());
        self
    }

    /// The structural rules both schema authorities check for a policy.
    pub fn structural(&self) -> Result<(), DndError> {
        wire::check_safe_id(&self.target_id, "targetId")?;
        wire::check_len(self.allowed_operations.len(), 1, wire::OPERATIONS_MAX, "allowedOperations")?;
        wire::check_len(self.accepted_kinds.len(), 1, wire::KINDS_MAX, "acceptedKinds")?;
        if let Some(patterns) = self.accepted_media_types.as_deref() {
            wire::check_len(patterns.len(), 1, wire::MEDIA_PATTERNS_MAX, "acceptedMediaTypes")?;
            if let Some(bad) = patterns.iter().find(|p| !wire::is_media_type_pattern(p)) {
                return Err(DndError(format!("acceptedMediaTypes entry is not a canonical media type pattern: {bad}")));
            }
        }
        if self.max_items.is_some_and(|v| !(1..=wire::POLICY_MAX_ITEMS_MAX).contains(&v)) {
            return Err(DndError("maxItems must be 1..=64".into()));
        }
        if self.max_total_bytes.is_some_and(|v| v < 1) {
            return Err(DndError("maxTotalBytes must be >= 1".into()));
        }
        wire::check_opt_safe_id(self.form_id.as_deref(), "formId")
    }

    /// Structural sanity as a reject code (the session ignores invalid policies).
    pub fn validate(&self) -> Result<(), DndRejectCode> {
        self.structural().map_err(|_| DndRejectCode::InvalidEnvelope)
    }
}

/// `pattern` is an exact media type or a `type/*` wildcard; comparison is
/// ASCII case-insensitive and ignores parameters after `;`.
pub fn media_type_matches(pattern: &str, media_type: &str) -> bool {
    let media = media_type.split(';').next().unwrap_or("").trim();
    let pattern = pattern.trim();
    if let Some(prefix) = pattern.strip_suffix("/*") {
        media
            .split_once('/')
            .is_some_and(|(top, _)| top.eq_ignore_ascii_case(prefix))
    } else {
        media.eq_ignore_ascii_case(pattern)
    }
}

/// Evaluate `policy` against `envelope`; `Ok` carries the negotiated operation.
///
/// Order: operation → item kind → media type → item count → total bytes → form.
pub fn evaluate_policy(
    envelope: &DndEnvelope,
    policy: &DndDropPolicy,
    preferred: Option<DndOperation>,
) -> Result<DndOperation, DndRejectCode> {
    let operation = negotiate_operation(&envelope.allowed_operations, &policy.allowed_operations, preferred)
        .ok_or(DndRejectCode::NoCommonOperation)?;
    if envelope.items.iter().any(|item| !policy.accepted_kinds.contains(&item.kind)) {
        return Err(DndRejectCode::ItemKindNotAccepted);
    }
    if let Some(patterns) = policy.accepted_media_types.as_deref() {
        let all_match = envelope
            .items
            .iter()
            .all(|item| patterns.iter().any(|pattern| media_type_matches(pattern, &item.media_type)));
        if !all_match {
            return Err(DndRejectCode::MediaTypeNotAccepted);
        }
    }
    if let Some(max_items) = policy.max_items {
        if envelope.items.len() > max_items.max(0) as usize {
            return Err(DndRejectCode::TooManyItems);
        }
    }
    if let Some(max_bytes) = policy.max_total_bytes {
        if envelope.total_data_bytes() > max_bytes.max(0) as usize {
            return Err(DndRejectCode::PayloadTooLarge);
        }
    }
    if let (Some(policy_form), Some(envelope_form)) = (policy.form_id.as_deref(), envelope.form_id.as_deref()) {
        if policy_form != envelope_form {
            return Err(DndRejectCode::FormMismatch);
        }
    }
    Ok(operation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_type_matching_is_case_insensitive_and_supports_wildcards() {
        assert!(media_type_matches("text/plain", "TEXT/Plain; charset=utf-8"));
        assert!(media_type_matches("text/*", "text/markdown"));
        assert!(!media_type_matches("text/*", "image/png"));
        assert!(!media_type_matches("text/plain", "text/markdown"));
        assert!(!media_type_matches("text/*", "text"));
    }

    #[test]
    fn policy_bounds_are_enforced() {
        let mut policy = DndDropPolicy::new("z", &[DndOperation::Copy], &[DndItemKind::Text]);
        assert!(policy.validate().is_ok());
        policy.max_items = Some(0);
        assert_eq!(policy.validate(), Err(DndRejectCode::InvalidEnvelope));
    }
}
