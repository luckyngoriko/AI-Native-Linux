use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::IntegrationError;
use crate::evidence::IntegrationEvidenceEmitter;
use crate::vendor::{VendorIntegrationContract, VendorTrustClass};

/// Closed taxonomy of external package bridge kinds.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BridgeKind {
    /// Flathub (flatpak) bridge.
    Flathub,
    /// OCI container registry bridge.
    OciRegistry {
        /// Registry host, e.g. "docker.io" or "ghcr.io".
        registry_host: String,
    },
    /// Debian/Ubuntu APT repository bridge.
    Apt {
        /// Distribution codename, e.g. "debian" or "ubuntu".
        distro: String,
    },
    /// Fedora/RHEL DNF repository bridge.
    Dnf {
        /// Distribution, e.g. "fedora" or "rhel".
        distro: String,
    },
    /// Arch Linux pacman repository bridge.
    Pacman {
        /// Distribution, e.g. "arch" or "manjaro".
        distro: String,
    },
}

impl BridgeKind {
    /// Returns the canonical label for this bridge kind (used for filtering).
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Flathub => "Flathub",
            Self::OciRegistry { .. } => "OciRegistry",
            Self::Apt { .. } => "Apt",
            Self::Dnf { .. } => "Dnf",
            Self::Pacman { .. } => "Pacman",
        }
    }
}

/// How capabilities are extracted from the source manifest.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapabilityExtractorRule {
    /// Flatpak finishes block + plugs/slots.
    FlatpakFinishesSection,
    /// OCI image manifest annotations.
    OciAnnotations,
    /// Debian `Depends:` + `Recommends:` parsed from control file.
    DebianControl,
    /// RPM `Requires:` / `BuildRequires:` from spec file.
    RpmSpec,
    /// PKGBUILD `depends=()` array.
    PkgbuildArray,
    /// Operator must hand-author the capability set.
    OperatorAuthored,
}

/// Manifest translation rules for an external bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestTranslationRules {
    /// The source manifest format recognised by this bridge.
    pub source_manifest_format: String,
    /// How capabilities are extracted from the manifest.
    pub capability_extractor: CapabilityExtractorRule,
    /// Lowest trust class admissible from this bridge.
    pub trust_floor: VendorTrustClass,
}

/// A typed bridge contract wrapping a vendor integration contract with
/// manifest-translation rules and a bridge kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeContract {
    /// Unique bridge identifier.
    pub bridge_id: String,
    /// The kind of external bridge.
    pub kind: BridgeKind,
    /// The underlying vendor integration contract (T-176).
    pub vendor_contract: VendorIntegrationContract,
    /// Manifest translation rules for this bridge.
    pub translation_rules: ManifestTranslationRules,
    /// When the bridge was admitted.
    pub admitted_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Default translation-rule constructors
// ---------------------------------------------------------------------------

/// Default Flathub translation rules: `FlatpakFinishesSection` extractor,
/// `CommunityVerified` trust floor.
#[must_use]
pub fn default_flathub_contract() -> ManifestTranslationRules {
    ManifestTranslationRules {
        source_manifest_format: "flatpak-manifest.json".into(),
        capability_extractor: CapabilityExtractorRule::FlatpakFinishesSection,
        trust_floor: VendorTrustClass::CommunityVerified,
    }
}

/// Default OCI translation rules: `OciAnnotations` extractor,
/// `CommunityVerified` trust floor.
#[must_use]
pub fn default_oci_contract() -> ManifestTranslationRules {
    ManifestTranslationRules {
        source_manifest_format: "OCI image manifest v1".into(),
        capability_extractor: CapabilityExtractorRule::OciAnnotations,
        trust_floor: VendorTrustClass::CommunityVerified,
    }
}

/// Default APT translation rules: `DebianControl` extractor,
/// `CommunityVerified` trust floor.
#[must_use]
pub fn default_apt_contract() -> ManifestTranslationRules {
    ManifestTranslationRules {
        source_manifest_format: "control".into(),
        capability_extractor: CapabilityExtractorRule::DebianControl,
        trust_floor: VendorTrustClass::CommunityVerified,
    }
}

/// Default DNF translation rules: `RpmSpec` extractor,
/// `CommunityVerified` trust floor.
#[must_use]
pub fn default_dnf_contract() -> ManifestTranslationRules {
    ManifestTranslationRules {
        source_manifest_format: "spec".into(),
        capability_extractor: CapabilityExtractorRule::RpmSpec,
        trust_floor: VendorTrustClass::CommunityVerified,
    }
}

