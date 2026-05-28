//! Unified Record Catalogue — canonical index of every `RecordType` that the
//! AIOS evidence system can emit, keyed by wire name with ownership, retention,
//! and description metadata.
//!
//! The catalogue is pre-populated with 50+ default entries spanning all M1..M17
//! crates and serves as the single source of truth for record-type discovery,
//! audit trail completeness checks, and retention-policy enforcement.

use std::collections::HashMap;

use aios_evidence::{RecordType, RetentionClass};

// ---------------------------------------------------------------------------
// RecordTypeOwnership — 18 closed variants (M1..M17 + Reserved)
// ---------------------------------------------------------------------------

/// Identifies which AIOS crate or subsystem owns (produces) a given
/// `RecordType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordTypeOwnership {
    /// M1 — Action Runtime (aios-action).
    AiosAction,
    /// M2 — Evidence Log (aios-evidence).
    AiosEvidence,
    /// M3 — Policy Kernel (aios-policy).
    AiosPolicy,
    /// M4 — Service Graph Runtime (aios-sgr).
    AiosSgr,
    /// M5 — Identity & Groups (aios-identity).
    AiosIdentity,
    /// M6 — Semantic Filesystem (aios-fs).
    AiosFs,
    /// M7 — Network Policy (aios-network).
    AiosNetwork,
    /// M8 — Hardware Graph (aios-hardware).
    AiosHardware,
    /// M9 — Kernel Pipeline (aios-kernel).
    AiosKernel,
    /// M10 — Cognitive Core (aios-cognitive).
    AiosCognitive,
    /// M11 — Renderers (KDE/Web/CLI/Voice/Mobile).
    AiosRenderer,
    /// M12 — Compatibility Runtimes (Wine/Waydroid/KVM).
    AiosCompat,
    /// M13 — Repository Model (aios-repo).
    AiosRepo,
    /// M14 — Marketplace & Publisher Onboarding.
    AiosMarketplace,
    /// M15 — Observability & Telemetry (aios-observability).
    AiosObservability,
    /// M16 — System Integration Layer (aios-integration).
    AiosIntegration,
    /// M17 — Distribution & Release (aios-distribution).
    AiosDistribution,
    /// Reserved for future Wave 14+ crate allocations (IDs 1000..=9999).
    Reserved,
}

impl RecordTypeOwnership {
    /// Canonical wire name for this ownership variant.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AiosAction => "AIOS_ACTION",
            Self::AiosEvidence => "AIOS_EVIDENCE",
            Self::AiosPolicy => "AIOS_POLICY",
            Self::AiosSgr => "AIOS_SGR",
            Self::AiosIdentity => "AIOS_IDENTITY",
            Self::AiosFs => "AIOS_FS",
            Self::AiosNetwork => "AIOS_NETWORK",
            Self::AiosHardware => "AIOS_HARDWARE",
            Self::AiosKernel => "AIOS_KERNEL",
            Self::AiosCognitive => "AIOS_COGNITIVE",
            Self::AiosRenderer => "AIOS_RENDERER",
            Self::AiosCompat => "AIOS_COMPAT",
            Self::AiosRepo => "AIOS_REPO",
            Self::AiosMarketplace => "AIOS_MARKETPLACE",
            Self::AiosObservability => "AIOS_OBSERVABILITY",
            Self::AiosIntegration => "AIOS_INTEGRATION",
            Self::AiosDistribution => "AIOS_DISTRIBUTION",
            Self::Reserved => "RESERVED",
        }
    }
}

// ---------------------------------------------------------------------------
// CatalogueEntry
// ---------------------------------------------------------------------------

/// A single entry in the unified record catalogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogueEntry {
    /// Which crate owns this record type.
    pub ownership: RecordTypeOwnership,
    /// The canonical `aios_evidence::RecordType` variant.
    pub record_type: RecordType,
    /// Retention class for this record.
    pub retention: RetentionClass,
    /// Human-readable description of what this record type documents.
    pub description: &'static str,
}

// ---------------------------------------------------------------------------
// UnifiedRecordCatalogue
// ---------------------------------------------------------------------------

/// In-memory catalogue of every known `RecordType`, keyed by its wire name.
///
/// Registries pre-populate this at startup via [`default_index_entries`] and
/// can add custom entries for crate-local record types.
#[derive(Debug, Default)]
pub struct UnifiedRecordCatalogue {
    entries: HashMap<String, CatalogueEntry>,
}

