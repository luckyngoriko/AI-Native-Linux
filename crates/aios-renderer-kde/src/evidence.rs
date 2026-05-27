//! L7 KDE Renderer Evidence Emitter (S7.4 ↔ S3.1) — typed lifecycle event
//! emission into the append-only Evidence Log.
//!
//! Every surface allocate/release, layer-shell rejection, recovery entry/exit,
//! degraded fallback, `KWin` script verify/reject, and icon bundle verify/reject
//! produces a chained evidence receipt. INV-015: NO secret material, NO raw
//! key bytes in any payload.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use aios_evidence::{EvidenceReceipt, ReceiptBuilder, ReceiptChain, RecordType, RetentionClass};

use crate::error::KdeRendererError;
use crate::recovery_shell::DegradedTrigger;
use crate::renderer::RecoveryEntryReceipt;
use crate::types::KdeSurfaceDescriptor;
use crate::zone::CompositionZone;

// ---------------------------------------------------------------------------
// KdeRecordType — 10 renderer-side lifecycle event discriminators
// ---------------------------------------------------------------------------

/// Closed set of L7 KDE renderer lifecycle event types.
///
/// These map to the nearest `aios_evidence::RecordType` variant at emission
/// time. The mapping is stable and one-directional.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum KdeRecordType {
    /// Surface allocated in the KDE renderer.
    KdeSurfaceAllocated,
    /// Surface released from the KDE renderer.
    KdeSurfaceReleased,
    /// wlr-layer-shell zone claim rejected (INV I4).
    KdeLayerShellRejected,
    /// Recovery shell session entered (INV I5).
    KdeRecoveryEntered,
    /// Recovery shell session exited.
    KdeRecoveryExited,
    /// Renderer entered degraded text-only fallback (INV I7, FOREVER retention).
    KdeRendererDegraded,
    /// `KWin` script verified and loaded (INV I8).
    KdeKwinScriptVerified,
    /// `KWin` script rejected (INV I8 negative path).
    KdeKwinScriptRejected,
    /// Constitutional icon bundle verified (INV I6).
    KdeIconBundleVerified,
    /// Constitutional icon bundle rejected (INV I6 negative path).
    KdeIconBundleRejected,
}

impl KdeRecordType {
    /// Map to the evidence crate's closed `RecordType`.
    const fn to_evidence_record_type(self) -> RecordType {
        match self {
            Self::KdeSurfaceAllocated => RecordType::SurfaceCreated,
            Self::KdeSurfaceReleased => RecordType::SurfaceDestroyed,
            Self::KdeLayerShellRejected => RecordType::KdeLayerShellRejected,
            Self::KdeRecoveryEntered => RecordType::KdeRecoveryShellStarted,
            Self::KdeRecoveryExited => RecordType::RecoveryEvent,
            Self::KdeRendererDegraded => RecordType::KdeRendererDegraded,
            Self::KdeKwinScriptVerified => RecordType::KdeKwinScriptLoaded,
            Self::KdeKwinScriptRejected => RecordType::KdeKwinScriptRejected,
            Self::KdeIconBundleVerified => RecordType::ThemeLoaded,
            Self::KdeIconBundleRejected => RecordType::ThemeRejected,
        }
    }

    /// Retention class for this event type.
    ///
    /// `KdeRendererDegraded` is FOREVER per INV I7. All other KDE events use
    /// standard 24-month retention.
    const fn retention_class(self) -> RetentionClass {
        match self {
            Self::KdeRendererDegraded => RetentionClass::Forever,
            _ => RetentionClass::Standard24M,
        }
    }

    /// Wire-name string for this discriminator.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::KdeSurfaceAllocated => "KDE_SURFACE_ALLOCATED",
            Self::KdeSurfaceReleased => "KDE_SURFACE_RELEASED",
            Self::KdeLayerShellRejected => "KDE_LAYER_SHELL_REJECTED",
            Self::KdeRecoveryEntered => "KDE_RECOVERY_ENTERED",
            Self::KdeRecoveryExited => "KDE_RECOVERY_EXITED",
            Self::KdeRendererDegraded => "KDE_RENDERER_DEGRADED",
            Self::KdeKwinScriptVerified => "KDE_KWIN_SCRIPT_VERIFIED",
            Self::KdeKwinScriptRejected => "KDE_KWIN_SCRIPT_REJECTED",
            Self::KdeIconBundleVerified => "KDE_ICON_BUNDLE_VERIFIED",
            Self::KdeIconBundleRejected => "KDE_ICON_BUNDLE_REJECTED",
        }
    }
}

