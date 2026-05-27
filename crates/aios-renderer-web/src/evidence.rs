//! L7 Web Renderer Evidence Emitter (S7.5 ↔ S3.1) — typed lifecycle event
//! emission into the append-only Evidence Log.
//!
//! Every surface allocate/release, exposure transition, exposure granted,
//! LAN exposure heartbeat, renderer degraded, extension interference, and
//! icon bundle verify/reject produces a chained evidence receipt. INV-015:
//! NO secret material, NO raw key bytes in any payload.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use aios_evidence::{ReceiptBuilder, ReceiptChain, RecordType, RetentionClass};

use crate::error::WebRendererError;
use crate::exposure::ExposureLevel;
use crate::types::WebSurfaceDescriptor;

// ---------------------------------------------------------------------------
// WebRecordType — 9 renderer-side lifecycle event discriminators
// ---------------------------------------------------------------------------

/// Closed set of L7 Web renderer lifecycle event types.
///
/// These map to the nearest `aios_evidence::RecordType` variant at emission
/// time. The mapping is stable and one-directional.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WebRecordType {
    /// Surface allocated in the Web renderer.
    WebSurfaceAllocated,
    /// Surface released from the Web renderer.
    WebSurfaceReleased,
    /// Exposure level transition (covers both granted and revoked paths).
    WebExposureTransition,
    /// LAN/public exposure explicitly granted (FOREVER retention).
    WebExposureGranted,
    /// LAN exposure is active with heartbeat monitoring.
    WebLanExposureActive,
    /// Renderer entered degraded mode.
    WebRendererDegraded,
    /// Extension interference detected in chrome shadow root (INV I10).
    WebExtensionInterference,
    /// Constitutional icon bundle verified.
    WebIconBundleVerified,
    /// Constitutional icon bundle rejected.
    WebIconBundleRejected,
}

impl WebRecordType {
    /// Map to the evidence crate's closed `RecordType`.
    const fn to_evidence_record_type(self) -> RecordType {
        match self {
            Self::WebSurfaceAllocated => RecordType::SurfaceCreated,
            Self::WebSurfaceReleased => RecordType::SurfaceDestroyed,
            Self::WebExposureTransition => RecordType::WebExposureGranted,
            Self::WebExposureGranted => RecordType::WebLanExposureGranted,
            Self::WebLanExposureActive => RecordType::WebLanExposureActive,
            Self::WebRendererDegraded => RecordType::WebRendererDegraded,
            Self::WebExtensionInterference => RecordType::WebExtensionInterference,
            Self::WebIconBundleVerified => RecordType::ThemeLoaded,
            Self::WebIconBundleRejected => RecordType::ThemeRejected,
        }
    }

    /// Retention class for this event type.
    ///
    /// `WebExposureGranted` is FOREVER per spec. All other events use the
    /// underlying `RecordType`'s native retention class.
    const fn retention_class(self) -> RetentionClass {
        match self {
            Self::WebExposureGranted => RetentionClass::Forever,
            _ => self.to_evidence_record_type().retention_class(),
        }
    }

    /// Wire-name string for this discriminator.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WebSurfaceAllocated => "WEB_SURFACE_ALLOCATED",
            Self::WebSurfaceReleased => "WEB_SURFACE_RELEASED",
            Self::WebExposureTransition => "WEB_EXPOSURE_TRANSITION",
            Self::WebExposureGranted => "WEB_EXPOSURE_GRANTED",
            Self::WebLanExposureActive => "WEB_LAN_EXPOSURE_ACTIVE",
            Self::WebRendererDegraded => "WEB_RENDERER_DEGRADED",
            Self::WebExtensionInterference => "WEB_EXTENSION_INTERFERENCE",
            Self::WebIconBundleVerified => "WEB_ICON_BUNDLE_VERIFIED",
            Self::WebIconBundleRejected => "WEB_ICON_BUNDLE_REJECTED",
        }
    }
}

// ---------------------------------------------------------------------------
// WebEvidenceReceipt — simplified renderer-side receipt view
// ---------------------------------------------------------------------------

