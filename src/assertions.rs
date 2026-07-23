//! Assertion helpers for the conformance test suite (impl spec §3.5).

use crate::kappa::KappaLabel;

pub fn assert_header_present(headers: &[(String, String)], name: &str) -> Result<(), String> {
    let lower = name.to_lowercase();
    if headers.iter().any(|(k, _)| k.to_lowercase() == lower) {
        Ok(())
    } else {
        Err(format!("expected header {name} present"))
    }
}

pub fn assert_header_value(
    headers: &[(String, String)],
    name: &str,
    expected: &str,
) -> Result<(), String> {
    let lower = name.to_lowercase();
    match headers.iter().find(|(k, _)| k.to_lowercase() == lower) {
        Some((_, v)) if v == expected => Ok(()),
        Some((_, v)) => Err(format!(
            "header {name}: expected \"{expected}\", got \"{v}\""
        )),
        None => Err(format!("header {name} not present")),
    }
}

pub fn assert_body_eq(body: &[u8], expected: &[u8]) -> Result<(), String> {
    if body == expected {
        Ok(())
    } else {
        Err(format!(
            "body mismatch: got {} bytes, expected {} bytes",
            body.len(),
            expected.len()
        ))
    }
}

pub fn assert_kappa_matches(body: &[u8], expected: &KappaLabel) -> Result<(), String> {
    let computed = match expected.axis() {
        "sha256" => KappaLabel::sha256(body),
        "blake3" => KappaLabel::blake3(body),
        _ => {
            return Err(format!(
                "unsupported axis for verification: {}",
                expected.axis()
            ))
        }
    };
    if computed == *expected {
        Ok(())
    } else {
        Err(format!(
            "κ mismatch: computed {}, expected {}",
            computed.as_str(),
            expected.as_str()
        ))
    }
}

pub fn assert_kappa_valid(s: &str) -> Result<KappaLabel, String> {
    KappaLabel::parse(s).map_err(|e| format!("invalid κ-label \"{s}\": {e}"))
}

/// Verify the section 6.16 error response format: `{"errors":[{"code":"<EXPECTED>",...}]}`.
pub fn verify_error_response(body: &[u8], expected_code: &str) -> Result<(), String> {
    let text = std::str::from_utf8(body).map_err(|e| format!("body not UTF-8: {e}"))?;
    let v: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("body not JSON: {e}"))?;
    let errors = v
        .get("errors")
        .and_then(|e| e.as_array())
        .ok_or("missing errors array")?;
    let first = errors.first().ok_or("errors array empty")?;
    let code = first
        .get("code")
        .and_then(|c| c.as_str())
        .ok_or("missing code field")?;

    // Verify uppercase-underscore format.
    if !code.bytes().all(|b| b.is_ascii_uppercase() || b == b'_') {
        return Err(format!("code \"{code}\" is not uppercase/underscore"));
    }
    if code != expected_code {
        return Err(format!(
            "expected error code \"{expected_code}\", got \"{code}\""
        ));
    }
    Ok(())
}
