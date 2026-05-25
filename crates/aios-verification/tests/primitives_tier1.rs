//! Tier-1 deterministic primitive helper coverage for T-066.

use std::error::Error;
use std::sync::Arc;

use aios_action::ActionId;
use aios_verification::primitives::tier1::{blake3_matches, json_field_eq, regex_matches};
use aios_verification::primitives::tier2::env_var_eq;
use aios_verification::{LocalProbe, MockLocalProbe};
use serde_json::json;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

#[test]
fn json_field_eq_passes_for_nested_path() {
    let actual = json!({"root": {"state": "ready"}});
    let verdict = json_field_eq(
        &json!({"path": ["root", "state"], "expected": "ready"}),
        &actual,
    );

    assert!(verdict.passed);
    assert_eq!(verdict.actual, json!("ready"));
    assert_eq!(verdict.error, None);
}

#[test]
fn json_field_eq_fails_for_mismatch() {
    let actual = json!({"root": {"state": "stopped"}});
    let verdict = json_field_eq(
        &json!({"path": ["root", "state"], "expected": "ready"}),
        &actual,
    );

    assert!(!verdict.passed);
    assert_eq!(verdict.actual, json!("stopped"));
    assert_eq!(verdict.error, None);
}

#[test]
fn blake3_matches_passes_for_inline_bytes() {
    let expected_hash = blake3::hash(b"aios").to_hex().to_string();
    let verdict = blake3_matches(&json!({
        "bytes": "aios",
        "expected_hash_hex": expected_hash,
    }));

    assert!(verdict.passed);
    assert_eq!(verdict.actual, json!({"observed_hash": expected_hash}));
    assert_eq!(verdict.error, None);
}

#[test]
fn blake3_matches_fails_for_wrong_hash() {
    let verdict = blake3_matches(&json!({
        "bytes": "aios",
        "expected_hash_hex": "0000000000000000000000000000000000000000000000000000000000000000",
    }));

    assert!(!verdict.passed);
    assert_eq!(verdict.error, None);
}

#[test]
fn regex_matches_passes_when_text_matches_pattern() {
    let verdict = regex_matches(&json!({
        "text": "service nginx active",
        "pattern": "nginx\\s+active",
    }));

    assert!(verdict.passed);
    assert_eq!(verdict.actual, json!({"matched": true}));
    assert_eq!(verdict.error, None);
}

#[test]
fn regex_matches_returns_probe_error_for_invalid_pattern() {
    let verdict = regex_matches(&json!({
        "text": "service nginx active",
        "pattern": "[",
    }));

    assert!(!verdict.passed);
    assert!(verdict
        .error
        .as_deref()
        .is_some_and(|err| err.contains("regex")));
}

#[tokio::test]
async fn env_var_eq_passes_with_mock_probe() -> TestResult {
    let probe: Arc<dyn LocalProbe> =
        Arc::new(MockLocalProbe::default().with_env_var("AIOS_MODE", "test"));

    let verdict = env_var_eq(
        probe.as_ref(),
        &json!({"name": "AIOS_MODE", "expected": "test"}),
    )
    .await;

    assert!(verdict.passed);
    assert_eq!(verdict.actual, json!({"value": "test"}));
    assert_eq!(verdict.error, None);
    Ok(())
}

#[tokio::test]
async fn env_var_eq_fails_with_mock_probe_mismatch() -> TestResult {
    let probe: Arc<dyn LocalProbe> =
        Arc::new(MockLocalProbe::default().with_env_var("AIOS_MODE", "prod"));

    let verdict = env_var_eq(
        probe.as_ref(),
        &json!({"name": "AIOS_MODE", "expected": "test"}),
    )
    .await;

    assert!(!verdict.passed);
    assert_eq!(verdict.actual, json!({"value": "prod"}));
    assert_eq!(verdict.error, None);
    Ok(())
}

#[test]
fn t064_vocabulary_does_not_contain_json_field_eq_as_top_level_primitive() {
    let _action_id = ActionId::new();
    assert!(
        serde_json::from_value::<aios_verification::VerificationPrimitive>(json!("JSON_FIELD_EQ"))
            .is_err()
    );
}
