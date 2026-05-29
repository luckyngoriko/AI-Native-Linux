//! L10 Distribution Evidence Emitter (S11.1 §17 ↔ S3.1) — typed lifecycle
//! event emission into the append-only Evidence Log.
//!
//! Every package fetch, verification, install, quarantine, deplatform,
//! capability-lie detection, trust-chain break, key rotation, and external-bridge
//! admission produces a chained evidence receipt. The 19 discriminant variants are
//! crate-local per §17 ("queued for the next S3.1 consolidation"); emitters write
//! through the existing `aios_evidence::RecordType` variants (IDs 151..=169).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use aios_evidence::{ReceiptBuilder, ReceiptChain, RecordType, RetentionClass};

use crate::error::DistributionError;
use crate::ids::PackageId;
use crate::install_state::PackageVerificationResult;

// ---------------------------------------------------------------------------
// DistributionRecordType — 19 lifecycle event discriminators (S11.1 §17)
// ---------------------------------------------------------------------------

/// Closed set of L10 distribution lifecycle event types.
///
/// These 19 discriminators are crate-local per S11.1 §17 ("queued for the next
/// S3.1 consolidation"). At emission time each maps to the nearest
/// `aios_evidence::RecordType` variant (IDs 151..=169) via
/// [`to_evidence_record_type`](Self::to_evidence_record_type).
///
/// The mapping is stable and one-directional.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DistributionRecordType {
    /// Package fetch from a repository mirror started.
    PackageFetchStarted,
    /// Package binary signature + manifest verified successfully.
    PackageVerified,
    /// Package verification failed (signature, manifest, or hash).
    PackageVerificationFailed,
    /// Operator or policy approval requested for this package.
    PackageApprovalRequested,
    /// Package installed and transitioned to `Active`.
    PackageInstalled,
    /// Package install aborted before `Active` (terminal).
    PackageInstallFailed,
    /// Package moved to quarantine (manifest violation, deplatform, capability-lie, etc.).
    PackageQuarantined,
    /// Package fully uninstalled (`Removed` terminal state).
    PackageUninstalled,
    /// Version downgrade blocked by monotonic counter.
    PackageDowngradeBlocked,
    /// Capability-lie detected at first-run audit or runtime.
    CapabilityLieDetected,
    /// Trust chain broken (revoked, absent, or tampered at any hop).
    TrustChainBroken,
    /// Trust chain exceeds `MAX_CHAIN_DEPTH` (3 hops).
    TrustChainTooDeep,
    /// Manifest fields tampered post-sign.
    ManifestForged,
    /// Mirror returned a content hash that does not match the signed manifest.
    MirrorHashMismatchBlacklisted,
    /// Publisher signing key rotated.
    PublisherKeyRotated,
    /// Publisher deplatformed by AIOS governance.
    PublisherDeplatformed,
    /// External bridge package admitted into AIOS ecosystem.
    ExternalBridgePackageAdmitted,
    /// External bridge upstream signature failed to verify.
    ExternalBridgeUpstreamSignatureFailed,
    /// AIOS root key rotated.
    AiosRootKeyRotated,
}

impl DistributionRecordType {
    /// Map to the evidence crate's closed `RecordType`.
    const fn to_evidence_record_type(self) -> RecordType {
        match self {
            Self::PackageFetchStarted => RecordType::PackageFetchStarted,
            Self::PackageVerified => RecordType::PackageVerified,
            Self::PackageVerificationFailed => RecordType::PackageVerificationFailed,
            Self::PackageApprovalRequested => RecordType::PackageApprovalRequested,
            Self::PackageInstalled => RecordType::PackageInstalled,
            Self::PackageInstallFailed => RecordType::PackageInstallFailed,
            Self::PackageQuarantined => RecordType::PackageQuarantined,
            Self::PackageUninstalled => RecordType::PackageUninstalled,
            Self::PackageDowngradeBlocked => RecordType::PackageDowngradeBlocked,
            Self::CapabilityLieDetected => RecordType::CapabilityLieDetected,
            Self::TrustChainBroken => RecordType::TrustChainBroken,
            Self::TrustChainTooDeep => RecordType::TrustChainTooDeep,
            Self::ManifestForged => RecordType::ManifestForged,
            Self::MirrorHashMismatchBlacklisted => RecordType::MirrorHashMismatchBlacklisted,
            Self::PublisherKeyRotated => RecordType::PublisherKeyRotated,
            Self::PublisherDeplatformed => RecordType::PublisherDeplatformed,
            Self::ExternalBridgePackageAdmitted => RecordType::ExternalBridgePackageAdmitted,
            Self::ExternalBridgeUpstreamSignatureFailed => {
                RecordType::ExternalBridgeUpstreamSignatureFailed
            }
            Self::AiosRootKeyRotated => RecordType::AiosRootKeyRotated,
        }
    }

