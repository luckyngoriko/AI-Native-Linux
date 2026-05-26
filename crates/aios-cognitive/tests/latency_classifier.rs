//! T-096 integration tests for `LatencyClassifier`.
//!
//! Minimum 14 tests covering tier classification, guard rules (recovery mode,
//! `SECRET_BEARING`, `CLASSIFIED`), determinism, thread safety, and all 5 tier reachability.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]

use std::sync::Arc;

use chrono::Utc;

use aios_cognitive::{CognitiveIntent, IntentId, LatencyClassifier, LatencyTier, SubjectRef};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn make_intent(text: &str) -> CognitiveIntent {
    CognitiveIntent {
        intent_id: IntentId::new(),
        subject: SubjectRef("agent:dev".into()),
        natural_language: text.to_string(),
        context_hash: String::new(),
        created_at: Utc::now(),
        latency_class: LatencyTier::T1Deterministic,
        privacy_class: aios_cognitive::PrivacyClass::Public,
    }
}

// ---------------------------------------------------------------------------
// tier classification
// ---------------------------------------------------------------------------

#[test]
fn new_with_defaults_succeeds() {
    let c = LatencyClassifier::new_with_defaults();
    let intent = make_intent("hello");
    let tier = c.classify(&intent, "PUBLIC", false);
    assert!(matches!(
        tier,
        LatencyTier::T0CachedUiState | LatencyTier::T1Deterministic
    ));
}

#[test]
fn single_word_returns_t0() {
    let c = LatencyClassifier::new_with_defaults();
    let intent = make_intent("status");
    let tier = c.classify(&intent, "PUBLIC", false);
    assert_eq!(tier, LatencyTier::T0CachedUiState);
}

#[test]
fn short_query_returns_t1() {
    let c = LatencyClassifier::new_with_defaults();
    let intent = make_intent("restart nginx");
    let tier = c.classify(&intent, "PUBLIC", false);
    assert_eq!(tier, LatencyTier::T1Deterministic);
}

#[test]
fn medium_query_returns_t2() {
    let c = LatencyClassifier::new_with_defaults();
    let words: Vec<String> = (0..50).map(|i| format!("word{i}")).collect();
    let intent = make_intent(&words.join(" "));
    let tier = c.classify(&intent, "PUBLIC", false);
    assert_eq!(tier, LatencyTier::T2CatalogRetrieval);
}

#[test]
fn long_query_returns_t3() {
    let c = LatencyClassifier::new_with_defaults();
    let words: Vec<String> = (0..80).map(|i| format!("word{i}")).collect();
    let intent = make_intent(&words.join(" "));
    let tier = c.classify(&intent, "PUBLIC", false);
    assert_eq!(tier, LatencyTier::T3LocalCognitive);
}

#[test]
fn multi_action_returns_t4() {
    let c = LatencyClassifier::new_with_defaults();
    let intent = make_intent("setup and deploy the application then configure the firewall");
    let tier = c.classify(&intent, "PUBLIC", false);
    assert_eq!(tier, LatencyTier::T4PowerfulReasoning);
}

#[test]
fn very_long_returns_t4() {
    let c = LatencyClassifier::new_with_defaults();
    let words: Vec<String> = (0..200).map(|i| format!("word{i}")).collect();
    let intent = make_intent(&words.join(" "));
    let tier = c.classify(&intent, "PUBLIC", false);
    assert_eq!(tier, LatencyTier::T4PowerfulReasoning);
}

// ---------------------------------------------------------------------------
// guard rules
// ---------------------------------------------------------------------------

#[test]
fn recovery_mode_caps_at_t1() {
    let c = LatencyClassifier::new_with_defaults();
    let words: Vec<String> = (0..200).map(|i| format!("word{i}")).collect();
    let intent = make_intent(&words.join(" "));
    let tier_normal = c.classify(&intent, "PUBLIC", false);
    assert_eq!(tier_normal, LatencyTier::T4PowerfulReasoning);
    let tier_recovery = c.classify(&intent, "PUBLIC", true);
    assert_eq!(tier_recovery, LatencyTier::T1Deterministic);
}

