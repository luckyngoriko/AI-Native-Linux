//! Latency tiering classifier per S1.2 — stub heuristic implementation.
//!
//! Stub heuristic; replaced with model-based estimate in M12+.
//! The classifier uses intent characteristic proxies (word count, estimated tokens,
//! multi-action indicators) to pick a [`LatencyTier`]. Real catalog-lookup, cache-hit,
//! and model-based prediction are deferred to future milestones.

use crate::intent::CognitiveIntent;
use crate::latency::{LatencyTier, PrivacyClass};

/// Classifies cognitive intents into latency tiers per S1.2 routing rules.
///
/// Stub heuristic — uses word count, estimated token count, and multi-action
/// keyword detection as proxies for the real model-based estimate planned for M12+.
/// Recovery-mode and privacy-class guard rules from S1.2 §7.1 are fully enforced.
#[derive(Debug, Clone, Default)]
pub struct LatencyClassifier {
    // Reserved for M12+ configuration: model-based threshold tuning, catalog pointer, etc.
}

impl LatencyClassifier {
    /// Create a new classifier with default heuristic thresholds.
    #[must_use]
    pub const fn new_with_defaults() -> Self {
        Self {}
    }

    /// Classify an intent into a latency tier.
    ///
    /// Applies S1.2 §7.1 guard rules in priority order:
    ///
    /// 1. Recovery mode → cap at T1 (no T2+ model-dependent paths)
    /// 2. CLASSIFIED privacy → cap at T2
    /// 3. `SECRET_BEARING` privacy → cap at T3 (no T4 external)
    ///
    /// Within the allowed set, a stub heuristic picks the lowest tier
    /// that plausibly matches the intent's estimated complexity.
    #[must_use]
    pub fn classify(
        &self,
        intent: &CognitiveIntent,
        privacy_class: &str,
        recovery_mode: bool,
    ) -> LatencyTier {
        let derived = Self::stub_derive_tier(intent);
        let pc = parse_privacy_class(privacy_class);

        // Guard 1: recovery mode caps at T1 (§7.1)
        let tier = if recovery_mode {
            min_tier(derived, LatencyTier::T1Deterministic)
        } else {
            derived
        };

        // Guard 2: CLASSIFIED caps at T2 (§7.1)
        let tier = if pc == PrivacyClass::Classified {
            min_tier(tier, LatencyTier::T2CatalogRetrieval)
        } else {
            tier
        };

        // Guard 3: SECRET_BEARING caps at T3 — no T4 external (§5.1)
        if pc == PrivacyClass::SecretBearing {
            min_tier(tier, LatencyTier::T3LocalCognitive)
        } else {
            tier
        }
    }

    /// Stub heuristic: derive tier from intent text characteristics.
    ///
    /// Uses word count, estimated token count (chars / 4), and multi-action
    /// keyword detection as proxies for semantic complexity. Replaced with
    /// model-based estimate in M12+.
    fn stub_derive_tier(intent: &CognitiveIntent) -> LatencyTier {
        let text = &intent.natural_language;
        let word_count = text.split_whitespace().count();
        let estimated_tokens = text.len() / 4;
        let is_multi = stub_is_multi_action(text);

        if is_multi || estimated_tokens > 500 {
            LatencyTier::T4PowerfulReasoning
        } else if estimated_tokens > 100 {
            LatencyTier::T3LocalCognitive
        } else if estimated_tokens > 20 {
            LatencyTier::T2CatalogRetrieval
        } else if word_count > 1 {
            LatencyTier::T1Deterministic
        } else {
            LatencyTier::T0CachedUiState
        }
    }
}

/// Return the more restrictive (lower-numbered) of two tiers.
const fn min_tier(a: LatencyTier, b: LatencyTier) -> LatencyTier {
    if (a as u8) <= (b as u8) {
        a
    } else {
        b
    }
}

/// Parse a privacy class string into the closed enum.
///
/// Defaults to [`PrivacyClass::Sensitive`] per S1.2 §5.2 conservative default.
fn parse_privacy_class(s: &str) -> PrivacyClass {
    match s {
        "PUBLIC" => PrivacyClass::Public,
        "INTERNAL" => PrivacyClass::Internal,
        "SECRET_BEARING" => PrivacyClass::SecretBearing,
        "CLASSIFIED" => PrivacyClass::Classified,
        _ => PrivacyClass::Sensitive,
    }
}

