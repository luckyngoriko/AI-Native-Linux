//! L6 Apps Evidence Emitter (S12.x ↔ S3.1) — typed lifecycle event emission
//! into the append-only Evidence Log.
//!
//! Every package register/update/rollback/session open/close produces a
//! chained evidence receipt. INV-015: NO secret material, NO raw key bytes
//! in any payload.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use aios_evidence::{EvidenceReceipt, ReceiptBuilder, ReceiptChain, RecordType, RetentionClass};

use crate::error::AppsError;
use crate::package::PackageId;
use crate::session::SessionId;
use crate::session_driver::SessionExitReason;
use crate::update_driver::{FailureClass, RollbackReason, UpdatePlanId};

// ---------------------------------------------------------------------------
// AppsRecordType — 10 apps-side lifecycle event discriminators
// ---------------------------------------------------------------------------

/// Closed set of L6 apps lifecycle event types.
///
/// These map to the nearest `aios_evidence::RecordType` variant at emission
/// time. The mapping is stable and one-directional: apps events flow into
/// the evidence log via these 10 discriminators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppsRecordType {
    /// Package registered in the package store.
    PackageRegistered,
    /// Update plan created.
    PackageUpdatePlanned,
    /// Update execution completed.
    PackageUpdateExecuted,
    /// Update verification completed.
    PackageUpdateVerified,
    /// Update activated and live.
    PackageUpdateActivated,
    /// Update rolled back.
    PackageUpdateRolledBack,
    /// Update entered terminal failure.
    PackageUpdateFailed,
    /// Session container opened.
    SessionOpened,
    /// Session container closed.
    SessionClosed,
    /// Session heartbeat expired.
    SessionHeartbeatExpired,
}

impl AppsRecordType {
    /// Map to the evidence crate's closed `RecordType`.
    const fn to_evidence_record_type(self) -> RecordType {
        match self {
            Self::PackageRegistered => RecordType::PackageObjectCreated,
            Self::PackageUpdatePlanned | Self::PackageUpdateExecuted => {
                RecordType::PackageObjectUpdated
            }
            Self::PackageUpdateVerified => RecordType::PackageVerified,
            Self::PackageUpdateActivated => RecordType::PackageInstalled,
            Self::PackageUpdateRolledBack => RecordType::PackageObjectRolledBack,
            Self::PackageUpdateFailed => RecordType::PackageInstallFailed,
            Self::SessionOpened => RecordType::AppLaunchStarted,
            Self::SessionClosed => RecordType::AppLaunchSucceeded,
            Self::SessionHeartbeatExpired => RecordType::AppObserveTimeout,
        }
    }

    /// All L6 app lifecycle events use standard 24-month retention.
    const fn retention_class() -> RetentionClass {
        RetentionClass::Standard24M
    }

    /// Wire-name string for this discriminator.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PackageRegistered => "PACKAGE_REGISTERED",
            Self::PackageUpdatePlanned => "PACKAGE_UPDATE_PLANNED",
            Self::PackageUpdateExecuted => "PACKAGE_UPDATE_EXECUTED",
            Self::PackageUpdateVerified => "PACKAGE_UPDATE_VERIFIED",
            Self::PackageUpdateActivated => "PACKAGE_UPDATE_ACTIVATED",
            Self::PackageUpdateRolledBack => "PACKAGE_UPDATE_ROLLED_BACK",
            Self::PackageUpdateFailed => "PACKAGE_UPDATE_FAILED",
            Self::SessionOpened => "SESSION_OPENED",
            Self::SessionClosed => "SESSION_CLOSED",
            Self::SessionHeartbeatExpired => "SESSION_HEARTBEAT_EXPIRED",
        }
    }
}

// ---------------------------------------------------------------------------
// UpdatePhaseRecord — which phase of an update lifecycle emitted
// ---------------------------------------------------------------------------

/// Discriminator for update lifecycle phase evidence.
///
/// Carries the reason/failure-class payload inline so the evidence receipt
/// captures the forensic context at emission time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpdatePhaseRecord {
    /// Plan created; awaiting execution.
    Planned,
    /// Execution step completed.
    Executed,
    /// Verification step completed.
    Verified,
    /// Activation step completed; update is live.
    Activated,
    /// Rollback completed with the triggering reason.
    RolledBack(RollbackReason),
    /// Update entered terminal failure with classified reason.
    Failed(FailureClass),
}