    /// Retention class for this event type per S11.1 §17.
    ///
    /// | Count | Class        |
    /// |-------|-------------|
    /// | 6     | Standard24M |
    /// | 4     | Extended60M |
    /// | 9     | Forever     |
    #[must_use]
    pub const fn retention(self) -> RetentionClass {
        match self {
            // ── FOREVER: denials, tamper, deplatform, quarantine, root-key rotation ──
            Self::PackageQuarantined
            | Self::CapabilityLieDetected
            | Self::TrustChainBroken
            | Self::TrustChainTooDeep
            | Self::ManifestForged
            | Self::MirrorHashMismatchBlacklisted
            | Self::PublisherKeyRotated
            | Self::PublisherDeplatformed
            | Self::AiosRootKeyRotated => RetentionClass::Forever,

            // ── EXTENDED_60M: high-value forensic events ──
            Self::PackageVerificationFailed
            | Self::PackageInstallFailed
            | Self::PackageDowngradeBlocked
            | Self::ExternalBridgeUpstreamSignatureFailed => RetentionClass::Extended60M,

            // ── STANDARD_24M: routine observability ──
            Self::PackageFetchStarted
            | Self::PackageVerified
            | Self::PackageApprovalRequested
            | Self::PackageInstalled
            | Self::PackageUninstalled
            | Self::ExternalBridgePackageAdmitted => RetentionClass::Standard24M,
        }
    }

    /// Wire-name string for this discriminator (S11.1 §17 `SCREAMING_CASE`).
    #[must_use]
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::PackageFetchStarted => "PACKAGE_FETCH_STARTED",
            Self::PackageVerified => "PACKAGE_VERIFIED",
            Self::PackageVerificationFailed => "PACKAGE_VERIFICATION_FAILED",
            Self::PackageApprovalRequested => "PACKAGE_APPROVAL_REQUESTED",
            Self::PackageInstalled => "PACKAGE_INSTALLED",
            Self::PackageInstallFailed => "PACKAGE_INSTALL_FAILED",
            Self::PackageQuarantined => "PACKAGE_QUARANTINED",
            Self::PackageUninstalled => "PACKAGE_UNINSTALLED",
            Self::PackageDowngradeBlocked => "PACKAGE_DOWNGRADE_BLOCKED",
            Self::CapabilityLieDetected => "CAPABILITY_LIE_DETECTED",
            Self::TrustChainBroken => "TRUST_CHAIN_BROKEN",
            Self::TrustChainTooDeep => "TRUST_CHAIN_TOO_DEEP",
            Self::ManifestForged => "MANIFEST_FORGED",
            Self::MirrorHashMismatchBlacklisted => "MIRROR_HASH_MISMATCH_BLACKLISTED",
            Self::PublisherKeyRotated => "PUBLISHER_KEY_ROTATED",
            Self::PublisherDeplatformed => "PUBLISHER_DEPLATFORMED",
            Self::ExternalBridgePackageAdmitted => "EXTERNAL_BRIDGE_PACKAGE_ADMITTED",
            Self::ExternalBridgeUpstreamSignatureFailed => {
                "EXTERNAL_BRIDGE_UPSTREAM_SIGNATURE_FAILED"
            }
            Self::AiosRootKeyRotated => "AIOS_ROOT_KEY_ROTATED",
        }
    }

    /// All 19 variants in declaration order (for tests and catalogues).
    #[must_use]
    pub const fn all() -> [Self; 19] {
        [
            Self::PackageFetchStarted,
            Self::PackageVerified,
            Self::PackageVerificationFailed,
            Self::PackageApprovalRequested,
            Self::PackageInstalled,
            Self::PackageInstallFailed,
            Self::PackageQuarantined,
            Self::PackageUninstalled,
            Self::PackageDowngradeBlocked,
            Self::CapabilityLieDetected,
            Self::TrustChainBroken,
            Self::TrustChainTooDeep,
            Self::ManifestForged,
            Self::MirrorHashMismatchBlacklisted,
            Self::PublisherKeyRotated,
            Self::PublisherDeplatformed,
            Self::ExternalBridgePackageAdmitted,
            Self::ExternalBridgeUpstreamSignatureFailed,
            Self::AiosRootKeyRotated,
        ]
    }
}

// ---------------------------------------------------------------------------
// record_type_for_failure — PackageVerificationResult → DistributionRecordType
// ---------------------------------------------------------------------------