/// Stub multi-action detection via keyword heuristics.
///
/// Checks for sequential connectors and complex task verbs as rough
/// indicators that the intent requires multi-step planning (T4).
fn stub_is_multi_action(text: &str) -> bool {
    let lower = text.to_lowercase();
    let word_count = lower.split_whitespace().count();
    if word_count > 100 {
        return true;
    }
    let connectors = [" and ", " then ", " after ", " first ", " next "];
    let complex_verbs = ["setup", "prepare", "deploy", "configure", "orchestrate"];
    connectors.iter().any(|c| lower.contains(c)) || complex_verbs.iter().any(|v| lower.contains(v))
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::intent::{CognitiveIntent, IntentId, SubjectRef};
    use chrono::Utc;

    fn make_intent(text: &str) -> CognitiveIntent {
        CognitiveIntent {
            intent_id: IntentId::new(),
            subject: SubjectRef("test-subject".into()),
            natural_language: text.to_string(),
            context_hash: String::new(),
            created_at: Utc::now(),
            latency_class: LatencyTier::T1Deterministic,
            privacy_class: PrivacyClass::Public,
        }
    }

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

    #[test]
    fn recovery_mode_caps_at_t1() {
        let c = LatencyClassifier::new_with_defaults();
        let words: Vec<String> = (0..200).map(|i| format!("word{i}")).collect();
        let intent = make_intent(&words.join(" "));
        // Without recovery this would be T4
        let tier_normal = c.classify(&intent, "PUBLIC", false);
        assert_eq!(tier_normal, LatencyTier::T4PowerfulReasoning);
        // With recovery it must be T1 or lower
        let tier_recovery = c.classify(&intent, "PUBLIC", true);
        assert_eq!(tier_recovery, LatencyTier::T1Deterministic);
    }

    #[test]
    fn secret_bearing_caps_at_t3() {
        let c = LatencyClassifier::new_with_defaults();
        let words: Vec<String> = (0..200).map(|i| format!("word{i}")).collect();
        let intent = make_intent(&words.join(" "));
        let tier = c.classify(&intent, "SECRET_BEARING", false);
        // T4 not allowed for SECRET_BEARING; capped at T3
        assert_eq!(tier, LatencyTier::T3LocalCognitive);
    }

    #[test]
    fn secret_bearing_plus_recovery_caps_at_t1() {
        let c = LatencyClassifier::new_with_defaults();
        let words: Vec<String> = (0..200).map(|i| format!("word{i}")).collect();
        let intent = make_intent(&words.join(" "));
        let tier = c.classify(&intent, "SECRET_BEARING", true);
        // Recovery caps at T1, SECRET_BEARING caps at T3 → effective cap T1
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
            ("status", "PUBLIC", false),              // T0
            ("restart nginx", "PUBLIC", false),       // T1
            // 50 words → T2
            ("word0 word1 word2 word3 word4 word5 word6 word7 word8 word9 word10 word11 word12 word13 word14 word15 word16 word17 word18 word19 word20 word21 word22 word23 word24 word25 word26 word27 word28 word29 word30 word31 word32 word33 word34 word35 word36 word37 word38 word39 word40 word41 word42 word43 word44 word45 word46 word47 word48 word49", "PUBLIC", false),
            // 50 longer words → ~137 tokens → T3
            ("abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij abcdefghij", "PUBLIC", false),
            ("setup and deploy the entire stack", "PUBLIC", false), // T4 (multi)
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
        let c = std::sync::Arc::new(LatencyClassifier::new_with_defaults());
        let intent = std::sync::Arc::new(make_intent("restart the nginx web server"));

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

        // All 3 should return the same tier
        assert!(results.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn unknown_privacy_class_defaults_to_sensitive() {
        let c = LatencyClassifier::new_with_defaults();
        let intent = make_intent("restart nginx");
        // Unknown class → Sensitive → T4 requires policy+approval but T1 is fine
        let tier = c.classify(&intent, "UNKNOWN_CLASS", false);
        assert_eq!(tier, LatencyTier::T1Deterministic);
    }
}