impl UnifiedRecordCatalogue {
    /// Create an empty catalogue.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Register a `CatalogueEntry` keyed by the record type's wire name.
    ///
    /// # Errors
    ///
    /// Returns `Err` with a message if an entry with the same wire name already
    /// exists.
    pub fn register(&mut self, entry: CatalogueEntry) -> Result<(), String> {
        let key = entry.record_type.as_wire_str().to_string();
        if self.entries.contains_key(&key) {
            return Err(format!("duplicate catalogue entry: {key}"));
        }
        self.entries.insert(key, entry);
        Ok(())
    }

    /// Look up a catalogue entry by its wire name.
    #[must_use]
    pub fn get(&self, wire_name: &str) -> Option<&CatalogueEntry> {
        self.entries.get(wire_name)
    }

    /// Return all registered catalogue entries.
    #[must_use]
    pub fn list(&self) -> Vec<&CatalogueEntry> {
        self.entries.values().collect()
    }

    /// Return entries owned by the given [`RecordTypeOwnership`] variant.
    #[must_use]
    pub fn list_by_owner(&self, owner: RecordTypeOwnership) -> Vec<&CatalogueEntry> {
        self.entries
            .values()
            .filter(|e| e.ownership == owner)
            .collect()
    }

    /// Return entries with the given [`RetentionClass`].
    #[must_use]
    pub fn list_by_retention(&self, retention: RetentionClass) -> Vec<&CatalogueEntry> {
        self.entries
            .values()
            .filter(|e| e.retention == retention)
            .collect()
    }

    /// Return all entries classified as FOREVER retention.
    #[must_use]
    pub fn list_forever_records(&self) -> Vec<&CatalogueEntry> {
        self.list_by_retention(RetentionClass::Forever)
    }