/// Map a [`PackageVerificationResult`] failure to the appropriate
/// `DistributionRecordType` per S11.1 §17.
///
/// Success variants (`VerifiedAiosRoot`, `VerifiedPublisher`) are excluded
/// because they do not represent failures. `RepositoryKindMismatch` maps to
/// `PackageVerificationFailed` as a catch-all verification-failure record.
///
/// # Panics
///
/// Panics if called with a success variant — the caller is
/// expected to guard before invoking.
#[must_use]
pub fn record_type_for_failure(result: PackageVerificationResult) -> DistributionRecordType {
    match result {
        // ── Failure → specific §17 evidence types ──
        PackageVerificationResult::SignatureFailed
        | PackageVerificationResult::RepositoryKindMismatch
        | PackageVerificationResult::BundleTampered => DistributionRecordType::PackageVerificationFailed,
        PackageVerificationResult::TrustChainBroken => DistributionRecordType::TrustChainBroken,
        PackageVerificationResult::TrustChainTooDeep => DistributionRecordType::TrustChainTooDeep,
        PackageVerificationResult::PublisherDeplatformed => {
            DistributionRecordType::PublisherDeplatformed
        }
        PackageVerificationResult::HashMismatch => {
            DistributionRecordType::MirrorHashMismatchBlacklisted
        }
        PackageVerificationResult::ManifestForged => DistributionRecordType::ManifestForged,
        PackageVerificationResult::CapabilityLie => DistributionRecordType::CapabilityLieDetected,
        // ── Success variants: caller must not pass these ──
        PackageVerificationResult::VerifiedAiosRoot | PackageVerificationResult::VerifiedPublisher => {
            unreachable!(
                "record_type_for_failure called with success variant: {result:?}"
            )
        }
    }
}

// ---------------------------------------------------------------------------
// DistributionEvidenceReceipt — distribution-side receipt view
// ---------------------------------------------------------------------------

/// Evidence receipt returned to L10 callers.
///
/// Carries the record identity, content hash, and chain sequence number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistributionEvidenceReceipt {
    /// Evidence receipt id (`evr_<ULID>`).
    pub record_id: String,
    /// BLAKE3-256 content hash (64 lowercase hex chars).
    pub hash: String,
    /// 0-based sequence position in the emitter's chain.
    pub sequence: u64,
}

