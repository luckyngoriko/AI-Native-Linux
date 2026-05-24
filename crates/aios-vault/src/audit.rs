//! In-memory capability lifecycle audit log for S5.2.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S5.2 vault broker vocabulary"
)]

use std::collections::HashMap;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use chrono::{DateTime, Utc};

use crate::capability::CapabilityId;
use crate::identity::SubjectRef;

/// Compact audit projection for one capability's lifecycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityAuditEntry {
    /// Capability being audited.
    pub capability_id: CapabilityId,
    /// Number of successful uses recorded by the broker.
    pub use_count: u64,
    /// Last successful use timestamp.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Last successful operation kind.
    pub last_used_op_kind: Option<String>,
    /// Subject recorded by the issuer path for this T-051 slice.
    pub issued_by: SubjectRef,
    /// Subject that revoked the capability, if revoked.
    pub revoked_by: Option<SubjectRef>,
    /// Revocation timestamp, if revoked.
    pub revoked_at: Option<DateTime<Utc>>,
    /// Expiration timestamp, if expired.
    pub expired_at: Option<DateTime<Utc>>,
}

/// Thread-safe in-memory audit log keyed by capability id.
#[derive(Debug, Default)]
pub struct CapabilityAuditLog {
    entries: RwLock<HashMap<CapabilityId, CapabilityAuditEntry>>,
}

impl CapabilityAuditLog {
    /// Construct an empty audit log.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record successful issuance.
    pub fn record_issue(&self, capability_id: CapabilityId, issued_by: SubjectRef) {
        let mut entries = self.write_entries();
        entries.insert(
            capability_id.clone(),
            CapabilityAuditEntry {
                capability_id,
                use_count: 0,
                last_used_at: None,
                last_used_op_kind: None,
                issued_by,
                revoked_by: None,
                revoked_at: None,
                expired_at: None,
            },
        );
    }

    /// Record successful use.
    pub fn record_use(&self, capability_id: &CapabilityId, op_kind: impl Into<String>) {
        let mut entries = self.write_entries();
        if let Some(entry) = entries.get_mut(capability_id) {
            entry.use_count = entry.use_count.saturating_add(1);
            entry.last_used_at = Some(Utc::now());
            entry.last_used_op_kind = Some(op_kind.into());
        }
    }

    /// Record successful revocation.
    pub fn record_revoke(&self, capability_id: &CapabilityId, revoked_by: SubjectRef) {
        let mut entries = self.write_entries();
        if let Some(entry) = entries.get_mut(capability_id) {
            entry.revoked_by = Some(revoked_by);
            entry.revoked_at = Some(Utc::now());
        }
    }

    /// Record expiration.
    pub fn record_expire(&self, capability_id: &CapabilityId) {
        let mut entries = self.write_entries();
        if let Some(entry) = entries.get_mut(capability_id) {
            entry.expired_at.get_or_insert_with(Utc::now);
        }
    }

    /// Look up one audit entry.
    #[must_use]
    pub fn lookup(&self, capability_id: &CapabilityId) -> Option<CapabilityAuditEntry> {
        self.read_entries().get(capability_id).cloned()
    }

    /// Return all audit entries in deterministic id order.
    #[must_use]
    pub fn list_all(&self) -> Vec<CapabilityAuditEntry> {
        let mut entries = self.read_entries().values().cloned().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.capability_id
                .as_str()
                .cmp(right.capability_id.as_str())
        });
        entries
    }

    fn read_entries(&self) -> RwLockReadGuard<'_, HashMap<CapabilityId, CapabilityAuditEntry>> {
        match self.entries.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write_entries(&self) -> RwLockWriteGuard<'_, HashMap<CapabilityId, CapabilityAuditEntry>> {
        match self.entries.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}