// ---------------------------------------------------------------------------
// KdeEvidenceReceipt — simplified renderer-side receipt view
// ---------------------------------------------------------------------------

/// Simplified evidence receipt returned to L7 callers.
///
/// Carries the record identity, content hash, and chain sequence number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdeEvidenceReceipt {
    /// Evidence receipt id (`evr_<ULID>`).
    pub record_id: String,
    /// BLAKE3-256 content hash (64 lowercase hex chars).
    pub hash: String,
    /// 0-based sequence position in the emitter's chain.
    pub sequence: u64,
}

impl From<&EvidenceReceipt> for KdeEvidenceReceipt {
    fn from(r: &EvidenceReceipt) -> Self {
        Self {
            record_id: r.receipt_id().as_str().to_owned(),
            hash: r.content_hash().to_owned(),
            sequence: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// KdeEvidenceEmitter trait
// ---------------------------------------------------------------------------

/// S7.4 ↔ S3.1 — async contract for emitting KDE renderer lifecycle events
/// into the Evidence Log.
///
/// Ten methods cover the full L7 KDE event surface. Implementations are
/// optional (`Option<Arc<dyn KdeEvidenceEmitter>>`): when `None`, no
/// emission occurs and no error is raised.
#[async_trait]
pub trait KdeEvidenceEmitter: Send + Sync {
    /// Emit a `KDE_SURFACE_ALLOCATED` evidence record.
    async fn emit_surface_allocated(
        &self,
        descriptor: &KdeSurfaceDescriptor,
        actor: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError>;

    /// Emit a `KDE_SURFACE_RELEASED` evidence record.
    async fn emit_surface_released(
        &self,
        descriptor: &KdeSurfaceDescriptor,
        actor: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError>;

    /// Emit a `KDE_LAYER_SHELL_REJECTED` evidence record (INV I4).
    async fn emit_layer_shell_rejected(
        &self,
        claimed_by: &str,
        zone: CompositionZone,
        reason: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError>;

    /// Emit a `KDE_RECOVERY_ENTERED` evidence record.
    async fn emit_recovery_entered(
        &self,
        receipt: &RecoveryEntryReceipt,
        actor: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError>;

    /// Emit a `KDE_RECOVERY_EXITED` evidence record.
    async fn emit_recovery_exited(
        &self,
        actor: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError>;

    /// Emit a `KDE_RENDERER_DEGRADED` evidence record (INV I7, FOREVER retention).
    async fn emit_renderer_degraded(
        &self,
        trigger: DegradedTrigger,
        reason: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError>;

    /// Emit a `KDE_KWIN_SCRIPT_VERIFIED` evidence record (INV I8).
    async fn emit_kwin_script_verified(
        &self,
        script_id: &str,
        signer_fingerprint: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError>;

    /// Emit a `KDE_KWIN_SCRIPT_REJECTED` evidence record (INV I8 negative).
    async fn emit_kwin_script_rejected(
        &self,
        script_id: &str,
        reason: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError>;

    /// Emit a `KDE_ICON_BUNDLE_VERIFIED` evidence record (INV I6).
    async fn emit_icon_bundle_verified(
        &self,
        theme_id: &str,
        signer_fingerprint: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError>;

    /// Emit a `KDE_ICON_BUNDLE_REJECTED` evidence record (INV I6 negative).
    async fn emit_icon_bundle_rejected(
        &self,
        theme_id: &str,
        reason: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError>;
}

// ---------------------------------------------------------------------------
// InMemoryKdeEvidenceEmitter
// ---------------------------------------------------------------------------

/// In-memory `KdeEvidenceEmitter` backed by a `ReceiptChain`.
///
/// Every emission seals a new receipt, appends it to the chain, and returns
/// a `KdeEvidenceReceipt` with the chain sequence number.
pub struct InMemoryKdeEvidenceEmitter {
    chain: Arc<RwLock<ReceiptChain>>,
    subject: String,
}

impl InMemoryKdeEvidenceEmitter {
    /// Create an emitter that stamps receipts with the given subject
    /// canonical id (e.g. `"service:aios-renderer-kde"`).
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
    pub async fn verify_chain(&self) -> Result<(), KdeRendererError> {
        self.chain
            .read()
            .await
            .verify_integrity()
            .map_err(|e| KdeRendererError::Internal(format!("chain integrity: {e}")))
    }

    /// Shared seal-and-append helper.
    async fn seal_and_append(
        &self,
        kde_record_type: KdeRecordType,
        payload: serde_json::Value,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let mut chain = self.chain.write().await;
        let prev = chain.receipts().last();
        let seq = chain.len() as u64;

        let builder = ReceiptBuilder::new(
            kde_record_type.to_evidence_record_type(),
            kde_record_type.retention_class(),
            &self.subject,
        )
        .with_payload(payload);

        let receipt = builder
            .seal(prev)
            .map_err(|e| KdeRendererError::Internal(format!("seal: {e}")))?;

        let mut kde_receipt = KdeEvidenceReceipt::from(&receipt);
        kde_receipt.sequence = seq;

        chain
            .append(receipt)
            .map_err(|e| KdeRendererError::Internal(format!("append: {e}")))?;
        drop(chain);

        Ok(kde_receipt)
    }
}

#[async_trait]
impl KdeEvidenceEmitter for InMemoryKdeEvidenceEmitter {
    async fn emit_surface_allocated(
        &self,
        descriptor: &KdeSurfaceDescriptor,
        actor: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let payload = serde_json::json!({
            "surface_id": descriptor.id.to_string(),
            "zone": descriptor.zone,
            "layer": descriptor.layer,
            "claimed_by": descriptor.claimed_by,
            "actor": actor,
            "created_at": descriptor.created_at.to_rfc3339(),
        });
        self.seal_and_append(KdeRecordType::KdeSurfaceAllocated, payload)
            .await
    }

    async fn emit_surface_released(
        &self,
        descriptor: &KdeSurfaceDescriptor,
        actor: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let payload = serde_json::json!({
            "surface_id": descriptor.id.to_string(),
            "zone": descriptor.zone,
            "actor": actor,
        });
        self.seal_and_append(KdeRecordType::KdeSurfaceReleased, payload)
            .await
    }

    async fn emit_layer_shell_rejected(
        &self,
        claimed_by: &str,
        zone: CompositionZone,
        reason: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let payload = serde_json::json!({
            "claimed_by": claimed_by,
            "zone": zone,
            "reason": reason,
        });
        self.seal_and_append(KdeRecordType::KdeLayerShellRejected, payload)
            .await
    }

    async fn emit_recovery_entered(
        &self,
        receipt: &RecoveryEntryReceipt,
        actor: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let payload = serde_json::json!({
            "entered_at": receipt.entered_at.to_rfc3339(),
            "aios_surfaces_only": receipt.aios_surfaces_only,
            "display_separation": receipt.display_separation,
            "actor": actor,
        });
        self.seal_and_append(KdeRecordType::KdeRecoveryEntered, payload)
            .await
    }

    async fn emit_recovery_exited(
        &self,
        actor: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let payload = serde_json::json!({
            "actor": actor,
        });
        self.seal_and_append(KdeRecordType::KdeRecoveryExited, payload)
            .await
    }

    async fn emit_renderer_degraded(
        &self,
        trigger: DegradedTrigger,
        reason: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let trigger_label = serde_json::to_value(trigger)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "unknown".to_string());
        let payload = serde_json::json!({
            "trigger": trigger_label,
            "reason": reason,
        });
        self.seal_and_append(KdeRecordType::KdeRendererDegraded, payload)
            .await
    }

    async fn emit_kwin_script_verified(
        &self,
        script_id: &str,
        signer_fingerprint: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let payload = serde_json::json!({
            "script_id": script_id,
            "signer_fingerprint": signer_fingerprint,
        });
        self.seal_and_append(KdeRecordType::KdeKwinScriptVerified, payload)
            .await
    }

    async fn emit_kwin_script_rejected(
        &self,
        script_id: &str,
        reason: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let payload = serde_json::json!({
            "script_id": script_id,
            "reason": reason,
        });
        self.seal_and_append(KdeRecordType::KdeKwinScriptRejected, payload)
            .await
    }

    async fn emit_icon_bundle_verified(
        &self,
        theme_id: &str,
        signer_fingerprint: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let payload = serde_json::json!({
            "theme_id": theme_id,
            "signer_fingerprint": signer_fingerprint,
        });
        self.seal_and_append(KdeRecordType::KdeIconBundleVerified, payload)
            .await
    }

    async fn emit_icon_bundle_rejected(
        &self,
        theme_id: &str,
        reason: &str,
    ) -> Result<KdeEvidenceReceipt, KdeRendererError> {
        let payload = serde_json::json!({
            "theme_id": theme_id,
            "reason": reason,
        });
        self.seal_and_append(KdeRecordType::KdeIconBundleRejected, payload)
            .await
    }
}