impl DistributionEvidenceReceipt {
    fn from_evidence_receipt(r: &aios_evidence::EvidenceReceipt) -> Self {
        Self {
            record_id: r.receipt_id().as_str().to_owned(),
            hash: r.content_hash().to_owned(),
            sequence: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// DistributionEvidenceEmitter
// ---------------------------------------------------------------------------

/// In-memory `DistributionEvidenceEmitter` backed by a `ReceiptChain`.
///
/// Every emission seals a new receipt, appends it to the chain, and returns a
/// `DistributionEvidenceReceipt` with the chain sequence number.
pub struct DistributionEvidenceEmitter {
    chain: Arc<RwLock<ReceiptChain>>,
    subject: String,
}

impl DistributionEvidenceEmitter {
    /// Create an emitter that stamps receipts with the given subject canonical
    /// id (e.g. `"service:aios-distribution"`).
    #[must_use]
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            chain: Arc::new(RwLock::new(ReceiptChain::new())),
            subject: subject.into(),
        }
    }

    /// Return the number of receipts currently in the chain (test seam).
    pub async fn receipt_count(&self) -> usize {
        self.chain.read().await.len()
    }

    /// Return the payload at the given 0-based index (test seam).
    #[must_use]
    pub async fn get_payload(&self, index: usize) -> Option<serde_json::Value> {
        let chain = self.chain.read().await;
        chain.receipts().get(index).map(|r| r.payload().clone())
    }

    /// Return the record type at the given 0-based index (test seam).
    #[must_use]
    pub async fn get_record_type(&self, index: usize) -> Option<RecordType> {
        let chain = self.chain.read().await;
        chain.receipts().get(index).map(|r| r.record_type())
    }

    /// Return the retention class at the given 0-based index (test seam).
    #[must_use]
    pub async fn get_retention_class(&self, index: usize) -> Option<RetentionClass> {
        let chain = self.chain.read().await;
        chain.receipts().get(index).map(|r| r.retention_class())
    }

    /// Verify the full hash-chain integrity (test seam).
    ///
    /// # Errors
    ///
    /// Returns `Internal` wrapping the underlying chain error.
    pub async fn verify_chain(&self) -> Result<(), DistributionError> {
        self.chain
            .read()
            .await
            .verify_integrity()
            .map_err(|e| DistributionError::Internal(format!("chain integrity: {e}")))
    }

    // -----------------------------------------------------------------------
    // emit — generic seal-and-append
    // -----------------------------------------------------------------------

    /// Build and append a typed evidence record.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if sealing or chaining fails.
    pub async fn emit(
        &self,
        record_type: DistributionRecordType,
        package_id: Option<&PackageId>,
        payload: serde_json::Value,
        _now: DateTime<Utc>,
    ) -> Result<DistributionEvidenceReceipt, DistributionError> {
        let mut chain = self.chain.write().await;
        let prev = chain.receipts().last();
        let seq = chain.len() as u64;

        let builder = ReceiptBuilder::new(
            record_type.to_evidence_record_type(),
            record_type.retention(),
            &self.subject,
        )
        .with_payload(payload);

        let receipt = builder
            .seal(prev)
            .map_err(|e| DistributionError::Internal(format!("seal: {e}")))?;

        let mut dist_receipt = DistributionEvidenceReceipt::from_evidence_receipt(&receipt);
        dist_receipt.sequence = seq;

        // Optionally inject package_id into the receipt metadata when available.
        if let Some(pkg) = package_id {
            let _ = pkg; // anchored in the payload, not a separate receipt field
        }

        chain
            .append(receipt)
            .map_err(|e| DistributionError::Internal(format!("append: {e}")))?;
        drop(chain);

        Ok(dist_receipt)
    }

    // -----------------------------------------------------------------------
    // Typed helpers — high-value records per S11.1 §17
    // -----------------------------------------------------------------------

    /// Emit a `PACKAGE_INSTALLED` evidence record.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if sealing or chaining fails.
    pub async fn emit_package_installed(
        &self,
        package_id: &PackageId,
    ) -> Result<DistributionEvidenceReceipt, DistributionError> {
        let payload = serde_json::json!({
            "package_id": package_id.0,
            "state": "ACTIVE",
        });
        self.emit(
            DistributionRecordType::PackageInstalled,
            Some(package_id),
            payload,
            Utc::now(),
        )
        .await
    }

    /// Emit a `PACKAGE_QUARANTINED` evidence record (FOREVER).
    ///
    /// # Errors
    ///
    /// Returns `Internal` if sealing or chaining fails.
    pub async fn emit_package_quarantined(
        &self,
        package_id: &PackageId,
        reason: &str,
    ) -> Result<DistributionEvidenceReceipt, DistributionError> {
        let payload = serde_json::json!({
            "package_id": package_id.0,
            "reason": reason,
        });
        self.emit(
            DistributionRecordType::PackageQuarantined,
            Some(package_id),
            payload,
            Utc::now(),
        )
        .await
    }

    /// Emit a `CAPABILITY_LIE_DETECTED` evidence record (FOREVER).
    ///
    /// # Errors
    ///
    /// Returns `Internal` if sealing or chaining fails.
    pub async fn emit_capability_lie_detected(
        &self,
        package_id: &PackageId,
        drift: &str,
    ) -> Result<DistributionEvidenceReceipt, DistributionError> {
        let payload = serde_json::json!({
            "package_id": package_id.0,
            "drift": drift,
        });
        self.emit(
            DistributionRecordType::CapabilityLieDetected,
            Some(package_id),
            payload,
            Utc::now(),
        )
        .await
    }

    /// Emit a `TRUST_CHAIN_BROKEN` evidence record (FOREVER).
    ///
    /// # Errors
    ///
    /// Returns `Internal` if sealing or chaining fails.
    pub async fn emit_trust_chain_broken(
        &self,
        package_id: &PackageId,
    ) -> Result<DistributionEvidenceReceipt, DistributionError> {
        let payload = serde_json::json!({
            "package_id": package_id.0,
        });
        self.emit(
            DistributionRecordType::TrustChainBroken,
            Some(package_id),
            payload,
            Utc::now(),
        )
        .await
    }

    /// Emit a `PUBLISHER_DEPLATFORMED` evidence record (FOREVER).
    ///
    /// # Errors
    ///
    /// Returns `Internal` if sealing or chaining fails.
    pub async fn emit_publisher_deplatformed(
        &self,
        reason: &str,
    ) -> Result<DistributionEvidenceReceipt, DistributionError> {
        let payload = serde_json::json!({
            "reason": reason,
        });
        self.emit(
            DistributionRecordType::PublisherDeplatformed,
            None,
            payload,
            Utc::now(),
        )
        .await
    }

    /// Emit a `PACKAGE_DOWNGRADE_BLOCKED` evidence record (EXTENDED_60M).
    ///
    /// # Errors
    ///
    /// Returns `Internal` if sealing or chaining fails.
    pub async fn emit_package_downgrade_blocked(
        &self,
        package_id: &PackageId,
        current_version: &str,
        attempted_version: &str,
    ) -> Result<DistributionEvidenceReceipt, DistributionError> {
        let payload = serde_json::json!({
            "package_id": package_id.0,
            "current_version": current_version,
            "attempted_version": attempted_version,
        });
        self.emit(
            DistributionRecordType::PackageDowngradeBlocked,
            Some(package_id),
            payload,
            Utc::now(),
        )
        .await
    }

    /// Emit a `MIRROR_HASH_MISMATCH_BLACKLISTED` evidence record (FOREVER).
    ///
    /// # Errors
    ///
    /// Returns `Internal` if sealing or chaining fails.
    pub async fn emit_mirror_hash_mismatch_blacklisted(
        &self,
        package_id: &PackageId,
        expected_hash: &str,
        actual_hash: &str,
    ) -> Result<DistributionEvidenceReceipt, DistributionError> {
        let payload = serde_json::json!({
            "package_id": package_id.0,
            "expected_hash": expected_hash,
            "actual_hash": actual_hash,
        });
        self.emit(
            DistributionRecordType::MirrorHashMismatchBlacklisted,
            Some(package_id),
            payload,
            Utc::now(),
        )
        .await
    }
}

// ---------------------------------------------------------------------------
// Retention-group accessors (for tests / catalogue)
// ---------------------------------------------------------------------------

/// Return the 6 variants with [`RetentionClass::Standard24M`].
#[must_use]
pub const fn standard_24m_variants() -> [DistributionRecordType; 6] {
    [
        DistributionRecordType::PackageFetchStarted,
        DistributionRecordType::PackageVerified,
        DistributionRecordType::PackageApprovalRequested,
        DistributionRecordType::PackageInstalled,
        DistributionRecordType::PackageUninstalled,
        DistributionRecordType::ExternalBridgePackageAdmitted,
    ]
}

/// Return the 4 variants with [`RetentionClass::Extended60M`].
#[must_use]
pub const fn extended_60m_variants() -> [DistributionRecordType; 4] {
    [
        DistributionRecordType::PackageVerificationFailed,
        DistributionRecordType::PackageInstallFailed,
        DistributionRecordType::PackageDowngradeBlocked,
        DistributionRecordType::ExternalBridgeUpstreamSignatureFailed,
    ]
}

/// Return the 9 variants with [`RetentionClass::Forever`].
#[must_use]
pub const fn forever_variants() -> [DistributionRecordType; 9] {
    [
        DistributionRecordType::PackageQuarantined,
        DistributionRecordType::CapabilityLieDetected,
        DistributionRecordType::TrustChainBroken,
        DistributionRecordType::TrustChainTooDeep,
        DistributionRecordType::ManifestForged,
        DistributionRecordType::MirrorHashMismatchBlacklisted,
        DistributionRecordType::PublisherKeyRotated,
        DistributionRecordType::PublisherDeplatformed,
        DistributionRecordType::AiosRootKeyRotated,
    ]
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // wire_name
    // -----------------------------------------------------------------------

    #[test]
    fn wire_name_matches_spec_for_all_19() {
        let expected: [(&str, DistributionRecordType); 19] = [
            ("PACKAGE_FETCH_STARTED", DistributionRecordType::PackageFetchStarted),
            ("PACKAGE_VERIFIED", DistributionRecordType::PackageVerified),
            (
                "PACKAGE_VERIFICATION_FAILED",
                DistributionRecordType::PackageVerificationFailed,
            ),
            (
                "PACKAGE_APPROVAL_REQUESTED",
                DistributionRecordType::PackageApprovalRequested,
            ),
            ("PACKAGE_INSTALLED", DistributionRecordType::PackageInstalled),
            (
                "PACKAGE_INSTALL_FAILED",
                DistributionRecordType::PackageInstallFailed,
            ),
            (
                "PACKAGE_QUARANTINED",
                DistributionRecordType::PackageQuarantined,
            ),
            (
                "PACKAGE_UNINSTALLED",
                DistributionRecordType::PackageUninstalled,
            ),
            (
                "PACKAGE_DOWNGRADE_BLOCKED",
                DistributionRecordType::PackageDowngradeBlocked,
            ),
            (
                "CAPABILITY_LIE_DETECTED",
                DistributionRecordType::CapabilityLieDetected,
            ),
            ("TRUST_CHAIN_BROKEN", DistributionRecordType::TrustChainBroken),
            (
                "TRUST_CHAIN_TOO_DEEP",
                DistributionRecordType::TrustChainTooDeep,
            ),
            ("MANIFEST_FORGED", DistributionRecordType::ManifestForged),
            (
                "MIRROR_HASH_MISMATCH_BLACKLISTED",
                DistributionRecordType::MirrorHashMismatchBlacklisted,
            ),
            (
                "PUBLISHER_KEY_ROTATED",
                DistributionRecordType::PublisherKeyRotated,
            ),
            (
                "PUBLISHER_DEPLATFORMED",
                DistributionRecordType::PublisherDeplatformed,
            ),
            (
                "EXTERNAL_BRIDGE_PACKAGE_ADMITTED",
                DistributionRecordType::ExternalBridgePackageAdmitted,
            ),
            (
                "EXTERNAL_BRIDGE_UPSTREAM_SIGNATURE_FAILED",
                DistributionRecordType::ExternalBridgeUpstreamSignatureFailed,
            ),
            (
                "AIOS_ROOT_KEY_ROTATED",
                DistributionRecordType::AiosRootKeyRotated,
            ),
        ];
        for (spec_name, variant) in &expected {
            assert_eq!(
                variant.wire_name(),
                *spec_name,
                "wire_name mismatch for {variant:?}"
            );
        }
    }

    #[test]
    fn wire_names_distinct_count_is_19() {
        let names: std::collections::BTreeSet<&str> =
            DistributionRecordType::all().iter().map(|v| v.wire_name()).collect();
        assert_eq!(names.len(), 19, "all 19 wire_names must be distinct");
    }

    // -----------------------------------------------------------------------
    // retention
    // -----------------------------------------------------------------------

    #[test]
    fn all_19_variants_present() {
        assert_eq!(DistributionRecordType::all().len(), 19);
    }

    #[test]
    fn exactly_9_forever() {
        let forever = forever_variants();
        assert_eq!(forever.len(), 9);
        for v in &forever {
            assert_eq!(v.retention(), RetentionClass::Forever);
        }
    }

    #[test]
    fn exactly_4_extended_60m() {
        let ext = extended_60m_variants();
        assert_eq!(ext.len(), 4);
        for v in &ext {
            assert_eq!(v.retention(), RetentionClass::Extended60M);
        }
    }

    #[test]
    fn exactly_6_standard_24m() {
        let std24 = standard_24m_variants();
        assert_eq!(std24.len(), 6);
        for v in &std24 {
            assert_eq!(v.retention(), RetentionClass::Standard24M);
        }
    }

    #[test]
    fn retention_counts_sum_to_19() {
        let forever = forever_variants().len();
        let ext = extended_60m_variants().len();
        let std24 = standard_24m_variants().len();
        assert_eq!(forever + ext + std24, 19);
    }

    #[test]
    fn retention_matches_spec_for_each_variant() {
        use RetentionClass::{Extended60M, Forever, Standard24M};
        let spec: [(DistributionRecordType, RetentionClass); 19] = [
            (DistributionRecordType::PackageFetchStarted, Standard24M),
            (DistributionRecordType::PackageVerified, Standard24M),
            (
                DistributionRecordType::PackageVerificationFailed,
                Extended60M,
            ),
            (
                DistributionRecordType::PackageApprovalRequested,
                Standard24M,
            ),
            (DistributionRecordType::PackageInstalled, Standard24M),
            (DistributionRecordType::PackageInstallFailed, Extended60M),
            (DistributionRecordType::PackageQuarantined, Forever),
            (DistributionRecordType::PackageUninstalled, Standard24M),
            (
                DistributionRecordType::PackageDowngradeBlocked,
                Extended60M,
            ),
            (DistributionRecordType::CapabilityLieDetected, Forever),
            (DistributionRecordType::TrustChainBroken, Forever),
            (DistributionRecordType::TrustChainTooDeep, Forever),
            (DistributionRecordType::ManifestForged, Forever),
            (
                DistributionRecordType::MirrorHashMismatchBlacklisted,
                Forever,
            ),
            (DistributionRecordType::PublisherKeyRotated, Forever),
            (DistributionRecordType::PublisherDeplatformed, Forever),
            (
                DistributionRecordType::ExternalBridgePackageAdmitted,
                Standard24M,
            ),
            (
                DistributionRecordType::ExternalBridgeUpstreamSignatureFailed,
                Extended60M,
            ),
            (DistributionRecordType::AiosRootKeyRotated, Forever),
        ];
        for (variant, expected_retention) in &spec {
            assert_eq!(
                variant.retention(),
                *expected_retention,
                "retention mismatch for {variant:?}"
            );
        }
    }

    // -----------------------------------------------------------------------
    // record_type_for_failure
    // -----------------------------------------------------------------------

    #[test]
    fn record_type_for_failure_trust_chain_too_deep() {
        assert_eq!(
            record_type_for_failure(PackageVerificationResult::TrustChainTooDeep),
            DistributionRecordType::TrustChainTooDeep,
        );
    }

    #[test]
    fn record_type_for_failure_capability_lie() {
        assert_eq!(
            record_type_for_failure(PackageVerificationResult::CapabilityLie),
            DistributionRecordType::CapabilityLieDetected,
        );
    }

    #[test]
    fn record_type_for_failure_manifest_forged() {
        assert_eq!(
            record_type_for_failure(PackageVerificationResult::ManifestForged),
            DistributionRecordType::ManifestForged,
        );
    }

    #[test]
    fn record_type_for_failure_hash_mismatch() {
        assert_eq!(
            record_type_for_failure(PackageVerificationResult::HashMismatch),
            DistributionRecordType::MirrorHashMismatchBlacklisted,
        );
    }

    #[test]
    fn record_type_for_failure_signature_failed() {
        assert_eq!(
            record_type_for_failure(PackageVerificationResult::SignatureFailed),
            DistributionRecordType::PackageVerificationFailed,
        );
    }

    #[test]
    fn record_type_for_failure_trust_chain_broken() {
        assert_eq!(
            record_type_for_failure(PackageVerificationResult::TrustChainBroken),
            DistributionRecordType::TrustChainBroken,
        );
    }

    #[test]
    fn record_type_for_failure_publisher_deplatformed() {
        assert_eq!(
            record_type_for_failure(PackageVerificationResult::PublisherDeplatformed),
            DistributionRecordType::PublisherDeplatformed,
        );
    }

    #[test]
    fn record_type_for_failure_bundle_tampered() {
        assert_eq!(
            record_type_for_failure(PackageVerificationResult::BundleTampered),
            DistributionRecordType::PackageVerificationFailed,
        );
    }

    #[test]
    fn record_type_for_failure_repo_kind_mismatch() {
        assert_eq!(
            record_type_for_failure(PackageVerificationResult::RepositoryKindMismatch),
            DistributionRecordType::PackageVerificationFailed,
        );
    }

    // -----------------------------------------------------------------------
    // emitter — typed helpers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn emit_package_installed_appends_with_correct_type_and_standard_24m() {
        let emitter = DistributionEvidenceEmitter::new("test:distribution");
        let pkg = PackageId("pkg:acme:hello".into());

        let receipt = emitter
            .emit_package_installed(&pkg)
            .await
            .expect("emit_package_installed");

        assert!(!receipt.record_id.is_empty());
        assert!(!receipt.hash.is_empty());
        assert_eq!(receipt.sequence, 0);

        let rt = emitter.get_record_type(0).await.expect("record type");
        assert_eq!(rt, RecordType::PackageInstalled);

        let rc = emitter.get_retention_class(0).await.expect("retention");
        assert_eq!(rc, RetentionClass::Standard24M);

        let payload = emitter.get_payload(0).await.expect("payload");
        assert_eq!(payload["package_id"], "pkg:acme:hello");
        assert_eq!(payload["state"], "ACTIVE");
    }