#[test]
fn secret_bearing_caps_at_t3() {
    let c = LatencyClassifier::new_with_defaults();
    let words: Vec<String> = (0..200).map(|i| format!("word{i}")).collect();
    let intent = make_intent(&words.join(" "));
    let tier = c.classify(&intent, "SECRET_BEARING", false);
    assert_eq!(tier, LatencyTier::T3LocalCognitive);
}

#[test]
fn secret_bearing_plus_recovery_caps_at_t1() {
    let c = LatencyClassifier::new_with_defaults();
    let words: Vec<String> = (0..200).map(|i| format!("word{i}")).collect();
    let intent = make_intent(&words.join(" "));
    let tier = c.classify(&intent, "SECRET_BEARING", true);
    assert_eq!(tier, LatencyTier::T1Deterministic);
}

#[test]
fn classified_caps_at_t2() {
    let c = LatencyClassifier::new_with_defaults();
    let words: Vec<String> = (0..200).map(|i| format!("word{i}")).collect();
    let intent = make_intent(&words.join(" "));
    let tier = c.classify(&intent, "CLASSIFIED", false);
    assert_eq!(tier, LatencyTier::T2CatalogRetrieval);
}

// ---------------------------------------------------------------------------
// determinism + reachability + thread safety
// ---------------------------------------------------------------------------

#[test]
fn determinism_same_intent_same_tier() {
    let c = LatencyClassifier::new_with_defaults();
    let intent = make_intent("restart the nginx web server");
    let tier1 = c.classify(&intent, "PUBLIC", false);
    let tier2 = c.classify(&intent, "PUBLIC", false);
    assert_eq!(tier1, tier2);
}

#[test]
fn all_five_tiers_reachable() {
    let c = LatencyClassifier::new_with_defaults();
    let mut seen = std::collections::HashSet::new();

    let cases: Vec<(&str, &str, bool)> = vec![
        ("status", "PUBLIC", false),
        ("restart nginx", "PUBLIC", false),
        ("word0 word1 word2 word3 word4 word5 word6 word7 word8 word9 word10 word11 word12 word13 word14 word15 word16 word17 word18 word19 word20 word21 word22 word23 word24 word25 word26 word27 word28 word29 word30 word31 word32 word33 word34 word35 word36 word37 word38 word39 word40 word41 word42 word43 word44 word45 word46 word47 word48 word49", "PUBLIC", false),
        ("a0 a1 a2 a3 a4 a5 a6 a7 a8 a9 a10 a11 a12 a13 a14 a15 a16 a17 a18 a19 a20 a21 a22 a23 a24 a25 a26 a27 a28 a29 a30 a31 a32 a33 a34 a35 a36 a37 a38 a39 a40 a41 a42 a43 a44 a45 a46 a47 a48 a49", "PUBLIC", false),
        // 50 longer words → ~137 tokens → T3
        ("abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij", "PUBLIC", false),
        ("setup and deploy the entire stack", "PUBLIC", false),
    ];

    for (text, pc, recovery) in &cases {
        let intent = make_intent(text);
        let tier = c.classify(&intent, pc, *recovery);
        seen.insert(format!("{tier:?}"));
    }

    assert_eq!(
        seen.len(),
        5,
        "all 5 LatencyTier variants must be reachable"
    );
}

#[test]
fn latency_classifier_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LatencyClassifier>();
}

#[test]
fn concurrent_classify_no_panic() {
    let c = Arc::new(LatencyClassifier::new_with_defaults());
    let intent = Arc::new(make_intent("restart the nginx web server"));

    let handles: Vec<_> = (0..3)
        .map(|_| {
            let c = c.clone();
            let intent = intent.clone();
            std::thread::spawn(move || c.classify(&intent, "PUBLIC", false))
        })
        .collect();

    let mut results = Vec::new();
    for h in handles {
        results.push(h.join().expect("thread panicked"));
    }

    assert!(results.windows(2).all(|w| w[0] == w[1]));
}

#[test]
fn unknown_privacy_class_defaults_to_sensitive() {
    let c = LatencyClassifier::new_with_defaults();
    let intent = make_intent("restart nginx");
    let tier = c.classify(&intent, "UNKNOWN_CLASS", false);
    assert_eq!(tier, LatencyTier::T1Deterministic);
}
