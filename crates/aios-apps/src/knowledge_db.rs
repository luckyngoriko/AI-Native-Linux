//! S12.4 Compatibility Knowledge — per-app profile database.
//!
//! `CompatibilityKnowledgeDB` stores an [`AppProfile`] per [`PackageId`],
//! supports signed-profile registration with Ed25519 authority verification,
//! and provides a mutation API for known issues, hints, and compatibility
//! score updates.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, SigningKey, VerifyingKey};
use tokio::sync::RwLock;

use crate::app_profile::{AppProfile, CompatibilityRating, EvidenceLevel, RatingDimension};
use crate::ecosystem::{EcosystemHonestyClass, EcosystemRuntime, RecipeTrustClass};
use crate::error::AppsError;
use crate::package::PackageId;

// ---------------------------------------------------------------------------
// KnowledgeDbEntry — internal enriched record
// ---------------------------------------------------------------------------

/// Internal per-package record that wraps the frozen [`AppProfile`] with
/// mutable compatibility metadata the DB can update over time.
#[derive(Clone, Debug)]
struct KnowledgeDbEntry {
    /// The frozen compatibility profile (T-115 shape).
    profile: AppProfile,
    /// Human-readable issue descriptions appended over time.
    known_issues: Vec<String>,
    /// Human-readable hints for achieving higher compatibility.
    hints: Vec<String>,
    /// 0–100 compatibility score (100 = best). Setters clamp at 100.
    compatibility_score: u8,
    /// When this entry was last mutated through the DB API.
    last_updated: DateTime<Utc>,
}

impl KnowledgeDbEntry {
    fn new(profile: AppProfile) -> Self {
        Self {
            profile,
            known_issues: Vec::new(),
            hints: Vec::new(),
            compatibility_score: 50,
            last_updated: Utc::now(),
        }
    }
}

// ---------------------------------------------------------------------------
// AppProfileMutation
// ---------------------------------------------------------------------------

/// A partial mutation to apply to a knowledge-DB entry.
///
/// Each variant changes exactly one aspect; other fields are untouched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppProfileMutation {
    /// Append a human-readable issue description to `known_issues`.
    AddIssue(String),
    /// Append a human-readable hint to `hints`.
    AddHint(String),
    /// Override the compatibility score. Values above 100 are clamped.
    SetCompatibilityScore(u8),
    /// Explicitly mark the last-updated timestamp.
    MarkLastUpdated(DateTime<Utc>),
}

// ---------------------------------------------------------------------------
// CompatibilityKnowledgeDB
// ---------------------------------------------------------------------------

/// S12.4 — per-app compatibility knowledge database.
///
/// Stores an [`AppProfile`] keyed by [`PackageId`] with Ed25519
/// profile-authority verification on registration and a partial-update
/// mutation API for known issues, hints, and compatibility score.
pub struct CompatibilityKnowledgeDB {
    profiles: RwLock<HashMap<PackageId, KnowledgeDbEntry>>,
    profile_authority: HashMap<String, VerifyingKey>,
}

impl CompatibilityKnowledgeDB {
    /// Create an empty DB with the given trusted profile authorities.
    ///
    /// Each entry maps an authority name to its Ed25519 verifying key.
    /// Registration signatures are verified against every key in this set;
    /// if the map is empty every registration is rejected.
    #[must_use]
    pub fn new(profile_authority: HashMap<String, VerifyingKey>) -> Self {
        Self {
            profiles: RwLock::new(HashMap::new()),
            profile_authority,
        }
    }

