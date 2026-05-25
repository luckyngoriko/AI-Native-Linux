//! Tier-1 pure deterministic primitive executors.

use regex::Regex;
use serde_json::{json, Value};

use crate::{PrimitiveResult, VerificationPrimitive};

use super::{
    optional_bool, primitive_result, required_bool, required_str, required_u64, string_array,
    ProbeVerdict,
};

/// Compare a JSON field selected by `path` with an expected value.
#[must_use]
pub fn json_field_eq(expected: &Value, actual_root: &Value) -> ProbeVerdict {
    let path = match path_segments(expected) {
        Ok(path) => path,
        Err(error) => return error,
    };
    let Some(expected_value) = expected.get("expected") else {
        return ProbeVerdict::probe_error("missing required field `expected`");
    };
    let actual = path
        .iter()
        .try_fold(actual_root, |cursor, segment| cursor.get(segment))
        .cloned()
        .unwrap_or(Value::Null);

    if actual == *expected_value {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

/// Compare BLAKE3 over inline UTF-8 bytes with `expected_hash_hex`.
#[must_use]
pub fn blake3_matches(expected: &Value) -> ProbeVerdict {
    let bytes = match required_str(expected, "bytes") {
        Ok(bytes) => bytes,
        Err(error) => return error,
    };
    let expected_hash = match required_str(expected, "expected_hash_hex") {
        Ok(hash) => hash,
        Err(error) => return error,
    };
    let observed_hash = blake3::hash(bytes.as_bytes()).to_hex().to_string();
    let actual = json!({"observed_hash": observed_hash});

    if observed_hash == expected_hash {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

/// Match `text` against a regular expression `pattern`.
#[must_use]
pub fn regex_matches(expected: &Value) -> ProbeVerdict {
    let text = match required_str(expected, "text") {
        Ok(text) => text,
        Err(error) => return error,
    };
    let pattern = match required_str(expected, "pattern") {
        Ok(pattern) => pattern,
        Err(error) => return error,
    };
    let regex = match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(error) => {
            return ProbeVerdict::probe_error(format!("invalid regex pattern: {error}"));
        }
    };
    let matched = regex.is_match(text);
    let actual = json!({"matched": matched});

    if matched {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

/// Execute a Tier-1 S2.4 primitive over a supplied JSON payload.
#[must_use]
pub fn execute(primitive: VerificationPrimitive, expected: &Value) -> PrimitiveResult {
    let verdict = match primitive {
        VerificationPrimitive::TreeMaxDepth => tree_max_depth(expected),
        VerificationPrimitive::WebChromeZIndexAtLeast => web_chrome_z_index_at_least(expected),
        VerificationPrimitive::NamespaceCatalogVersion => namespace_catalog_version(expected),
        VerificationPrimitive::SubjectSessionFlagState => subject_session_flag_state(expected),
        VerificationPrimitive::SecretPatternMatch => secret_pattern_match(expected),
        other => ProbeVerdict::probe_error(format!("{other} is not a Tier-1 primitive")),
    };

    primitive_result(primitive, expected, verdict)
}

fn tree_max_depth(expected: &Value) -> ProbeVerdict {
    let max_depth = match required_u64(expected, "max_depth") {
        Ok(max_depth) => max_depth,
        Err(error) => return error,
    };
    let observed_depth = match required_u64(expected, "observed_depth") {
        Ok(observed_depth) => observed_depth,
        Err(error) => return error,
    };
    let actual = json!({"observed_depth": observed_depth});

    if observed_depth <= max_depth {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

fn web_chrome_z_index_at_least(expected: &Value) -> ProbeVerdict {
    let minimum = match required_u64(expected, "minimum_z_index") {
        Ok(minimum) => minimum,
        Err(error) => return error,
    };
    let observed = match required_u64(expected, "observed_z_index") {
        Ok(observed) => observed,
        Err(error) => return error,
    };
    let actual = json!({"observed_z_index": observed});

    if observed >= minimum {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

fn namespace_catalog_version(expected: &Value) -> ProbeVerdict {
    let expected_catalog = match required_str(expected, "expected_catalog_id") {
        Ok(expected_catalog) => expected_catalog,
        Err(error) => return error,
    };
    let observed_catalog = match required_str(expected, "observed_catalog_id") {
        Ok(observed_catalog) => observed_catalog,
        Err(error) => return error,
    };
    let require_exact = optional_bool(expected, "require_exact_match", true);
    let supersedes = optional_bool(expected, "observed_supersedes_expected", false);
    let actual = json!({"observed_catalog_id": observed_catalog});
    let passed = observed_catalog == expected_catalog || (!require_exact && supersedes);

    if passed {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

fn subject_session_flag_state(expected: &Value) -> ProbeVerdict {
    let expected_state = match required_bool(expected, "expected_state") {
        Ok(expected_state) => expected_state,
        Err(error) => return error,
    };
    let observed_state = match required_bool(expected, "observed_state") {
        Ok(observed_state) => observed_state,
        Err(error) => return error,
    };
    let actual = json!({"observed_state": observed_state});

    if observed_state == expected_state {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

fn secret_pattern_match(expected: &Value) -> ProbeVerdict {
    let text =
        match required_str(expected, "text").or_else(|_err| required_str(expected, "payload")) {
            Ok(text) => text,
            Err(error) => return error,
        };
    let patterns = match patterns(expected) {
        Ok(patterns) => patterns,
        Err(error) => return error,
    };
    let mut total_hits = 0_u64;

    for pattern in patterns {
        let regex = match Regex::new(&pattern) {
            Ok(regex) => regex,
            Err(error) => {
                return ProbeVerdict::probe_error(format!("invalid regex pattern: {error}"));
            }
        };
        total_hits += u64::try_from(regex.find_iter(text).count()).unwrap_or(u64::MAX);
    }

    let expected_match = optional_bool(expected, "expected_match", false);
    let actual = json!({"total_hits": total_hits});
    let matched = total_hits > 0;

    if matched == expected_match {
        ProbeVerdict::passed(actual)
    } else {
        ProbeVerdict::failed(actual)
    }
}

fn patterns(expected: &Value) -> Result<Vec<String>, ProbeVerdict> {
    if expected.get("patterns").is_some() {
        string_array(expected, "patterns")
    } else {
        required_str(expected, "pattern").map(|pattern| vec![pattern.to_owned()])
    }
}

fn path_segments(expected: &Value) -> Result<Vec<String>, ProbeVerdict> {
    match expected.get("path") {
        Some(Value::Array(_)) => string_array(expected, "path"),
        Some(Value::String(path)) => Ok(path.split('.').map(str::to_owned).collect()),
        Some(_) => Err(ProbeVerdict::probe_error(
            "field `path` must be a string or string array",
        )),
        None => Err(ProbeVerdict::probe_error("missing required field `path`")),
    }
}