/// Default pacman translation rules: `PkgbuildArray` extractor,
/// `OperatorAuthorised` trust floor (AUR is not formally moderated).
#[must_use]
pub fn default_pacman_contract() -> ManifestTranslationRules {
    ManifestTranslationRules {
        source_manifest_format: "PKGBUILD".into(),
        capability_extractor: CapabilityExtractorRule::PkgbuildArray,
        trust_floor: VendorTrustClass::OperatorAuthorised,
    }
}

// ---------------------------------------------------------------------------
// ExternalBridgeRegistry
// ---------------------------------------------------------------------------

fn lock_poisoned() -> IntegrationError {
    IntegrationError::Internal("lock poisoned".into())
}

/// Registry of admitted external bridges keyed by bridge id.
pub struct ExternalBridgeRegistry {
    bridges: RwLock<HashMap<String, BridgeContract>>,
    emitter: Option<Arc<dyn IntegrationEvidenceEmitter>>,
}

impl ExternalBridgeRegistry {
    /// Creates an empty bridge registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bridges: RwLock::new(HashMap::new()),
            emitter: None,
        }
    }

    /// Attach an optional [`IntegrationEvidenceEmitter`] for chain-of-custody
    /// evidence emission.
    #[must_use]
    pub fn with_emitter(mut self, emitter: Arc<dyn IntegrationEvidenceEmitter>) -> Self {
        self.emitter = Some(emitter);
        self
    }

    /// Admits a bridge contract.
    ///
    /// # Errors
    ///
    /// Returns `VendorBlacklisted` if the underlying vendor contract has trust
    /// class `BlacklistedDoNotAdmit`.
    ///
    /// Returns `Internal` if a bridge with the same `bridge_id` already exists
    /// or a lock is poisoned.
    #[allow(clippy::unused_async)]
    pub async fn admit_bridge(&self, bridge: BridgeContract) -> Result<(), IntegrationError> {
        let contract_id = bridge.vendor_contract.contract_id.clone();
        if bridge.vendor_contract.trust_class == VendorTrustClass::BlacklistedDoNotAdmit {
            return Err(IntegrationError::VendorBlacklisted { contract_id });
        }

        let bridge_id = {
            let mut bridges = self.bridges.write().map_err(|_| lock_poisoned())?;
            if bridges.contains_key(&bridge.bridge_id) {
                return Err(IntegrationError::Internal(
                    "bridge_id already exists".into(),
                ));
            }
            let bridge_id = bridge.bridge_id.clone();
            bridges.insert(bridge_id.clone(), bridge);
            bridge_id
        };

        if let Some(ref emitter) = self.emitter {
            let admitted = {
                let bridges = self.bridges.read().map_err(|_| lock_poisoned())?;
                bridges.get(&bridge_id).cloned()
            };
            if let Some(b) = admitted {
                let _ = emitter.emit_bridge_admitted(&b).await;
            }
        }

        Ok(())
    }

    /// Returns the bridge contract for the given id, if any.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn get_bridge(&self, bridge_id: &str) -> Option<BridgeContract> {
        let bridges = self.bridges.read().ok()?;
        bridges.get(bridge_id).cloned()
    }

    /// Lists all admitted bridge contracts.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn list_bridges(&self) -> Vec<BridgeContract> {
        let bridges = self.bridges.read().ok();
        bridges
            .map(|b| b.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Lists admitted bridges filtered by kind label.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn list_by_kind_label(&self, label: &str) -> Vec<BridgeContract> {
        let bridges = self.bridges.read().ok();
        bridges
            .map(|b| {
                b.values()
                    .filter(|bc| bc.kind.label() == label)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Revokes (removes) a bridge by id.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the bridge is unknown or a lock is poisoned.
    #[allow(clippy::unused_async)]
    pub async fn revoke_bridge(
        &self,
        bridge_id: &str,
        _reason: &str,
    ) -> Result<(), IntegrationError> {
        let mut bridges = self.bridges.write().map_err(|_| lock_poisoned())?;
        if bridges.remove(bridge_id).is_none() {
            return Err(IntegrationError::Internal("unknown bridge_id".into()));
        }
        drop(bridges);
        Ok(())
    }
}

impl Default for ExternalBridgeRegistry {
    fn default() -> Self {
        Self {
            bridges: RwLock::new(HashMap::new()),
            emitter: None,
        }
    }
}
