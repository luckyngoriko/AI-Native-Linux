//! Shared identifier helpers for S1.3 prefix-namespaced IDs.

use ulid::Ulid;

/// Validate `<prefix><ULID>` and return the owned canonical string.
pub fn validate_prefixed_ulid(
    input: &str,
    expected_prefix: &'static str,
) -> Result<String, String> {
    if input.is_empty() {
        return Err("identifier is empty".to_owned());
    }

    let Some(body) = input.strip_prefix(expected_prefix) else {
        return Err(format!("expected prefix {expected_prefix}, got {input}"));
    };

    Ulid::from_string(body).map_err(|err| format!("invalid ULID body for {input}: {err}"))?;

    Ok(input.to_owned())
}

/// Mint a fresh `<prefix><ULID>` string.
pub fn fresh_prefixed_ulid(prefix: &'static str) -> String {
    format!("{prefix}{}", Ulid::new())
}

/// Validate a full lowercase-hex `chk_` BLAKE3 chunk id.
pub fn validate_chunk_id(input: &str) -> Result<String, String> {
    const PREFIX: &str = "chk_";
    const HASH_HEX_LEN: usize = 64;

    if input.is_empty() {
        return Err("chunk identifier is empty".to_owned());
    }

    let Some(body) = input.strip_prefix(PREFIX) else {
        return Err(format!("expected prefix {PREFIX}, got {input}"));
    };

    if body.len() != HASH_HEX_LEN {
        return Err(format!(
            "expected {HASH_HEX_LEN}-char BLAKE3 hex body, got {} chars",
            body.len()
        ));
    }

    if !body
        .chars()
        .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    {
        return Err("chunk body must be lowercase hex [0-9a-f]".to_owned());
    }

    Ok(input.to_owned())
}