    /// Create a DB pre-loaded with 5 canonical fixture profiles covering
    /// one representative app per [`EcosystemRuntime`] family, plus a
    /// deterministic self-signed authority so registration tests work
    /// out-of-the-box.
    #[must_use]
    pub fn with_fixtures() -> Self {
        let seed: [u8; 32] = [
            0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
            0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42, 0x42,
            0x42, 0x42, 0x42, 0x42,
        ];
        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = signing_key.verifying_key();

        let mut authority = HashMap::new();
        authority.insert("aios-fixture-authority".to_string(), verifying_key);

        let db = Self::new(authority);

        let now = Utc::now();

        // 1 — LinuxNative: a first-class AIOS app.
        let p1 = AppProfile {
            app_id: "aios-terminal".into(),
            ecosystem_runtime: EcosystemRuntime::RuntimeLinuxNative,
            current_recipe_trust_class: RecipeTrustClass::RecipeAiosCurated,
            headline_rating: CompatibilityRating::Platinum,
            headline_evidence_level: EvidenceLevel::VerifiedPublisher,
            worst_dimension: RatingDimension::LaunchReliability,
            ecosystem_honesty_class: EcosystemHonestyClass::FullySupported,
        };

        // 2 — Flatpak: commonly re-packaged GUI app.
        let p2 = AppProfile {
            app_id: "org.gimp.GIMP".into(),
            ecosystem_runtime: EcosystemRuntime::RuntimeFlatpak,
            current_recipe_trust_class: RecipeTrustClass::RecipeCommunity,
            headline_rating: CompatibilityRating::Gold,
            headline_evidence_level: EvidenceLevel::MultiOperatorCorroborated,
            worst_dimension: RatingDimension::VisualQuality,
            ecosystem_honesty_class: EcosystemHonestyClass::PartiallySupported,
        };

        // 3 — WindowsProton: Steam game.
        let p3 = AppProfile {
            app_id: "valve-hl2".into(),
            ecosystem_runtime: EcosystemRuntime::RuntimeWindowsProton,
            current_recipe_trust_class: RecipeTrustClass::RecipeCommunity,
            headline_rating: CompatibilityRating::Gold,
            headline_evidence_level: EvidenceLevel::SingleOperatorObserved,
            worst_dimension: RatingDimension::InputHandling,
            ecosystem_honesty_class: EcosystemHonestyClass::PartiallySupported,
        };

        // 4 — AndroidWaydroid: mobile app.
        let p4 = AppProfile {
            app_id: "com.signal".into(),
            ecosystem_runtime: EcosystemRuntime::RuntimeAndroidWaydroid,
            current_recipe_trust_class: RecipeTrustClass::RecipeAiosCurated,
            headline_rating: CompatibilityRating::Silver,
            headline_evidence_level: EvidenceLevel::MultiOperatorCorroborated,
            worst_dimension: RatingDimension::AudioFunctionality,
            ecosystem_honesty_class: EcosystemHonestyClass::PartiallySupported,
        };

        // 5 — MacosDarling: CLI tool.
        let p5 = AppProfile {
            app_id: "com.panic.transmit".into(),
            ecosystem_runtime: EcosystemRuntime::RuntimeMacosDarling,
            current_recipe_trust_class: RecipeTrustClass::RecipeImported,
            headline_rating: CompatibilityRating::Bronze,
            headline_evidence_level: EvidenceLevel::SelfReported,
            worst_dimension: RatingDimension::NetworkBehavior,
            ecosystem_honesty_class: EcosystemHonestyClass::PartiallySupported,
        };

        let fixtures: Vec<(PackageId, AppProfile)> = vec![
            (PackageId("pkg_fixture_linux_native".into()), p1),
            (PackageId("pkg_fixture_flatpak".into()), p2),
            (PackageId("pkg_fixture_wine".into()), p3),
            (PackageId("pkg_fixture_waydroid".into()), p4),
            (PackageId("pkg_fixture_darling".into()), p5),
        ];

        let mut profiles = HashMap::new();
        for (id, profile) in fixtures {
            let mut entry = KnowledgeDbEntry::new(profile);
            entry.last_updated = now;
            profiles.insert(id, entry);
        }

        Self {
            profiles: RwLock::new(profiles),
            profile_authority: db.profile_authority,
        }
    }

    // ------------------------------------------------------------------
    // Registration
    // ------------------------------------------------------------------