    #[tokio::test]
    async fn emit_package_quarantined_forever_record_carries_reason() {
        let emitter = DistributionEvidenceEmitter::new("test:distribution");
        let pkg = PackageId("pkg:evil:tool".into());

        emitter
            .emit_package_quarantined(&pkg, "capability-lie detected at audit")
            .await
            .expect("emit_package_quarantined");

        let rc = emitter.get_retention_class(0).await.expect("retention");
        assert_eq!(rc, RetentionClass::Forever);

        let rt = emitter.get_record_type(0).await.expect("record type");
        assert_eq!(rt, RecordType::PackageQuarantined);

        let payload = emitter.get_payload(0).await.expect("payload");
        assert_eq!(payload["package_id"], "pkg:evil:tool");
        assert_eq!(
            payload["reason"],
            "capability-lie detected at audit"
        );
    }

    #[tokio::test]
    async fn emit_capability_lie_detected_forever_with_drift() {
        let emitter = DistributionEvidenceEmitter::new("test:distribution");
        let pkg = PackageId("pkg:shady:lib".into());

        emitter
            .emit_capability_lie_detected(&pkg, "claims-gpu-access but no GPU observed")
            .await
            .expect("emit_capability_lie_detected");

        let rc = emitter.get_retention_class(0).await.expect("retention");
        assert_eq!(rc, RetentionClass::Forever);

        let rt = emitter.get_record_type(0).await.expect("record type");
        assert_eq!(rt, RecordType::CapabilityLieDetected);

        let payload = emitter.get_payload(0).await.expect("payload");
        assert_eq!(payload["drift"], "claims-gpu-access but no GPU observed");
    }

