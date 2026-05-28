//! System Integration Test Harness.
//!
//! Wires every M1..M18 subsystem (vendor registry, standard registry, CVE feed,
//! bridge registry, control map, composition engine, orchestrator, evidence
//! emitter, and unified record catalogue) behind a shared in-memory evidence
//! emitter so that acceptance-grade system integration tests can exercise
//! realistic cross-subsystem scenarios without spawning real processes.

use std::sync::Arc;

use crate::bridges::ExternalBridgeRegistry;
use crate::composition_engine::CompositionEngine;
use crate::control_map::ControlMapRegistry;
use crate::cve_feed::CveFeedShape;
use crate::error::IntegrationError;
use crate::evidence::InMemoryIntegrationEvidenceEmitter;
use crate::evidence::IntegrationEvidenceEmitter;
use crate::orchestrator::Orchestrator;
use crate::record_catalogue::{default_index_entries, UnifiedRecordCatalogue};
use crate::standard_registry::ExternalStandardRegistry;
use crate::vendor_registry::VendorIntegrationRegistry;
use ed25519_dalek::VerifyingKey;

/// Pre-built system integration test harness wiring all 9 L10 subsystems
/// behind a single shared `InMemoryIntegrationEvidenceEmitter`.
///
/// Every subsystem that can emit evidence shares the same emitter, so
/// acceptance-grade tests can inspect the full cross-subsystem evidence
/// chain in one place.
#[allow(clippy::module_name_repetitions)]
pub struct SystemIntegrationHarness {
    vendor_registry: Arc<VendorIntegrationRegistry>,
    standard_registry: Arc<ExternalStandardRegistry>,
    cve_feed: Arc<CveFeedShape>,
    bridge_registry: Arc<ExternalBridgeRegistry>,
    control_map: Arc<ControlMapRegistry>,
    #[allow(dead_code)]
    composition_engine: Arc<CompositionEngine>,
    orchestrator: Arc<Orchestrator>,
    evidence_emitter: Arc<InMemoryIntegrationEvidenceEmitter>,
    evidence_catalogue: Arc<UnifiedRecordCatalogue>,
}

impl SystemIntegrationHarness {
    /// Bootstraps a fresh harness with in-memory instances of every subsystem,
    /// a single shared `InMemoryIntegrationEvidenceEmitter`, and the default
    /// 17-crate AIOS composition installed on the orchestrator.
    #[allow(
        clippy::expect_used,
        clippy::missing_panics_doc,
        clippy::too_many_lines
    )]
    #[must_use]
    pub fn new() -> Self {
        // ── evidence emitter ──────────────────────────────────────────────
        let evidence_emitter = Arc::new(InMemoryIntegrationEvidenceEmitter::new(
            "_system:service:integration-manager",
        ));
        let emitter_dyn: Arc<dyn IntegrationEvidenceEmitter> = evidence_emitter.clone();

        // ── registries (all wired to the shared emitter) ──────────────────
        let vendor_registry =
            Arc::new(VendorIntegrationRegistry::new().with_emitter(emitter_dyn.clone()));
        let standard_registry =
            Arc::new(ExternalStandardRegistry::new().with_emitter(emitter_dyn.clone()));
        let cve_feed = Arc::new(CveFeedShape::new().with_emitter(emitter_dyn.clone()));
        let bridge_registry =
            Arc::new(ExternalBridgeRegistry::new().with_emitter(emitter_dyn.clone()));
        let control_map = Arc::new(ControlMapRegistry::new().with_emitter(emitter_dyn.clone()));

        // ── composition engine + orchestrator ─────────────────────────────
        let composition_engine = Arc::new(CompositionEngine::new());
        let orchestrator = Arc::new(
            Orchestrator::from_default_composition()
                .expect("default AIOS composition must be valid"),
        );

        // ── unified record catalogue ──────────────────────────────────────
        let mut catalogue = UnifiedRecordCatalogue::new();
        for entry in default_index_entries() {
            catalogue
                .register(entry)
                .expect("default index entries must not collide");
        }
        let evidence_catalogue = Arc::new(catalogue);

        Self {
            vendor_registry,
            standard_registry,
            cve_feed,
            bridge_registry,
            control_map,
            composition_engine,
            orchestrator,
            evidence_emitter,
            evidence_catalogue,
        }
    }

    // ── authority registration (mut-before-clone) ────────────────────────

    /// Register a vendor signing authority in the vendor registry.
    ///
    /// # Panics
    ///
    /// Panics if the vendor registry `Arc` has already been shared
    /// (i.e. `vendor()` has been called).
    #[allow(clippy::expect_used, clippy::missing_panics_doc)]
    pub fn register_vendor_authority(&mut self, fingerprint: &str, key: VerifyingKey) {
        Arc::get_mut(&mut self.vendor_registry)
            .expect("vendor registry Arc already shared")
            .register_authority(fingerprint, key);
    }

    // ── subsystem accessors ──────────────────────────────────────────────

    /// Returns the topological boot order from the orchestrator.
    #[allow(clippy::unused_async)]
    pub async fn boot_topological_order(&self) -> Vec<String> {
        self.orchestrator.boot_order().await
    }

    /// Returns a clone of the vendor-registry `Arc`.
    #[allow(clippy::unused_async)]
    pub async fn vendor(&self) -> Arc<VendorIntegrationRegistry> {
        Arc::clone(&self.vendor_registry)
    }

    /// Returns a clone of the standard-registry `Arc`.
    #[allow(clippy::unused_async)]
    pub async fn standards(&self) -> Arc<ExternalStandardRegistry> {
        Arc::clone(&self.standard_registry)
    }

    /// Returns a clone of the CVE feed `Arc`.
    #[allow(clippy::unused_async)]
    pub async fn cve(&self) -> Arc<CveFeedShape> {
        Arc::clone(&self.cve_feed)
    }

    /// Returns a clone of the bridge-registry `Arc`.
    #[allow(clippy::unused_async)]
    pub async fn bridge(&self) -> Arc<ExternalBridgeRegistry> {
        Arc::clone(&self.bridge_registry)
    }

    /// Returns a clone of the control-map `Arc`.
    #[allow(clippy::unused_async)]
    pub async fn control_map(&self) -> Arc<ControlMapRegistry> {
        Arc::clone(&self.control_map)
    }

    /// Returns a clone of the unified record catalogue `Arc`.
    #[allow(clippy::unused_async)]
    pub async fn catalogue(&self) -> Arc<UnifiedRecordCatalogue> {
        Arc::clone(&self.evidence_catalogue)
    }

    // ── evidence chain inspection ────────────────────────────────────────

    /// Returns a reference to the shared evidence emitter for test inspection.
    #[must_use]
    pub fn evidence_emitter(&self) -> &InMemoryIntegrationEvidenceEmitter {
        &self.evidence_emitter
    }

    /// Returns the current sequence count on the shared evidence emitter.
    #[allow(clippy::unused_async)]
    pub async fn evidence_chain_length(&self) -> u64 {
        self.evidence_emitter.receipt_count().await as u64
    }

    /// Validates the full hash-chain of emitted evidence records.
    ///
    /// Delegates to `ReceiptChain::verify_integrity()` which walks every
    /// receipt and asserts each `prev_hash` matches the prior receipt's hash.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the chain integrity check fails.
    #[allow(clippy::unused_async)]
    pub async fn validate_evidence_chain(&self) -> Result<(), IntegrationError> {
        self.evidence_emitter.verify_chain().await
    }
}

impl Default for SystemIntegrationHarness {
    fn default() -> Self {
        Self::new()
    }
}