    /// Register a profile with an Ed25519 signature over its canonical
    /// serialised bytes.
    ///
    /// The signature must verify against at least one trusted authority in
    /// `profile_authority`. If the authority set is empty every registration
    /// is rejected.
    ///
    /// # Errors
    ///
    /// - `ValidationFailed` when the signature is invalid, no authority
    ///   matches, or a profile is already registered for this `PackageId`.
    pub async fn register_profile(
        &self,
        package_id: PackageId,
        profile: AppProfile,
        signature: Vec<u8>,
    ) -> Result<(), AppsError> {
        // Reject duplicate registration — fail-closed.
        {
            let guard = self.profiles.read().await;
            if guard.contains_key(&package_id) {
                return Err(AppsError::ValidationFailed(format!(
                    "profile already registered for package_id: {}",
                    package_id.0,
                )));
            }
        }

        // Canonical serialisation for signature verification.
        let canonical_bytes = serde_json::to_vec(&profile).map_err(|e| {
            AppsError::ValidationFailed(format!("failed to serialise profile: {e}"))
        })?;

        let sig_bytes: [u8; 64] = signature.as_slice().try_into().map_err(|_| {
            AppsError::ValidationFailed("invalid signature length: expected 64 bytes".into())
        })?;
        let sig = Signature::from_bytes(&sig_bytes);

        let mut verified = false;
        for vk in self.profile_authority.values() {
            if vk.verify_strict(&canonical_bytes, &sig).is_ok() {
                verified = true;
                break;
            }
        }

        if !verified {
            return Err(AppsError::ValidationFailed(
                "profile signature invalid: no trusted authority verified the signature".into(),
            ));
        }

        self.profiles
            .write()
            .await
            .insert(package_id, KnowledgeDbEntry::new(profile));

        Ok(())
    }

    // ------------------------------------------------------------------
    // Lookup
    // ------------------------------------------------------------------

    /// Look up a profile by its [`PackageId`].
    ///
    /// # Errors
    ///
    /// Returns [`AppsError::PackageNotFound`] when no entry exists for
    /// the given id.
    pub async fn lookup(&self, package_id: &PackageId) -> Result<AppProfile, AppsError> {
        self.profiles
            .read()
            .await
            .get(package_id)
            .map(|e| e.profile.clone())
            .ok_or_else(|| AppsError::PackageNotFound(package_id.0.clone()))
    }

    // ------------------------------------------------------------------
    // List by ecosystem
    // ------------------------------------------------------------------

    /// Return every profile whose `ecosystem_runtime` matches the given
    /// value. Result is unordered.
    #[must_use]
    pub async fn list_by_ecosystem(&self, ecosystem: EcosystemRuntime) -> Vec<AppProfile> {
        self.profiles
            .read()
            .await
            .values()
            .filter(|e| e.profile.ecosystem_runtime == ecosystem)
            .map(|e| e.profile.clone())
            .collect()
    }

    // ------------------------------------------------------------------
    // Mutation
    // ------------------------------------------------------------------

    /// Apply a partial mutation to an existing profile entry.
    ///
    /// Only the aspect named by the variant is changed; all other fields
    /// remain untouched. `SetCompatibilityScore` values above 100 are
    /// clamped to 100.
    ///
    /// # Errors
    ///
    /// Returns [`AppsError::PackageNotFound`] when no entry exists for
    /// the given `package_id`.
    #[allow(clippy::significant_drop_tightening)]
    pub async fn update_profile(
        &self,
        package_id: &PackageId,
        mutation: AppProfileMutation,
    ) -> Result<AppProfile, AppsError> {
        let mut guard = self.profiles.write().await;
        let entry = guard
            .get_mut(package_id)
            .ok_or_else(|| AppsError::PackageNotFound(package_id.0.clone()))?;

        match mutation {
            AppProfileMutation::AddIssue(issue) => {
                entry.known_issues.push(issue);
            }
            AppProfileMutation::AddHint(hint) => {
                entry.hints.push(hint);
            }
            AppProfileMutation::SetCompatibilityScore(score) => {
                entry.compatibility_score = score.min(100);
            }
            AppProfileMutation::MarkLastUpdated(ts) => {
                entry.last_updated = ts;
            }
        }

        Ok(entry.profile.clone())
    }

    // ------------------------------------------------------------------
    // Deletion
    // ------------------------------------------------------------------

    /// Remove a profile entry from the DB.
    ///
    /// # Errors
    ///
    /// Returns [`AppsError::PackageNotFound`] when no entry exists for
    /// the given `package_id`.
    pub async fn delete_profile(&self, package_id: &PackageId) -> Result<(), AppsError> {
        let removed = self.profiles.write().await.remove(package_id);
        if removed.is_none() {
            return Err(AppsError::PackageNotFound(package_id.0.clone()));
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Test seams
    // ------------------------------------------------------------------

    /// Return the number of stored profiles.
    #[allow(dead_code)]
    pub async fn profile_count(&self) -> usize {
        self.profiles.read().await.len()
    }

    /// Return the number of trusted authorities.
    #[allow(dead_code)]
    pub fn authority_count(&self) -> usize {
        self.profile_authority.len()
    }
}