    #[tokio::test]
    async fn emit_publisher_deplatformed_forever_with_reason() {
        let emitter = DistributionEvidenceEmitter::new("test:distribution");

        emitter
            .emit_publisher_deplatformed("governance vote #42")
            .await
            .expect("emit_publisher_deplatformed");

        let rc = emitter.get_retention_class(0).await.expect("retention");
        assert_eq!(rc, RetentionClass::Forever);

        let rt = emitter.get_record_type(0).await.expect("record type");
        assert_eq!(rt, RecordType::PublisherDeplatformed);

        let payload = emitter.get_payload(0).await.expect("payload");
        assert_eq!(payload["reason"], "governance vote #42");
    }

    #[tokio::test]
    async fn emit_package_downgrade_blocked_extended_60m() {
        let emitter = DistributionEvidenceEmitter::new("test:distribution");
        let pkg = PackageId("pkg:acme:app".into());

        emitter
            .emit_package_downgrade_blocked(&pkg, "2.1.0", "2.0.0")
            .await
            .expect("emit_package_downgrade_blocked");

        let rc = emitter.get_retention_class(0).await.expect("retention");
        assert_eq!(rc, RetentionClass::Extended60M);

        let rt = emitter.get_record_type(0).await.expect("record type");
        assert_eq!(rt, RecordType::PackageDowngradeBlocked);

        let payload = emitter.get_payload(0).await.expect("payload");
        assert_eq!(payload["current_version"], "2.1.0");
        assert_eq!(payload["attempted_version"], "2.0.0");
    }