    /// Number of registered entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the catalogue is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// default_index_entries — 50+ canonical entries spanning M1..M17
// ---------------------------------------------------------------------------

/// Returns the canonical set of 50+ `CatalogueEntry` values covering all
/// M1..M17 crates. Callers should register these into a
/// [`UnifiedRecordCatalogue`] at startup.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn default_index_entries() -> Vec<CatalogueEntry> {
    vec![
        // ─── M1: aios-action (Action Runtime) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::ActionReceived,
            retention: RetentionClass::Standard24M,
            description: "A typed action was received by the capability runtime",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::TranslationCreated,
            retention: RetentionClass::Standard24M,
            description: "A natural-language intent was translated into a typed action",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::RoutingDecision,
            retention: RetentionClass::Standard24M,
            description: "The router selected an adapter for an action",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::ActionValidated,
            retention: RetentionClass::Standard24M,
            description: "An action passed pre-execution validation",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::ActionPolicyDecision,
            retention: RetentionClass::Standard24M,
            description: "Policy kernel rendered a decision for an action",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::ActionDispatched,
            retention: RetentionClass::Standard24M,
            description: "An action was dispatched to its adapter for execution",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::ExecutionStarted,
            retention: RetentionClass::Standard24M,
            description: "An adapter began executing a dispatched action",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::ExecutionCompleted,
            retention: RetentionClass::Standard24M,
            description: "An adapter completed execution of an action",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::ExecutionSucceeded,
            retention: RetentionClass::Standard24M,
            description: "Post-execution verification confirmed success",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::ExecutionFailed,
            retention: RetentionClass::Standard24M,
            description: "Post-execution verification detected failure",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::RollbackCompleted,
            retention: RetentionClass::Extended60M,
            description: "Rollback of a failed action completed successfully",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosAction,
            record_type: RecordType::StatusTransition,
            retention: RetentionClass::Standard24M,
            description: "An action progressed through its lifecycle state machine",
        },
        // ─── M2: aios-evidence (Evidence Log) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosEvidence,
            record_type: RecordType::ChainCheckpoint,
            retention: RetentionClass::Forever,
            description: "A hash-chain checkpoint was sealed in the evidence log",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosEvidence,
            record_type: RecordType::GcPass,
            retention: RetentionClass::Standard24M,
            description: "A garbage-collection pass completed on the evidence log",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosEvidence,
            record_type: RecordType::SegmentSealed,
            retention: RetentionClass::Extended60M,
            description: "An evidence log segment was sealed and made immutable",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosEvidence,
            record_type: RecordType::ChainInconsistencyDetected,
            retention: RetentionClass::Forever,
            description: "A hash-chain consistency violation was detected",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosEvidence,
            record_type: RecordType::TamperDetected,
            retention: RetentionClass::Forever,
            description: "Tampering with the evidence log was detected",
        },
        // ─── M3: aios-policy (Policy Kernel) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosPolicy,
            record_type: RecordType::PolicyDecision,
            retention: RetentionClass::Standard24M,
            description: "The policy kernel evaluated a request and produced a decision",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosPolicy,
            record_type: RecordType::PolicyBundleLoad,
            retention: RetentionClass::Extended60M,
            description: "A policy bundle was loaded or reloaded",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosPolicy,
            record_type: RecordType::ApprovalRequested,
            retention: RetentionClass::Standard24M,
            description: "An approval was requested for a policy-gated operation",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosPolicy,
            record_type: RecordType::ApprovalGranted,
            retention: RetentionClass::Extended60M,
            description: "An approval was granted for a policy-gated operation",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosPolicy,
            record_type: RecordType::ApprovalDenied,
            retention: RetentionClass::Forever,
            description: "An approval request was denied by the policy kernel",
        },
        // ─── M4: aios-sgr (Service Graph Runtime) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosSgr,
            record_type: RecordType::AdapterRegistered,
            retention: RetentionClass::Standard24M,
            description: "A capability adapter was registered with the SGR",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosSgr,
            record_type: RecordType::AdapterDeregistered,
            retention: RetentionClass::Standard24M,
            description: "A capability adapter was deregistered from the SGR",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosSgr,
            record_type: RecordType::UnitStarted,
            retention: RetentionClass::Standard24M,
            description: "A systemd unit was started by the SGR",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosSgr,
            record_type: RecordType::UnitFailed,
            retention: RetentionClass::Standard24M,
            description: "A systemd unit reported a failure state",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosSgr,
            record_type: RecordType::TransitionSucceeded,
            retention: RetentionClass::Standard24M,
            description: "A desired-state graph transition succeeded",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosSgr,
            record_type: RecordType::TransitionFailed,
            retention: RetentionClass::Standard24M,
            description: "A desired-state graph transition failed",
        },
        // ─── M5: aios-identity (Identity & Groups) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosIdentity,
            record_type: RecordType::IdentityBundleLoaded,
            retention: RetentionClass::Standard24M,
            description: "An identity bundle was loaded into the identity subsystem",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosIdentity,
            record_type: RecordType::GroupRegistered,
            retention: RetentionClass::Standard24M,
            description: "A new identity group was registered",
        },
        // ─── M6: aios-fs (Semantic Filesystem) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosFs,
            record_type: RecordType::QuarantineEvent,
            retention: RetentionClass::Forever,
            description: "A filesystem object was placed in quarantine",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosFs,
            record_type: RecordType::ConflictEvent,
            retention: RetentionClass::Extended60M,
            description: "A concurrent modification conflict was detected in AIOS-FS",
        },
        // ─── M7: aios-network (Network Policy) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosNetwork,
            record_type: RecordType::NetworkPostureChanged,
            retention: RetentionClass::Extended60M,
            description: "The network posture (e.g. LAN-only, VPN, public) changed",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosNetwork,
            record_type: RecordType::ExposureGranted,
            retention: RetentionClass::Extended60M,
            description: "A service exposure (LAN or public) was granted",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosNetwork,
            record_type: RecordType::ExposureRevoked,
            retention: RetentionClass::Extended60M,
            description: "A service exposure was revoked",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosNetwork,
            record_type: RecordType::RawSocketBypassAttempted,
            retention: RetentionClass::Forever,
            description: "An attempt to bypass the network policy via raw sockets was blocked",
        },
        // ─── M8: aios-hardware (Hardware Graph) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosHardware,
            record_type: RecordType::HardwareGraphRebuilt,
            retention: RetentionClass::Standard24M,
            description: "The hardware graph was rebuilt from sysfs/devicetree",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosHardware,
            record_type: RecordType::DeviceDetected,
            retention: RetentionClass::Standard24M,
            description: "A new hardware device was detected",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosHardware,
            record_type: RecordType::DeviceQuarantined,
            retention: RetentionClass::Forever,
            description: "A hardware device was quarantined (driver blocked)",
        },
        // ─── M9: aios-kernel (Kernel Pipeline) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosKernel,
            record_type: RecordType::KernelBuildCompleted,
            retention: RetentionClass::Standard24M,
            description: "A kernel build pipeline stage completed",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosKernel,
            record_type: RecordType::KernelConverged,
            retention: RetentionClass::Standard24M,
            description: "An A/B kernel slot pair converged to identical images",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosKernel,
            record_type: RecordType::KernelDivergedRegression,
            retention: RetentionClass::Forever,
            description: "An A/B kernel slot diverged, indicating a regression",
        },
        // ─── M10: aios-cognitive (Cognitive Core) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosCognitive,
            record_type: RecordType::ModelCall,
            retention: RetentionClass::Standard24M,
            description: "A model invocation was recorded (legacy record type)",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosCognitive,
            record_type: RecordType::AgentRegistered,
            retention: RetentionClass::Standard24M,
            description: "A cognitive agent was registered with the agent runtime",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosCognitive,
            record_type: RecordType::AgentProposalEmitted,
            retention: RetentionClass::Standard24M,
            description: "An agent emitted a typed action proposal",
        },
        // ─── M11: aios-renderer (Renderers) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosRenderer,
            record_type: RecordType::SurfaceCreated,
            retention: RetentionClass::Standard24M,
            description: "A UI surface was created by a renderer",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosRenderer,
            record_type: RecordType::KdeRendererStarted,
            retention: RetentionClass::Standard24M,
            description: "The KDE Plasma renderer was started",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosRenderer,
            record_type: RecordType::WebRendererStarted,
            retention: RetentionClass::Standard24M,
            description: "The Web renderer was started",
        },
        // ─── M12: aios-compat (Compatibility Runtimes) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosCompat,
            record_type: RecordType::AppLaunchStarted,
            retention: RetentionClass::Standard24M,
            description: "An application launch was initiated by the compatibility runtime",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosCompat,
            record_type: RecordType::AppLaunchSucceeded,
            retention: RetentionClass::Standard24M,
            description: "An application launch completed successfully",
        },
        // ─── M13: aios-repo (Repository Model) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosRepo,
            record_type: RecordType::PackageFetchStarted,
            retention: RetentionClass::Standard24M,
            description: "A package fetch was initiated from the repository",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosRepo,
            record_type: RecordType::PackageVerified,
            retention: RetentionClass::Standard24M,
            description: "A package passed cryptographic signature verification",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosRepo,
            record_type: RecordType::PackageInstalled,
            retention: RetentionClass::Standard24M,
            description: "A package was successfully installed",
        },
        // ─── M14: aios-marketplace (Marketplace) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosMarketplace,
            record_type: RecordType::PublisherOnboardingApproved,
            retention: RetentionClass::Extended60M,
            description: "A publisher onboarding application was approved",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosMarketplace,
            record_type: RecordType::ListingPublished,
            retention: RetentionClass::Standard24M,
            description: "A marketplace listing was published",
        },
        // ─── M15: aios-observability (Observability & Telemetry) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosObservability,
            record_type: RecordType::TelemetryPipelineStarted,
            retention: RetentionClass::Standard24M,
            description: "The telemetry pipeline was started",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosObservability,
            record_type: RecordType::FailureObserved,
            retention: RetentionClass::Standard24M,
            description: "A failure was observed and recorded by the telemetry system",
        },
        // ─── M16: aios-integration (System Integration) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosIntegration,
            record_type: RecordType::ExternalBridgePackageAdmitted,
            retention: RetentionClass::Standard24M,
            description: "An external bridge package was admitted into the catalog",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosIntegration,
            record_type: RecordType::BridgeFetchStarted,
            retention: RetentionClass::Standard24M,
            description: "A bridge fetch operation was started",
        },
        // ─── M17: aios-distribution (Distribution) ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosDistribution,
            record_type: RecordType::FirstBootStarted,
            retention: RetentionClass::Extended60M,
            description: "The first-boot flow was initiated",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosDistribution,
            record_type: RecordType::FirstBootComplete,
            retention: RetentionClass::Forever,
            description: "The first-boot flow completed successfully",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosDistribution,
            record_type: RecordType::RecoveryEvent,
            retention: RetentionClass::Forever,
            description: "A recovery event was triggered by the distribution layer",
        },
        // ─── Cross-cutting / shared ───
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosEvidence,
            record_type: RecordType::EmergencyOverrideGrant,
            retention: RetentionClass::Forever,
            description: "An emergency override was granted (break-glass)",
        },
        CatalogueEntry {
            ownership: RecordTypeOwnership::AiosEvidence,
            record_type: RecordType::CompactionApprovalRequired,
            retention: RetentionClass::Extended60M,
            description: "Evidence compaction requires explicit operator approval",
        },
    ]
}