/// Simplified evidence receipt returned to L7 callers.
///
/// Carries the record identity, content hash, and chain sequence number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebEvidenceReceipt {
    /// Evidence receipt id (`evr_<ULID>`).
    pub record_id: String,
    /// BLAKE3-256 content hash (64 lowercase hex chars).
    pub hash: String,
    /// 0-based sequence position in the emitter's chain.
    pub sequence: u64,
}

impl WebEvidenceReceipt {
    fn from_evidence_receipt(r: &aios_evidence::EvidenceReceipt) -> Self {
        Self {
            record_id: r.receipt_id().as_str().to_owned(),
            hash: r.content_hash().to_owned(),
            sequence: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// WebEvidenceEmitter trait
// ---------------------------------------------------------------------------

/// S7.5 ↔ S3.1 — async contract for emitting Web renderer lifecycle events
/// into the Evidence Log.
///
/// Nine methods cover the full L7 Web event surface. Implementations are
/// optional (`Option<Arc<dyn WebEvidenceEmitter>>`): when `None`, no
/// emission occurs and no error is raised.
#[async_trait]
pub trait WebEvidenceEmitter: Send + Sync {
    /// Emit a `WEB_SURFACE_ALLOCATED` evidence record.
    async fn emit_surface_allocated(
        &self,
        descriptor: &WebSurfaceDescriptor,
        actor: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError>;

    /// Emit a `WEB_SURFACE_RELEASED` evidence record.
    async fn emit_surface_released(
        &self,
        descriptor: &WebSurfaceDescriptor,
        actor: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError>;

    /// Emit a `WEB_EXPOSURE_TRANSITION` evidence record.
    async fn emit_exposure_transition(
        &self,
        from: &str,
        to: &str,
        reason: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError>;

    /// Emit a `WEB_EXPOSURE_GRANTED` evidence record (FOREVER retention).
    async fn emit_exposure_granted(
        &self,
        level: &ExposureLevel,
        decision_id: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError>;

    /// Emit a `WEB_LAN_EXPOSURE_ACTIVE` heartbeat evidence record.
    async fn emit_lan_exposure_active(
        &self,
        level: &ExposureLevel,
    ) -> Result<WebEvidenceReceipt, WebRendererError>;

    /// Emit a `WEB_RENDERER_DEGRADED` evidence record.
    async fn emit_renderer_degraded(
        &self,
        reason: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError>;

    /// Emit a `WEB_EXTENSION_INTERFERENCE` evidence record (INV I10).
    async fn emit_extension_interference(
        &self,
        observed_hash: &str,
        mutation_kind: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError>;

    /// Emit a `WEB_ICON_BUNDLE_VERIFIED` evidence record.
    async fn emit_icon_bundle_verified(
        &self,
        theme_id: &str,
        signer_fingerprint: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError>;

    /// Emit a `WEB_ICON_BUNDLE_REJECTED` evidence record.
    async fn emit_icon_bundle_rejected(
        &self,
        theme_id: &str,
        reason: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError>;
}

// ---------------------------------------------------------------------------
// InMemoryWebEvidenceEmitter
// ---------------------------------------------------------------------------

/// In-memory `WebEvidenceEmitter` backed by a `ReceiptChain`.
///
/// Every emission seals a new receipt, appends it to the chain, and returns
/// a `WebEvidenceReceipt` with the chain sequence number.
pub struct InMemoryWebEvidenceEmitter {
    chain: Arc<RwLock<ReceiptChain>>,
    subject: String,
}

impl InMemoryWebEvidenceEmitter {
    /// Create an emitter that stamps receipts with the given subject
    /// canonical id (e.g. `"service:aios-renderer-web"`).
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
    /// Returns `Internal` wrapping the underlying chain error.
    pub async fn verify_chain(&self) -> Result<(), WebRendererError> {
        self.chain
            .read()
            .await
            .verify_integrity()
            .map_err(|e| WebRendererError::Internal(format!("chain integrity: {e}")))
    }

    /// Shared seal-and-append helper.
    async fn seal_and_append(
        &self,
        web_record_type: WebRecordType,
        payload: serde_json::Value,
    ) -> Result<WebEvidenceReceipt, WebRendererError> {
        let mut chain = self.chain.write().await;
        let prev = chain.receipts().last();
        let seq = chain.len() as u64;

        let builder = ReceiptBuilder::new(
            web_record_type.to_evidence_record_type(),
            web_record_type.retention_class(),
            &self.subject,
        )
        .with_payload(payload);

        let receipt = builder
            .seal(prev)
            .map_err(|e| WebRendererError::Internal(format!("seal: {e}")))?;

        let mut web_receipt = WebEvidenceReceipt::from_evidence_receipt(&receipt);
        web_receipt.sequence = seq;

        chain
            .append(receipt)
            .map_err(|e| WebRendererError::Internal(format!("append: {e}")))?;
        drop(chain);

        Ok(web_receipt)
    }
}

#[async_trait]
impl WebEvidenceEmitter for InMemoryWebEvidenceEmitter {
    async fn emit_surface_allocated(
        &self,
        descriptor: &WebSurfaceDescriptor,
        actor: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError> {
        let payload = serde_json::json!({
            "surface_id": descriptor.id.to_string(),
            "origin": descriptor.origin.full_origin,
            "node_kind": format!("{:?}", descriptor.node_kind),
            "claimed_by": descriptor.claimed_by,
            "actor": actor,
            "created_at": descriptor.created_at.to_rfc3339(),
        });
        self.seal_and_append(WebRecordType::WebSurfaceAllocated, payload)
            .await
    }

    async fn emit_surface_released(
        &self,
        descriptor: &WebSurfaceDescriptor,
        actor: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError> {
        let payload = serde_json::json!({
            "surface_id": descriptor.id.to_string(),
            "origin": descriptor.origin.full_origin,
            "actor": actor,
        });
        self.seal_and_append(WebRecordType::WebSurfaceReleased, payload)
            .await
    }

    async fn emit_exposure_transition(
        &self,
        from: &str,
        to: &str,
        reason: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError> {
        let payload = serde_json::json!({
            "from": from,
            "to": to,
            "reason": reason,
        });
        self.seal_and_append(WebRecordType::WebExposureTransition, payload)
            .await
    }

    async fn emit_exposure_granted(
        &self,
        level: &ExposureLevel,
        decision_id: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError> {
        let payload = serde_json::json!({
            "exposure_level": level.label().to_string(),
            "decision_id": decision_id,
        });
        self.seal_and_append(WebRecordType::WebExposureGranted, payload)
            .await
    }

    async fn emit_lan_exposure_active(
        &self,
        level: &ExposureLevel,
    ) -> Result<WebEvidenceReceipt, WebRendererError> {
        let payload = serde_json::json!({
            "exposure_level": level.label().to_string(),
        });
        self.seal_and_append(WebRecordType::WebLanExposureActive, payload)
            .await
    }

    async fn emit_renderer_degraded(
        &self,
        reason: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError> {
        let payload = serde_json::json!({
            "reason": reason,
        });
        self.seal_and_append(WebRecordType::WebRendererDegraded, payload)
            .await
    }

    async fn emit_extension_interference(
        &self,
        observed_hash: &str,
        mutation_kind: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError> {
        let payload = serde_json::json!({
            "observed_hash": observed_hash,
            "mutation_kind": mutation_kind,
        });
        self.seal_and_append(WebRecordType::WebExtensionInterference, payload)
            .await
    }

    async fn emit_icon_bundle_verified(
        &self,
        theme_id: &str,
        signer_fingerprint: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError> {
        let payload = serde_json::json!({
            "theme_id": theme_id,
            "signer_fingerprint": signer_fingerprint,
        });
        self.seal_and_append(WebRecordType::WebIconBundleVerified, payload)
            .await
    }

    async fn emit_icon_bundle_rejected(
        &self,
        theme_id: &str,
        reason: &str,
    ) -> Result<WebEvidenceReceipt, WebRendererError> {
        let payload = serde_json::json!({
            "theme_id": theme_id,
            "reason": reason,
        });
        self.seal_and_append(WebRecordType::WebIconBundleRejected, payload)
            .await
    }
}