    #[tokio::test]
    async fn emit_trust_chain_broken_forever() {
        let emitter = DistributionEvidenceEmitter::new("test:distribution");
        let pkg = PackageId("pkg:bogus:tool".into());

        emitter
            .emit_trust_chain_broken(&pkg)
            .await
            .expect("emit_trust_chain_broken");

        let rc = emitter.get_retention_class(0).await.expect("retention");
        assert_eq!(rc, RetentionClass::Forever);

        let rt = emitter.get_record_type(0).await.expect("record type");
        assert_eq!(rt, RecordType::TrustChainBroken);
    }

    #[tokio::test]
    async fn emit_mirror_hash_mismatch_blacklisted_forever() {
        let emitter = DistributionEvidenceEmitter::new("test:distribution");
        let pkg = PackageId("pkg:acme:app".into());

        emitter
            .emit_mirror_hash_mismatch_blacklisted(
                &pkg,
                "abc123def456...",
                "999999badbad...",
            )
            .await
            .expect("emit_mirror_hash_mismatch_blacklisted");

        let rc = emitter.get_retention_class(0).await.expect("retention");
        assert_eq!(rc, RetentionClass::Forever);

        let rt = emitter.get_record_type(0).await.expect("record type");
        assert_eq!(rt, RecordType::MirrorHashMismatchBlacklisted);

        let payload = emitter.get_payload(0).await.expect("payload");
        assert_eq!(payload["expected_hash"], "abc123def456...");
        assert_eq!(payload["actual_hash"], "999999badbad...");
    }