// ---------------------------------------------------------------------------
// SessionPhaseRecord — which phase of a session lifecycle emitted
// ---------------------------------------------------------------------------

/// Discriminator for session lifecycle phase evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionPhaseRecord {
    /// Session was opened and adapter-bound.
    Opened,
    /// Session was closed with exit reason.
    Closed(SessionExitReason),
    /// Session heartbeat window expired.
    HeartbeatExpired,
}

// ---------------------------------------------------------------------------
// AppsEvidenceReceipt — simplified apps-side receipt view
// ---------------------------------------------------------------------------

/// Simplified evidence receipt returned to L6 callers.
///
/// Carries the record identity, content hash, and chain sequence number.
/// The full `aios_evidence::EvidenceReceipt` is stored in the chain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppsEvidenceReceipt {
    /// Evidence receipt id (`evr_<ULID>`).
    pub record_id: String,
    /// BLAKE3-256 content hash (64 lowercase hex chars).
    pub hash: String,
    /// 0-based sequence position in the emitter's chain.
    pub sequence: u64,
}

impl From<&EvidenceReceipt> for AppsEvidenceReceipt {
    fn from(r: &EvidenceReceipt) -> Self {
        Self {
            record_id: r.receipt_id().as_str().to_owned(),
            hash: r.content_hash().to_owned(),
            sequence: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// AppsEvidenceEmitter trait
// ---------------------------------------------------------------------------

/// S12.x ↔ S3.1 — async contract for emitting apps lifecycle events into
/// the Evidence Log.
///
/// Three methods cover the full L6 event surface: package registration,
/// update lifecycle phases, and session lifecycle phases.
///
/// Implementations are optional (`Option<Arc<dyn AppsEvidenceEmitter>>`):
/// when `None`, no emission occurs and no error is raised.
#[async_trait]
pub trait AppsEvidenceEmitter: Send + Sync {
    /// Emit a `PACKAGE_REGISTERED` evidence record.
    ///
    /// Called after the package passes Ed25519 + BLAKE3 verification and
    /// is committed to the package store.
    async fn emit_package_registered(
        &self,
        package_id: &PackageId,
        name: &str,
        version: &str,
        content_hash: &str,
    ) -> Result<AppsEvidenceReceipt, AppsError>;

    /// Emit an update lifecycle phase evidence record.
    ///
    /// Called at each phase transition of the update FSM.
    async fn emit_update_event(
        &self,
        plan_id: &UpdatePlanId,
        package_id: &PackageId,
        phase: UpdatePhaseRecord,
    ) -> Result<AppsEvidenceReceipt, AppsError>;

    /// Emit a session lifecycle phase evidence record.
    ///
    /// Called at session open, close, and heartbeat expiry.
    async fn emit_session_event(
        &self,
        session_id: &SessionId,
        package_id: &PackageId,
        phase: SessionPhaseRecord,
    ) -> Result<AppsEvidenceReceipt, AppsError>;
}

// ---------------------------------------------------------------------------
// InMemoryAppsEvidenceEmitter
// ---------------------------------------------------------------------------

/// In-memory `AppsEvidenceEmitter` backed by a `ReceiptChain`.
///
/// Every emission seals a new receipt, appends it to the chain, and returns
/// an `AppsEvidenceReceipt` with the chain sequence number.
pub struct InMemoryAppsEvidenceEmitter {
    chain: Arc<RwLock<ReceiptChain>>,
    subject: String,
}

impl InMemoryAppsEvidenceEmitter {
    /// Create an emitter that stamps receipts with the given subject
    /// canonical id (e.g. `"service:aios-apps"`).
    #[must_use]
    pub fn new(subject: impl Into<String>) -> Self {
        Self {
            chain: Arc::new(RwLock::new(ReceiptChain::new())),
            subject: subject.into(),
        }
    }

    /// Return the number of receipts currently in the chain (test seam).
    #[allow(dead_code)]
    pub async fn receipt_count(&self) -> usize {
        self.chain.read().await.len()
    }

    /// Return the payload at the given 0-based index (integration-test seam).
    #[must_use]
    pub async fn get_payload(&self, index: usize) -> Option<serde_json::Value> {
        let chain = self.chain.read().await;
        chain.receipts().get(index).map(|r| r.payload().clone())
    }

    /// Verify the full hash-chain integrity (test seam).
    ///
    /// # Errors
    ///
    /// Returns `EvidenceEmitFailed` wrapping the underlying chain error.
    pub async fn verify_chain(&self) -> Result<(), AppsError> {
        self.chain
            .read()
            .await
            .verify_integrity()
            .map_err(|e| AppsError::EvidenceEmitFailed(format!("chain integrity: {e}")))
    }

    /// Shared seal-and-append helper.
    async fn seal_and_append(
        &self,
        apps_record_type: AppsRecordType,
        payload: serde_json::Value,
    ) -> Result<AppsEvidenceReceipt, AppsError> {
        let mut chain = self.chain.write().await;
        let prev = chain.receipts().last();
        let seq = chain.len() as u64;

        let builder = ReceiptBuilder::new(
            apps_record_type.to_evidence_record_type(),
            AppsRecordType::retention_class(),
            &self.subject,
        )
        .with_payload(payload);

        let receipt = builder
            .seal(prev)
            .map_err(|e| AppsError::EvidenceEmitFailed(format!("seal: {e}")))?;

        let mut apps_receipt = AppsEvidenceReceipt::from(&receipt);
        apps_receipt.sequence = seq;

        chain
            .append(receipt)
            .map_err(|e| AppsError::EvidenceEmitFailed(format!("append: {e}")))?;
        drop(chain);

        Ok(apps_receipt)
    }
}

#[async_trait]
impl AppsEvidenceEmitter for InMemoryAppsEvidenceEmitter {
    async fn emit_package_registered(
        &self,
        package_id: &PackageId,
        name: &str,
        version: &str,
        content_hash: &str,
    ) -> Result<AppsEvidenceReceipt, AppsError> {
        let payload = serde_json::json!({
            "package_id": package_id.0,
            "name": name,
            "version": version,
            "content_hash_blake3": content_hash,
        });
        self.seal_and_append(AppsRecordType::PackageRegistered, payload)
            .await
    }

    async fn emit_update_event(
        &self,
        plan_id: &UpdatePlanId,
        package_id: &PackageId,
        phase: UpdatePhaseRecord,
    ) -> Result<AppsEvidenceReceipt, AppsError> {
        let apps_record_type = match &phase {
            UpdatePhaseRecord::Planned => AppsRecordType::PackageUpdatePlanned,
            UpdatePhaseRecord::Executed => AppsRecordType::PackageUpdateExecuted,
            UpdatePhaseRecord::Verified => AppsRecordType::PackageUpdateVerified,
            UpdatePhaseRecord::Activated => AppsRecordType::PackageUpdateActivated,
            UpdatePhaseRecord::RolledBack(_) => AppsRecordType::PackageUpdateRolledBack,
            UpdatePhaseRecord::Failed(_) => AppsRecordType::PackageUpdateFailed,
        };

        let reason_str = match &phase {
            UpdatePhaseRecord::RolledBack(r) => serde_json::to_value(r)
                .ok()
                .and_then(|v| v.as_str().map(String::from)),
            UpdatePhaseRecord::Failed(f) => serde_json::to_value(f)
                .ok()
                .and_then(|v| v.as_str().map(String::from)),
            _ => None,
        };

        let payload = serde_json::json!({
            "plan_id": plan_id.0,
            "package_id": package_id.0,
            "phase": apps_record_type.as_str(),
            "reason": reason_str,
        });
        self.seal_and_append(apps_record_type, payload).await
    }

    async fn emit_session_event(
        &self,
        session_id: &SessionId,
        package_id: &PackageId,
        phase: SessionPhaseRecord,
    ) -> Result<AppsEvidenceReceipt, AppsError> {
        let apps_record_type = match &phase {
            SessionPhaseRecord::Opened => AppsRecordType::SessionOpened,
            SessionPhaseRecord::Closed(_) => AppsRecordType::SessionClosed,
            SessionPhaseRecord::HeartbeatExpired => AppsRecordType::SessionHeartbeatExpired,
        };

        let exit_reason = match &phase {
            SessionPhaseRecord::Closed(r) => serde_json::to_value(r)
                .ok()
                .and_then(|v| v.as_str().map(String::from)),
            _ => None,
        };

        let payload = serde_json::json!({
            "session_id": session_id.0,
            "package_id": package_id.0,
            "phase": apps_record_type.as_str(),
            "exit_reason": exit_reason,
        });
        self.seal_and_append(apps_record_type, payload).await
    }
}

// ---------------------------------------------------------------------------
// Unit tests (inline)
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    fn emitter() -> InMemoryAppsEvidenceEmitter {
        InMemoryAppsEvidenceEmitter::new("service:aios-apps")
    }

    fn pkg() -> PackageId {
        PackageId(format!(
            "pkg_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        ))
    }

    fn plan_id() -> UpdatePlanId {
        UpdatePlanId(format!(
            "updp_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        ))
    }

    fn sess() -> SessionId {
        SessionId(format!(
            "sess_{}",
            ulid::Ulid::new().to_string().to_lowercase()
        ))
    }

    #[test]
    fn apps_record_type_has_10_variants() {
        // Every AppsRecordType maps to a valid evidence RecordType.
        let variants = [
            AppsRecordType::PackageRegistered,
            AppsRecordType::PackageUpdatePlanned,
            AppsRecordType::PackageUpdateExecuted,
            AppsRecordType::PackageUpdateVerified,
            AppsRecordType::PackageUpdateActivated,
            AppsRecordType::PackageUpdateRolledBack,
            AppsRecordType::PackageUpdateFailed,
            AppsRecordType::SessionOpened,
            AppsRecordType::SessionClosed,
            AppsRecordType::SessionHeartbeatExpired,
        ];
        for v in &variants {
            let ev = v.to_evidence_record_type();
            let _ = ev.as_wire_str();
            let _ = v.as_str();
        }
    }

    #[tokio::test]
    async fn emit_package_registered_yields_receipt_with_sequence_0() {
        let em = emitter();
        let r = em
            .emit_package_registered(
                &pkg(),
                "firefox",
                "120.0",
                "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789",
            )
            .await
            .expect("emit");
        assert_eq!(r.sequence, 0);
        assert!(r.record_id.starts_with("evr_"));
        assert_eq!(r.hash.len(), 64);
    }

    #[tokio::test]
    async fn chained_hashes_increment_sequence() {
        let em = emitter();
        let p = pkg();
        let hash = "aaaa00001111222233334444555566667777888899990000aaaabbbbccccddddeeeeffff";

        let r0 = em
            .emit_package_registered(&p, "app-a", "1.0", hash)
            .await
            .expect("r0");
        let r1 = em
            .emit_package_registered(&p, "app-b", "2.0", hash)
            .await
            .expect("r1");
        let r2 = em
            .emit_package_registered(&p, "app-c", "3.0", hash)
            .await
            .expect("r2");

        assert_eq!(r0.sequence, 0);
        assert_eq!(r1.sequence, 1);
        assert_eq!(r2.sequence, 2);

        // Chain integrity must hold.
        em.verify_chain().await.expect("chain ok");
    }

    #[tokio::test]
    async fn update_phase_planned_emits() {
        let em = emitter();
        let r = em
            .emit_update_event(&plan_id(), &pkg(), UpdatePhaseRecord::Planned)
            .await
            .expect("emit");
        assert_eq!(r.sequence, 0);
        assert!(r.record_id.starts_with("evr_"));
    }

    #[tokio::test]
    async fn update_phase_executed_emits() {
        let em = emitter();
        let r = em
            .emit_update_event(&plan_id(), &pkg(), UpdatePhaseRecord::Executed)
            .await
            .expect("emit");
        assert_eq!(r.sequence, 0);
    }

    #[tokio::test]
    async fn update_phase_verified_emits() {
        let em = emitter();
        let r = em
            .emit_update_event(&plan_id(), &pkg(), UpdatePhaseRecord::Verified)
            .await
            .expect("emit");
        assert_eq!(r.sequence, 0);
    }

    #[tokio::test]
    async fn update_phase_activated_emits() {
        let em = emitter();
        let r = em
            .emit_update_event(&plan_id(), &pkg(), UpdatePhaseRecord::Activated)
            .await
            .expect("emit");
        assert_eq!(r.sequence, 0);
    }

    #[tokio::test]
    async fn rollback_reason_is_captured_in_payload() {
        let em = emitter();
        let _r = em
            .emit_update_event(
                &plan_id(),
                &pkg(),
                UpdatePhaseRecord::RolledBack(RollbackReason::VerifyFailed),
            )
            .await
            .expect("emit");
        // Verify the payload contains the reason by inspecting the chain.
        let payload = {
            let chain = em.chain.read().await;
            chain
                .receipts()
                .first()
                .expect("receipt present")
                .payload()
                .clone()
        };
        assert_eq!(payload["phase"], "PACKAGE_UPDATE_ROLLED_BACK");
        assert_eq!(payload["reason"], "VERIFY_FAILED");
    }

    #[tokio::test]
    async fn failure_class_is_captured_in_payload() {
        let em = emitter();
        let _r = em
            .emit_update_event(
                &plan_id(),
                &pkg(),
                UpdatePhaseRecord::Failed(FailureClass::ExecuteError),
            )
            .await
            .expect("emit");
        let payload = {
            let chain = em.chain.read().await;
            chain
                .receipts()
                .first()
                .expect("receipt present")
                .payload()
                .clone()
        };
        assert_eq!(payload["phase"], "PACKAGE_UPDATE_FAILED");
        assert_eq!(payload["reason"], "EXECUTE_ERROR");
    }

    #[tokio::test]
    async fn session_open_close_emit_sequence_increments() {
        let em = emitter();
        let s = sess();
        let p = pkg();

        let r_open = em
            .emit_session_event(&s, &p, SessionPhaseRecord::Opened)
            .await
            .expect("open");
        assert_eq!(r_open.sequence, 0);

        let r_close = em
            .emit_session_event(
                &s,
                &p,
                SessionPhaseRecord::Closed(SessionExitReason::ClosedByOwner),
            )
            .await
            .expect("close");
        assert_eq!(r_close.sequence, 1);

        em.verify_chain().await.expect("chain ok");
    }

    #[tokio::test]
    async fn session_closed_captures_exit_reason() {
        let em = emitter();
        let _r = em
            .emit_session_event(
                &sess(),
                &pkg(),
                SessionPhaseRecord::Closed(SessionExitReason::Crashed),
            )
            .await
            .expect("emit");
        let payload = {
            let chain = em.chain.read().await;
            chain
                .receipts()
                .first()
                .expect("receipt present")
                .payload()
                .clone()
        };
        assert_eq!(payload["exit_reason"], "CRASHED");
    }

    #[tokio::test]
    async fn heartbeat_expired_emits_correct_phase() {
        let em = emitter();
        let _r = em
            .emit_session_event(&sess(), &pkg(), SessionPhaseRecord::HeartbeatExpired)
            .await
            .expect("emit");
        let payload = {
            let chain = em.chain.read().await;
            chain
                .receipts()
                .first()
                .expect("receipt present")
                .payload()
                .clone()
        };
        assert_eq!(payload["phase"], "SESSION_HEARTBEAT_EXPIRED");
    }

    #[tokio::test]
    async fn hash_chain_verification_passes_after_multiple_emissions() {
        let em = emitter();
        let p = pkg();
        let s = sess();

        em.emit_package_registered(
            &p,
            "test-app",
            "1.0",
            "aaaa00001111222233334444555566667777888899990000aaaabbbbccccddddeeeeffff",
        )
        .await
        .expect("pkg");
        em.emit_update_event(&plan_id(), &p, UpdatePhaseRecord::Planned)
            .await
            .expect("planned");
        em.emit_update_event(&plan_id(), &p, UpdatePhaseRecord::Executed)
            .await
            .expect("executed");
        em.emit_session_event(&s, &p, SessionPhaseRecord::Opened)
            .await
            .expect("open");
        em.emit_session_event(
            &s,
            &p,
            SessionPhaseRecord::Closed(SessionExitReason::ClosedByOwner),
        )
        .await
        .expect("close");

        em.verify_chain().await.expect("full chain ok");
        assert_eq!(em.receipt_count().await, 5);
    }

    #[tokio::test]
    async fn inv015_no_secret_material_in_payload() {
        // Verify that typical emission payloads contain only public metadata
        // (package ids, names, versions, hashes) — no key bytes, no secrets.
        let em = emitter();
        let p = pkg();
        em.emit_package_registered(
            &p,
            "test-app",
            "1.0",
            "aaaa00001111222233334444555566667777888899990000aaaabbbbccccddddeeeeffff",
        )
        .await
        .expect("emit");

        let payload = {
            let chain = em.chain.read().await;
            chain
                .receipts()
                .first()
                .expect("receipt present")
                .payload()
                .clone()
        };
        let payload_str = serde_json::to_string(&payload).expect("ser");
        // Никакви необработени ключови байтове, пароли, токени
        assert!(!payload_str.contains("private_key"));
        assert!(!payload_str.contains("secret"));
        assert!(!payload_str.contains("password"));
        assert!(!payload_str.contains("token"));
        assert!(!payload_str.contains("signing_key"));
    }
}
