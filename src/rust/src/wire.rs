//! Wire-level (structural) rules shared by every declaration: the bounded
//! scalars of `contracts/main.tsp` (`SafeId`, `ProtocolId`, `MediaType`,
//! `MediaTypePattern`, `Traceparent`, `ErrorCode`) and the array/length bounds.
//! These are exactly what both schema authorities check, implemented without a
//! regex engine so the core stays dependency-light.

use crate::envelope::DndError;

pub const SAFE_ID_MAX: usize = 128;
pub const MEDIA_TYPE_MAX: usize = 255;
pub const ERROR_CODE_MAX: usize = 64;
pub const ITEM_NAME_MAX: usize = 255;
pub const ITEM_DATA_MAX_CHARS: usize = 1_048_576;
pub const ENVELOPE_ITEMS_MAX: usize = 64;
pub const OPERATIONS_MAX: usize = 3;
pub const KINDS_MAX: usize = 4;
pub const MEDIA_PATTERNS_MAX: usize = 64;
pub const POLICY_MAX_ITEMS_MAX: i32 = 64;
pub const TRACE_ID_MAX: usize = 128;
pub const TRACE_DESCRIPTION_MAX: usize = 512;
pub const TRACE_STEPS_MAX: usize = 256;

fn err(label: &str, what: &str) -> DndError {
    DndError(format!("{label} {what}"))
}

/// `^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$`
pub fn is_safe_id(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    value.len() <= SAFE_ID_MAX
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
}

pub fn check_safe_id(value: &str, label: &str) -> Result<(), DndError> {
    if is_safe_id(value) {
        Ok(())
    } else {
        Err(err(label, "must match ^[A-Za-z0-9][A-Za-z0-9._:-]{0,127}$"))
    }
}

pub fn check_opt_safe_id(value: Option<&str>, label: &str) -> Result<(), DndError> {
    value.map_or(Ok(()), |v| check_safe_id(v, label))
}

/// `^ores\.dnd/v[1-9][0-9]{0,2}$`
pub fn is_protocol_id(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("ores.dnd/v") else {
        return false;
    };
    let bytes = rest.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 3
        && bytes[0].is_ascii_digit()
        && bytes[0] != b'0'
        && bytes.iter().all(u8::is_ascii_digit)
}

fn is_media_token(token: &str) -> bool {
    let bytes = token.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 127
        && bytes[0].is_ascii_lowercase() | bytes[0].is_ascii_digit()
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || matches!(
                    b,
                    b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-'
                )
        })
}

/// `^[a-z0-9][a-z0-9!#$&^_.+-]{0,126}/[a-z0-9][a-z0-9!#$&^_.+-]{0,126}$` (3..=255 chars)
pub fn is_media_type(value: &str) -> bool {
    value.len() >= 3
        && value.len() <= MEDIA_TYPE_MAX
        && value
            .split_once('/')
            .is_some_and(|(t, s)| is_media_token(t) && is_media_token(s))
}

/// Like [`is_media_type`] but the subtype may be `*`.
pub fn is_media_type_pattern(value: &str) -> bool {
    value.len() >= 3
        && value.len() <= MEDIA_TYPE_MAX
        && value
            .split_once('/')
            .is_some_and(|(t, s)| is_media_token(t) && (s == "*" || is_media_token(s)))
}

/// `^[0-9a-f]{2}-[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{2}$`
pub fn is_traceparent(value: &str) -> bool {
    let parts: Vec<&str> = value.split('-').collect();
    parts.len() == 4
        && [2usize, 32, 16, 2].iter().zip(&parts).all(|(len, part)| {
            part.len() == *len
                && part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

/// `^[a-z0-9][a-z0-9-]{0,63}$`
pub fn is_error_code(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= ERROR_CODE_MAX
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

/// `^[a-z0-9][a-z0-9._-]{0,127}$`
pub fn is_trace_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= TRACE_ID_MAX
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && bytes.iter().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

pub fn check_len(len: usize, min: usize, max: usize, label: &str) -> Result<(), DndError> {
    if len < min || len > max {
        Err(err(
            label,
            &format!("must have between {min} and {max} entries"),
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_match_the_contract_patterns() {
        assert!(
            is_safe_id("drag-0001") && is_safe_id("A1.b_c:d-e") && is_safe_id(&"x".repeat(128))
        );
        assert!(
            !is_safe_id("")
                && !is_safe_id("-lead")
                && !is_safe_id("a b")
                && !is_safe_id(&"x".repeat(129))
                && !is_safe_id("é")
        );
        assert!(is_protocol_id("ores.dnd/v1") && is_protocol_id("ores.dnd/v999"));
        assert!(
            !is_protocol_id("ores.dnd/v0")
                && !is_protocol_id("ores.dnd/v")
                && !is_protocol_id("ores.dnd/v1000")
                && !is_protocol_id("ORES.DND/v1")
        );
        assert!(is_media_type("text/plain") && is_media_type("application/vnd.ores.dnd+json"));
        assert!(
            !is_media_type("text")
                && !is_media_type("Text/Plain")
                && !is_media_type("text/plain; charset=utf-8")
                && !is_media_type("text/*")
        );
        assert!(
            is_media_type_pattern("text/*")
                && !is_media_type_pattern("*/*")
                && !is_media_type_pattern("text/")
        );
        assert!(is_traceparent(
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        ));
        assert!(
            !is_traceparent("00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01")
                && !is_traceparent("abc")
        );
        assert!(
            is_error_code("http-500")
                && !is_error_code("Cancelled")
                && !is_error_code("a b")
                && !is_error_code(&"x".repeat(65))
        );
        assert!(is_trace_id("basic-drop") && !is_trace_id("Basic-Drop"));
    }
}
