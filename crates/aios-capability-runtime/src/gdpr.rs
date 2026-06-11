//! GDPR crypto-shred module for AIOS data sovereignty (S16.9).
#![allow(
    clippy::doc_markdown,
    clippy::missing_const_for_fn,
    clippy::module_name_repetitions
)]

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataClassification { Public, Internal, Confidential, Restricted }

impl DataClassification {
    pub fn requires_encryption(&self) -> bool {
        matches!(self, Self::Confidential | Self::Restricted)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Public => "Public",
            Self::Internal => "Internal",
            Self::Confidential => "Confidential",
            Self::Restricted => "Restricted",
        }
    }

    pub fn max_level(a: Self, b: Self) -> Self {
        use DataClassification::*;
        let to_ord = |c: Self| -> u8 { match c { Public => 0, Internal => 1, Confidential => 2, Restricted => 3 } };
        match to_ord(a).max(to_ord(b)) {
            0 => Public, 1 => Internal, 2 => Confidential, _ => Restricted,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CryptoShredKey {
    pub key_id: String,
    pub algorithm: String,
    pub created_at: u64,
}

impl CryptoShredKey {
    pub fn new(key_id: String, algorithm: String) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self { key_id, algorithm, created_at }
    }

    pub fn new_with_time(key_id: String, algorithm: String, created_at: u64) -> Self {
        Self { key_id, algorithm, created_at }
    }

    pub fn destroy(&self, request: &ShredRequest) -> ShredEvidence {
        let verification = format!(
            "KEY_DESTROYED:{}:{}:{}",
            self.key_id, request.destroyed_at, request.data_id
        );
        ShredEvidence {
            data_id: request.data_id.clone(),
            key_id: self.key_id.clone(),
            reason: request.reason.clone(),
            destroyed_at: request.destroyed_at,
            verification,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShredRequest {
    pub data_id: String,
    pub reason: String,
    pub created_at: u64,
    pub destroyed_at: u64,
}

impl ShredRequest {
    pub fn new(data_id: String, reason: String) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self { data_id, reason, created_at: now, destroyed_at: 0 }
    }

    pub fn new_full(data_id: String, reason: String, created_at: u64, destroyed_at: u64) -> Self {
        Self { data_id, reason, created_at, destroyed_at }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.data_id.is_empty() {
            return Err("data_id must not be empty".into());
        }
        if self.reason.is_empty() {
            return Err("reason must not be empty".into());
        }
        if self.destroyed_at > 0 && self.destroyed_at < self.created_at {
            return Err("destroyed_at must be >= created_at".into());
        }
        Ok(())
    }

    pub fn mark_destroyed(&mut self, timestamp: u64) -> Result<(), String> {
        if timestamp < self.created_at {
            return Err("destroy timestamp must be >= created_at".into());
        }
        self.destroyed_at = timestamp;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShredEvidence {
    pub data_id: String,
    pub key_id: String,
    pub reason: String,
    pub destroyed_at: u64,
    pub verification: String,
}

impl ShredEvidence {
    pub fn is_valid(&self) -> bool {
        !self.verification.is_empty()
            && self.verification.contains("KEY_DESTROYED")
            && self.verification.contains(&self.key_id)
            && self.verification.contains(&self.data_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub timestamp: u64,
    pub action: String,
    pub data_id: String,
    pub evidence_id: String,
}

impl AuditEntry {
    pub fn new(timestamp: u64, action: String, data_id: String, evidence_id: String) -> Self {
        Self { timestamp, action, data_id, evidence_id }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditTrail {
    pub entries: Vec<AuditEntry>,
}

impl AuditTrail {
    pub fn new() -> Self { Self { entries: Vec::new() } }

    pub fn record(&mut self, entry: AuditEntry) { self.entries.push(entry); }

    pub fn query_by_data_id(&self, data_id: &str) -> Vec<&AuditEntry> {
        self.entries.iter().filter(|e| e.data_id == data_id).collect()
    }

    pub fn len(&self) -> usize { self.entries.len() }

    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

impl Default for AuditTrail {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportBundle {
    pub subject_id: String,
    pub generated_at: u64,
    pub entries: Vec<AuditEntry>,
    pub signature: Option<Vec<u8>>,
}

impl ExportBundle {
    pub fn new(subject_id: String, generated_at: u64) -> Self {
        Self { subject_id, generated_at, entries: Vec::new(), signature: None }
    }

    pub fn add_entry(&mut self, entry: AuditEntry) { self.entries.push(entry); }

    pub fn sign(&mut self, sig: Vec<u8>) { self.signature = Some(sig); }

    pub fn entry_count(&self) -> usize { self.entries.len() }
}

// ── Data Governance Types (R3-W1.8) ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataCategory {
    Personal,
    Sensitive,
    Anonymous,
    System,
    Financial,
    Health,
}

impl DataCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Personal => "Personal",
            Self::Sensitive => "Sensitive",
            Self::Anonymous => "Anonymous",
            Self::System => "System",
            Self::Financial => "Financial",
            Self::Health => "Health",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetentionPolicy {
    pub duration_seconds: u64,
    pub auto_delete: bool,
}

impl RetentionPolicy {
    pub fn new(duration_seconds: u64, auto_delete: bool) -> Self {
        Self { duration_seconds, auto_delete }
    }

    pub fn is_expired(&self, stored_at: u64, now: u64) -> bool {
        now.saturating_sub(stored_at) >= self.duration_seconds
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataSubject {
    pub user_id: String,
    pub categories: Vec<DataCategory>,
    pub shred_key_id: String,
    pub registered_at: u64,
    pub shredded: bool,
}

impl DataSubject {
    pub fn new(
        user_id: String,
        categories: Vec<DataCategory>,
        shred_key_id: String,
        registered_at: u64,
    ) -> Self {
        Self { user_id, categories, shred_key_id, registered_at, shredded: false }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShredResult {
    Shredded,
    Partial(usize),
    NotFound,
}

impl ShredResult {
    pub fn is_shredded(&self) -> bool {
        matches!(self, Self::Shredded)
    }
}

#[derive(Debug, Clone)]
pub struct DataGovernanceRegistry {
    subjects: HashMap<String, DataSubject>,
    policies: HashMap<String, RetentionPolicy>,
}

impl DataGovernanceRegistry {
    pub fn new() -> Self {
        Self { subjects: HashMap::new(), policies: HashMap::new() }
    }

    pub fn register_subject(
        &mut self,
        user_id: String,
        shred_key_id: String,
        categories: Vec<DataCategory>,
        retention: RetentionPolicy,
    ) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let subject = DataSubject::new(user_id.clone(), categories, shred_key_id, now);
        self.subjects.insert(user_id.clone(), subject);
        self.policies.insert(user_id, retention);
    }

    pub fn register_subject_at(
        &mut self,
        user_id: String,
        shred_key_id: String,
        categories: Vec<DataCategory>,
        retention: RetentionPolicy,
        registered_at: u64,
    ) {
        let subject = DataSubject::new(user_id.clone(), categories, shred_key_id, registered_at);
        self.subjects.insert(user_id.clone(), subject);
        self.policies.insert(user_id, retention);
    }

    pub fn execute_shred(&mut self, user_id: &str) -> ShredResult {
        let subject = match self.subjects.get_mut(user_id) {
            Some(s) => s,
            None => return ShredResult::NotFound,
        };
        if subject.shredded {
            return ShredResult::Shredded;
        }
        let remaining = subject.categories.len();
        subject.shredded = true;
        subject.shred_key_id.clear();
        if remaining == 0 {
            ShredResult::Partial(0)
        } else {
            ShredResult::Shredded
        }
    }

    pub fn check_retention(&self, user_id: &str) -> Vec<DataCategory> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.check_retention_at(user_id, now)
    }

    pub fn check_retention_at(&self, user_id: &str, now: u64) -> Vec<DataCategory> {
        let subject = match self.subjects.get(user_id) {
            Some(s) => s,
            None => return Vec::new(),
        };
        let policy = match self.policies.get(user_id) {
            Some(p) => p,
            None => return Vec::new(),
        };
        if policy.is_expired(subject.registered_at, now) {
            subject.categories.clone()
        } else {
            Vec::new()
        }
    }

    pub fn subject_count(&self) -> usize {
        self.subjects.len()
    }

    pub fn get_subject(&self, user_id: &str) -> Option<&DataSubject> {
        self.subjects.get(user_id)
    }

    pub fn get_policy(&self, user_id: &str) -> Option<&RetentionPolicy> {
        self.policies.get(user_id)
    }
}

impl Default for DataGovernanceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Existing classification / shred tests ────────────────────────────────

    #[test]
    fn classification_levels_encryption_requirement() {
        assert!(!DataClassification::Public.requires_encryption());
        assert!(!DataClassification::Internal.requires_encryption());
        assert!(DataClassification::Confidential.requires_encryption());
        assert!(DataClassification::Restricted.requires_encryption());
    }

    #[test]
    fn classification_max_level_derivation() {
        assert_eq!(
            DataClassification::max_level(DataClassification::Public, DataClassification::Internal),
            DataClassification::Internal
        );
        assert_eq!(
            DataClassification::max_level(DataClassification::Confidential, DataClassification::Restricted),
            DataClassification::Restricted
        );
        assert_eq!(
            DataClassification::max_level(DataClassification::Public, DataClassification::Public),
            DataClassification::Public
        );
    }

    #[test]
    fn classification_labels() {
        assert_eq!(DataClassification::Public.label(), "Public");
        assert_eq!(DataClassification::Internal.label(), "Internal");
        assert_eq!(DataClassification::Confidential.label(), "Confidential");
        assert_eq!(DataClassification::Restricted.label(), "Restricted");
    }

    #[test]
    fn key_lifecycle_create_and_destroy() {
        let key = CryptoShredKey::new_with_time("k-001".into(), "AES-256-GCM".into(), 1000);
        assert_eq!(key.key_id, "k-001");
        assert_eq!(key.algorithm, "AES-256-GCM");
        assert_eq!(key.created_at, 1000);

        let request = ShredRequest::new_full("data-42".into(), "RTBF-erasure".into(), 1000, 2000);
        let evidence = key.destroy(&request);
        assert_eq!(evidence.data_id, "data-42");
        assert_eq!(evidence.key_id, "k-001");
        assert_eq!(evidence.reason, "RTBF-erasure");
        assert_eq!(evidence.destroyed_at, 2000);
        assert!(evidence.is_valid());
    }

    #[test]
    fn shred_evidence_creation_and_validation() {
        let key = CryptoShredKey::new_with_time("shred-1".into(), "ChaCha20-Poly1305".into(), 500);
        let req = ShredRequest::new_full("user-99".into(), "GDPR Article 17".into(), 500, 1500);
        let evidence = key.destroy(&req);

        assert!(evidence.verification.contains("KEY_DESTROYED"));
        assert!(evidence.verification.contains("shred-1"));
        assert!(evidence.verification.contains("user-99"));
        assert!(evidence.is_valid());
    }

    #[test]
    fn shred_request_validation() {
        let valid = ShredRequest::new_full("data-1".into(), "deletion".into(), 100, 200);
        assert!(valid.validate().is_ok());

        let empty_id = ShredRequest::new_full("".into(), "reason".into(), 100, 200);
        assert!(empty_id.validate().is_err());

        let empty_reason = ShredRequest::new_full("data-2".into(), "".into(), 100, 200);
        assert!(empty_reason.validate().is_err());

        let bad_time = ShredRequest::new_full("data-3".into(), "test".into(), 300, 100);
        assert!(bad_time.validate().is_err());
    }

    #[test]
    fn shred_request_mark_destroyed() {
        let mut req = ShredRequest::new_full("data-x".into(), "compliance".into(), 1000, 0);
        assert_eq!(req.destroyed_at, 0);

        assert!(req.mark_destroyed(2000).is_ok());
        assert_eq!(req.destroyed_at, 2000);

        let err = req.mark_destroyed(500);
        assert!(err.is_err());
    }

    #[test]
    fn audit_trail_recording_and_query() {
        let mut trail = AuditTrail::new();
        assert!(trail.is_empty());

        trail.record(AuditEntry::new(1000, "SHRED".into(), "data-a".into(), "ev-1".into()));
        trail.record(AuditEntry::new(2000, "EXPORT".into(), "data-a".into(), "ev-2".into()));
        trail.record(AuditEntry::new(3000, "SHRED".into(), "data-b".into(), "ev-3".into()));

        assert_eq!(trail.len(), 3);
        assert_eq!(trail.query_by_data_id("data-a").len(), 2);
        assert_eq!(trail.query_by_data_id("data-b").len(), 1);
        assert!(trail.query_by_data_id("nonexistent").is_empty());
    }

    #[test]
    fn export_bundle_lifecycle() {
        let mut bundle = ExportBundle::new("subject-1".into(), 5000);
        bundle.add_entry(AuditEntry::new(1000, "SHRED".into(), "data-z".into(), "ev-z".into()));
        bundle.add_entry(AuditEntry::new(2000, "CLASSIFY".into(), "data-y".into(), "ev-y".into()));

        assert_eq!(bundle.entry_count(), 2);
        assert!(bundle.signature.is_none());

        bundle.sign(b"sig-bytes".to_vec());
        assert_eq!(bundle.signature, Some(b"sig-bytes".to_vec()));
    }

    #[test]
    fn full_rtbf_erasure_flow() {
        let key = CryptoShredKey::new_with_time("k-rtbf".into(), "AES-256-GCM".into(), 100);
        let mut req = ShredRequest::new_full("pii-001".into(), "RTBF Article 17".into(), 100, 0);
        assert!(req.validate().is_ok());

        assert!(req.mark_destroyed(500).is_ok());
        let evidence = key.destroy(&req);
        assert!(evidence.is_valid());

        let mut trail = AuditTrail::new();
        trail.record(AuditEntry::new(
            evidence.destroyed_at,
            "CRYPTO_SHRED".into(),
            evidence.data_id.clone(),
            evidence.verification.clone(),
        ));
        assert_eq!(trail.len(), 1);

        let mut bundle = ExportBundle::new("subject-rtbf".into(), 600);
        bundle.add_entry(trail.entries[0].clone());
        bundle.sign(b"audit-sig".to_vec());
        assert_eq!(bundle.entry_count(), 1);
        assert!(bundle.signature.is_some());
    }

    // ── Data Governance tests (R3-W1.8) ─────────────────────────────────────

    #[test]
    fn data_category_labels() {
        assert_eq!(DataCategory::Personal.label(), "Personal");
        assert_eq!(DataCategory::Sensitive.label(), "Sensitive");
        assert_eq!(DataCategory::Anonymous.label(), "Anonymous");
        assert_eq!(DataCategory::System.label(), "System");
        assert_eq!(DataCategory::Financial.label(), "Financial");
        assert_eq!(DataCategory::Health.label(), "Health");
    }

    #[test]
    fn retention_policy_expiry() {
        let policy = RetentionPolicy::new(3600, true);
        assert!(policy.is_expired(0, 3600));
        assert!(!policy.is_expired(1000, 4000));
        assert!(!policy.is_expired(1000, 4599));
        assert!(policy.is_expired(1000, 4600));
    }

    #[test]
    fn shred_makes_data_unavailable() {
        let mut registry = DataGovernanceRegistry::new();
        registry.register_subject_at(
            "user-a".into(),
            "key-a".into(),
            vec![DataCategory::Personal, DataCategory::Financial],
            RetentionPolicy::new(3600, true),
            1000,
        );

        assert_eq!(registry.subject_count(), 1);
        let subject = registry.get_subject("user-a").unwrap_or_else(|| panic!("absent"));
        assert!(!subject.shredded);
        assert_eq!(subject.shred_key_id, "key-a");

        let result = registry.execute_shred("user-a");
        assert_eq!(result, ShredResult::Shredded);

        let subject = registry.get_subject("user-a").unwrap_or_else(|| panic!("absent"));
        assert!(subject.shredded);
        assert!(subject.shred_key_id.is_empty());
    }

    #[test]
    fn retention_check_triggers_for_expired_data() {
        let mut registry = DataGovernanceRegistry::new();
        registry.register_subject_at(
            "user-b".into(),
            "key-b".into(),
            vec![DataCategory::Health, DataCategory::Sensitive],
            RetentionPolicy::new(7200, false),
            0,
        );

        let expired = registry.check_retention_at("user-b", 7200);
        assert_eq!(expired.len(), 2);
        assert!(expired.contains(&DataCategory::Health));
        assert!(expired.contains(&DataCategory::Sensitive));

        let not_expired = registry.check_retention_at("user-b", 1000);
        assert!(not_expired.is_empty());
    }

    #[test]
    fn multiple_subjects_independent_shred() {
        let mut registry = DataGovernanceRegistry::new();
        registry.register_subject_at(
            "alice".into(),
            "k-alice".into(),
            vec![DataCategory::Personal],
            RetentionPolicy::new(86400, true),
            100,
        );
        registry.register_subject_at(
            "bob".into(),
            "k-bob".into(),
            vec![DataCategory::System, DataCategory::Financial],
            RetentionPolicy::new(43200, false),
            200,
        );

        assert_eq!(registry.subject_count(), 2);

        let result = registry.execute_shred("alice");
        assert_eq!(result, ShredResult::Shredded);

        let alice = registry.get_subject("alice").unwrap_or_else(|| panic!("absent"));
        let bob = registry.get_subject("bob").unwrap_or_else(|| panic!("absent"));
        assert!(alice.shredded);
        assert!(!bob.shredded);
        assert_eq!(bob.shred_key_id, "k-bob");
    }

    #[test]
    fn shred_not_found() {
        let mut registry = DataGovernanceRegistry::new();
        let result = registry.execute_shred("ghost");
        assert_eq!(result, ShredResult::NotFound);
    }

    #[test]
    fn double_shred_is_idempotent() {
        let mut registry = DataGovernanceRegistry::new();
        registry.register_subject_at(
            "user-c".into(),
            "key-c".into(),
            vec![DataCategory::Anonymous],
            RetentionPolicy::new(1800, true),
            500,
        );

        assert_eq!(registry.execute_shred("user-c"), ShredResult::Shredded);
        assert_eq!(registry.execute_shred("user-c"), ShredResult::Shredded);
    }

    #[test]
    fn check_retention_unknown_user_returns_empty() {
        let registry = DataGovernanceRegistry::new();
        let result = registry.check_retention_at("nobody", 9999);
        assert!(result.is_empty());
    }

    #[test]
    fn shred_result_is_shredded_helper() {
        assert!(ShredResult::Shredded.is_shredded());
        assert!(!ShredResult::Partial(3).is_shredded());
        assert!(!ShredResult::NotFound.is_shredded());
    }

    #[test]
    fn retention_before_deadline_returns_empty() {
        let mut registry = DataGovernanceRegistry::new();
        registry.register_subject_at(
            "user-d".into(),
            "key-d".into(),
            vec![DataCategory::Personal, DataCategory::System],
            RetentionPolicy::new(10000, true),
            100,
        );

        let result = registry.check_retention_at("user-d", 5000);
        assert!(result.is_empty());
    }
}