    // -----------------------------------------------------------------------
    // emitter — generic emit + chain
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn generic_emit_appends_and_returns_receipt() {
        let emitter = DistributionEvidenceEmitter::new("test:distribution");
        let pkg = PackageId("pkg:test:lib".into());
        let payload = serde_json::json!({"key": "value"});

        let receipt = emitter
            .emit(
                DistributionRecordType::PackageFetchStarted,
                Some(&pkg),
                payload,
                Utc::now(),
            )
            .await
            .expect("emit");

        assert_eq!(receipt.sequence, 0);
        assert_eq!(emitter.receipt_count().await, 1);
    }

    #[tokio::test]
    async fn chain_grows_with_multiple_emissions() {
        let emitter = DistributionEvidenceEmitter::new("test:distribution");
        let pkg = PackageId("pkg:test:lib".into());

        emitter
            .emit(
                DistributionRecordType::PackageFetchStarted,
                Some(&pkg),
                serde_json::json!({"step": 1}),
                Utc::now(),
            )
            .await
            .expect("1");
        emitter
            .emit(
                DistributionRecordType::PackageVerified,
                Some(&pkg),
                serde_json::json!({"step": 2}),
                Utc::now(),
            )
            .await
            .expect("2");
        emitter
            .emit_package_installed(&pkg)
            .await
            .expect("3");

        assert_eq!(emitter.receipt_count().await, 3);
        emitter.verify_chain().await.expect("chain must verify");
    }

    #[tokio::test]
    async fn verify_chain_maintains_integrity() {
        let emitter = DistributionEvidenceEmitter::new("test:distribution");
        let pkg = PackageId("pkg:test:lib".into());

        for i in 0..5 {
            emitter
                .emit(
                    DistributionRecordType::all()[i],
                    Some(&pkg),
                    serde_json::json!({"idx": i}),
                    Utc::now(),
                )
                .await
                .expect("emit");
        }

        emitter.verify_chain().await.expect("5-receipt chain ok");
    }
}
