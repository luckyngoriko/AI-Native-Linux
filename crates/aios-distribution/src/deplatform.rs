//! Deplatform / takedown discipline per S11.1 §12.
//!
//! # Deplatform flow
//!
//! 1. AIOS root cosigns a [`PublisherDeplatformEvent`].
//! 2. [`apply_deplatform`] sets the publisher's `trust_level = Deplatformed` +
//!    `retired_at` in the publisher catalog.
//! 3. On next health check, [`health_check_quarantine`] transitions all of
//!    the publisher's `Active` packages to `Quarantined`.
//! 4. New installs from the publisher are rejected with `PublisherDeplatformed`
//!    (enforced in the install pipeline, T-190).
//! 5. Existing installs run until the grace period ends, then auto-uninstall.
//!    The operator may extend the grace exactly once by ≤ 30 days via
//!    [`extend_grace`].
//! 6. A deplatform is **not reversible** by publisher action — a returning
//!    publisher starts fresh as `Community` trust per
//!    [`returning_publisher_default_trust`].

use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::{Signature, VerifyingKey};

use crate::catalog::PublisherCatalog;
use crate::error::DistributionError;
use crate::ids::{PackageId, PackageSigningKeyId, PublisherRootId};
use crate::install_state::PackageInstallState;
use crate::takedown::TakedownReason;
use crate::trust::PublisherTrustLevel;
use crate::trust_chain::AiosRootKey;

// ---------------------------------------------------------------------------
// PublisherDeplatformEvent (§12)
// ---------------------------------------------------------------------------

/// A publisher deplatform / takedown event per S11.1 §12.
///
/// AIOS-root-cosigned; sets the publisher's trust level to `Deplatformed`.
/// Packages from this publisher are auto-quarantined on the next health check.
/// Existing installs run until the grace period ends; the operator may extend
/// the grace exactly once by at most 30 days.
///
/// # Non-reversibility
///
/// A deplatform is **not reversible** by ordinary publisher action.  A
/// formerly-deplatformed publisher returning under a new identity is
/// treated as a fresh publisher with `Community` trust (see
/// [`returning_publisher_default_trust`]).
#[derive(Debug, Clone)]
pub struct PublisherDeplatformEvent {
    /// Publisher root identifier — `pub:<vendor>` per S11.1 §4.2.
    pub publisher_root_id: PublisherRootId,
    /// Reason for the deplatform (one of seven `TakedownReason` variants).
    pub reason: TakedownReason,
    /// When the deplatform took effect.
    pub deplatformed_at: DateTime<Utc>,
    /// End of the grace period — default `deplatformed_at + 30 days`.
    /// Existing installs auto-uninstall at this time.
    pub grace_period_ends_at: DateTime<Utc>,
    /// Pointer to the AIOS root review record (URL, evidence record ID, etc.).
    pub evidence_pointer: String,
    /// Ed25519 signature by the AIOS root over the canonical deplatform bytes.
    pub aios_root_signature: Vec<u8>,
    /// Whether the grace period has been extended exactly once.
    /// [`extend_grace`] sets this to `true`; a second extension is rejected.
    pub extended: bool,
}

// ---------------------------------------------------------------------------
// InstalledPackageRecord (minimal — reconciled with apps registry in T-197)
// ---------------------------------------------------------------------------

/// A minimal installed-package record for health-check quarantine logic.
///
/// This is a T-192 placeholder.  T-197 will reconcile this with the full
/// apps-registry record that tracks install paths, manifests, and sandbox
/// profiles.
#[derive(Debug, Clone)]
pub struct InstalledPackageRecord {
    /// Package identifier — `pkg:<vendor>:<name>`.
    pub package_id: PackageId,
    /// The publisher that signed this package's manifest.
    pub publisher_root_id: PublisherRootId,
    /// The signing key that signed this package's manifest.
    pub signing_key_id: PackageSigningKeyId,
    /// Current install state in the FSM.
    pub state: PackageInstallState,
    /// When the package was installed.
    pub installed_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Canonical bytes for deplatform signing
// ---------------------------------------------------------------------------

/// Returns the canonical bytes that the AIOS root signs for a deplatform event.
///
/// Format (newline-delimited):
/// ```text
/// <publisher_root_id>
/// <reason label>
/// <deplatformed_at RFC 3339>
/// <grace_period_ends_at RFC 3339>
/// <evidence_pointer>
/// ```
fn deplatform_canonical_bytes(event: &PublisherDeplatformEvent) -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    buf.extend_from_slice(event.publisher_root_id.0.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(event.reason.label().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(event.deplatformed_at.to_rfc3339().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(event.grace_period_ends_at.to_rfc3339().as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(event.evidence_pointer.as_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Ed25519 helpers
// ---------------------------------------------------------------------------

/// Verifies an Ed25519 signature against a verifying key.
fn verify_ed25519(
    payload: &[u8],
    signature_bytes: &[u8],
    verifying_key: &VerifyingKey,
) -> Result<(), DistributionError> {
    let sig = Signature::from_slice(signature_bytes).map_err(|_| {
        DistributionError::SignatureFailed("deplatform: malformed signature bytes".into())
    })?;
    verifying_key.verify_strict(payload, &sig).map_err(|_| {
        DistributionError::SignatureFailed("deplatform: Ed25519 verification failed".into())
    })
}

// ---------------------------------------------------------------------------
// Grace period
// ---------------------------------------------------------------------------

/// Returns the default grace period end timestamp: `deplatformed_at + 30 days`.
///
/// Per S11.1 §12 step 6, existing installs run for 30 days after deplatform
/// before auto-uninstall.
#[must_use]
pub fn default_grace_end(deplatformed_at: DateTime<Utc>) -> DateTime<Utc> {
    deplatformed_at + Duration::days(30)
}

/// Returns `true` if the deplatform grace period has expired relative to `now`.
///
/// When `now ≥ event.grace_period_ends_at`, the grace has expired and
/// existing installs should auto-uninstall.
#[must_use]
pub fn grace_expired(event: &PublisherDeplatformEvent, now: DateTime<Utc>) -> bool {
    now >= event.grace_period_ends_at
}

/// Extends the deplatform grace period by `extension`.
///
/// # Constraints (S11.1 §12 step 7)
///
/// - The grace may be extended **exactly once**.
/// - The extension must be **≤ 30 days**.
///
/// # Errors
///
/// Returns `Err(DistributionError::TakedownActive)` if the grace has already
/// been extended, or if the extension exceeds 30 days.
pub fn extend_grace(
    event: &mut PublisherDeplatformEvent,
    extension: Duration,
    _now: DateTime<Utc>,
) -> Result<(), DistributionError> {
    if event.extended {
        return Err(DistributionError::TakedownActive(
            "grace period has already been extended once — second extension denied".into(),
        ));
    }

    let max_extension = Duration::days(30);
    if extension > max_extension {
        return Err(DistributionError::TakedownActive(format!(
            "grace extension of {extension} exceeds the maximum 30 days"
        )));
    }

    event.grace_period_ends_at += extension;
    event.extended = true;
    Ok(())
}

// ---------------------------------------------------------------------------
// Verify
// ---------------------------------------------------------------------------

/// Verifies a [`PublisherDeplatformEvent`] against the AIOS root key.
///
/// # Checks
///
/// - `aios_root_signature` must verify against `aios_root.public_key` over
///   the canonical deplatform bytes.
///
/// # Errors
///
/// Returns `Err(DistributionError::SignatureFailed)` if verification fails.
pub fn verify_deplatform_event(
    event: &PublisherDeplatformEvent,
    aios_root: &AiosRootKey,
) -> Result<(), DistributionError> {
    let canonical = deplatform_canonical_bytes(event);
    verify_ed25519(
        &canonical,
        &event.aios_root_signature,
        &aios_root.public_key,
    )
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

/// Applies a deplatform event to the publisher catalog.
///
/// # Steps
///
/// 1. Verify the AIOS root signature on the event.
/// 2. Find the publisher in the catalog.
/// 3. Set `trust_level = Deplatformed` and `retired_at = deplatformed_at`.
///
/// # Errors
///
/// Returns `Err(DistributionError::SignatureFailed)` if the AIOS root
/// signature is invalid, or `Err(DistributionError::PublisherNotFound)` if
/// the publisher is absent from the catalog.
pub fn apply_deplatform(
    catalog: &mut PublisherCatalog,
    event: &PublisherDeplatformEvent,
    aios_root: &AiosRootKey,
    _now: DateTime<Utc>,
) -> Result<(), DistributionError> {
    // Step 1: verify AIOS root signature
    verify_deplatform_event(event, aios_root)?;

    // Step 2 & 3: find publisher and set deplatformed state
    let entry = catalog.get_mut(&event.publisher_root_id).ok_or_else(|| {
        DistributionError::PublisherNotFound(format!(
            "publisher {} not found in catalog — cannot deplatform",
            event.publisher_root_id.0
        ))
    })?;

    entry.trust_level = PublisherTrustLevel::Deplatformed;
    entry.retired_at = Some(event.deplatformed_at);

    Ok(())
}

// ---------------------------------------------------------------------------
// Health check quarantine
// ---------------------------------------------------------------------------

/// Runs a health-check quarantine sweep over installed packages.
///
/// Transitions `Active → Quarantined` (via the install FSM [`apply`]) for any
/// installed package whose:
///
/// - Publisher is `Deplatformed` (checked against the publisher catalog), or
/// - Signing key is in `revoked_keys` (from a reactive `KeyCompromise` rotation).
///
/// Returns the [`PackageId`]s of the packages that were newly quarantined.
///
/// This function operates on the minimal [`InstalledPackageRecord`] type;
/// T-197 will reconcile it with the full apps-registry record.
#[must_use]
pub fn health_check_quarantine(
    installed: &mut [InstalledPackageRecord],
    catalog: &PublisherCatalog,
    revoked_keys: &[PackageSigningKeyId],
    _now: DateTime<Utc>,
) -> Vec<PackageId> {
    let mut quarantined = Vec::new();

    for record in installed.iter_mut() {
        if record.state != PackageInstallState::Active {
            continue;
        }

        let should_quarantine = catalog
            .lookup(&record.publisher_root_id)
            .is_some_and(|publisher| publisher.trust_level == PublisherTrustLevel::Deplatformed)
            || revoked_keys.contains(&record.signing_key_id);

        if should_quarantine {
            // Use the install FSM apply to transition Active → Quarantined
            if crate::apply_install_state(&mut record.state, PackageInstallState::Quarantined)
                .is_ok()
            {
                quarantined.push(record.package_id.clone());
            }
        }
    }

    quarantined
}

// ---------------------------------------------------------------------------
// Non-reversibility
// ---------------------------------------------------------------------------

/// Returns the default trust level for a returning deplatformed publisher.
///
/// Per S11.1 §12: a formerly-deplatformed publisher returning under a new
/// identity is treated as a fresh publisher with `Community` trust.  AIOS
/// root can grade them up only after the standard onboarding review.
/// There is no `undeplatform` function — the deplatform is not reversible
/// by publisher action.
#[must_use]
pub const fn returning_publisher_default_trust() -> PublisherTrustLevel {
    PublisherTrustLevel::Community
}

// ---------------------------------------------------------------------------
// Convenience: TakedownReason label helper (used in canonical bytes)
// ---------------------------------------------------------------------------

impl TakedownReason {
    /// Returns the canonical S11.1 label for this takedown reason.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::MaliciousBehaviorDetected => "MALICIOUS_BEHAVIOR_DETECTED",
            Self::SupplyChainCompromise => "SUPPLY_CHAIN_COMPROMISE",
            Self::CapabilityLieDetected => "CAPABILITY_LIE_DETECTED",
            Self::LegalRequirement => "LEGAL_REQUIREMENT",
            Self::PublisherRequest => "PUBLISHER_REQUEST",
            Self::KeyCompromise => "KEY_COMPROMISE",
            Self::AbandonedAfterInactiveTtl => "ABANDONED_AFTER_INACTIVE_TTL",
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    clippy::too_many_arguments,
    clippy::float_cmp
)]
mod tests {
    use super::*;

    #[test]
    fn default_grace_end_is_30_days() {
        let now = Utc::now();
        let grace_end = default_grace_end(now);
        let diff = grace_end - now;
        // Approximately 30 days (allow for hour-level rounding)
        assert!(diff.num_days() >= 29 && diff.num_days() <= 31);
    }

    #[test]
    fn grace_expired_false_before_grace_end() {
        let deplatformed_at = Utc::now();
        let grace_end = default_grace_end(deplatformed_at);
        let event = PublisherDeplatformEvent {
            publisher_root_id: PublisherRootId("pub:test".into()),
            reason: TakedownReason::MaliciousBehaviorDetected,
            deplatformed_at,
            grace_period_ends_at: grace_end,
            evidence_pointer: "test://evidence/1".into(),
            aios_root_signature: vec![0u8; 64],
            extended: false,
        };
        // Check just before grace end
        assert!(!grace_expired(
            &event,
            grace_end - Duration::try_seconds(1).unwrap()
        ));
    }

    #[test]
    fn grace_expired_true_after_grace_end() {
        let deplatformed_at = Utc::now();
        let grace_end = default_grace_end(deplatformed_at);
        let event = PublisherDeplatformEvent {
            publisher_root_id: PublisherRootId("pub:test".into()),
            reason: TakedownReason::MaliciousBehaviorDetected,
            deplatformed_at,
            grace_period_ends_at: grace_end,
            evidence_pointer: "test://evidence/1".into(),
            aios_root_signature: vec![0u8; 64],
            extended: false,
        };
        // Check at grace end
        assert!(grace_expired(&event, grace_end));
        // Check after grace end
        assert!(grace_expired(
            &event,
            grace_end + Duration::try_days(1).unwrap()
        ));
    }

    #[test]
    fn extend_grace_first_extension_ok() {
        let deplatformed_at = Utc::now();
        let grace_end = default_grace_end(deplatformed_at);
        let mut event = PublisherDeplatformEvent {
            publisher_root_id: PublisherRootId("pub:test".into()),
            reason: TakedownReason::LegalRequirement,
            deplatformed_at,
            grace_period_ends_at: grace_end,
            evidence_pointer: "test://evidence/2".into(),
            aios_root_signature: vec![0u8; 64],
            extended: false,
        };
        let original_grace = event.grace_period_ends_at;
        let extension = Duration::try_days(15).unwrap();
        let result = extend_grace(&mut event, extension, Utc::now());
        assert!(result.is_ok());
        assert!(event.extended);
        assert_eq!(event.grace_period_ends_at, original_grace + extension);
    }

    #[test]
    fn extend_grace_second_extension_errors() {
        let deplatformed_at = Utc::now();
        let grace_end = default_grace_end(deplatformed_at);
        let mut event = PublisherDeplatformEvent {
            publisher_root_id: PublisherRootId("pub:test".into()),
            reason: TakedownReason::LegalRequirement,
            deplatformed_at,
            grace_period_ends_at: grace_end,
            evidence_pointer: "test://evidence/3".into(),
            aios_root_signature: vec![0u8; 64],
            extended: false,
        };
        // First extension
        let _ = extend_grace(&mut event, Duration::try_days(10).unwrap(), Utc::now());
        // Second extension
        let result = extend_grace(&mut event, Duration::try_days(10).unwrap(), Utc::now());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.code(),
            crate::error::DistributionErrorCode::TakedownActive
        );
    }

    #[test]
    fn extend_grace_exceeds_30_days_errors() {
        let deplatformed_at = Utc::now();
        let grace_end = default_grace_end(deplatformed_at);
        let mut event = PublisherDeplatformEvent {
            publisher_root_id: PublisherRootId("pub:test".into()),
            reason: TakedownReason::LegalRequirement,
            deplatformed_at,
            grace_period_ends_at: grace_end,
            evidence_pointer: "test://evidence/4".into(),
            aios_root_signature: vec![0u8; 64],
            extended: false,
        };
        let result = extend_grace(&mut event, Duration::try_days(31).unwrap(), Utc::now());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.code(),
            crate::error::DistributionErrorCode::TakedownActive
        );
    }

    #[test]
    fn returning_publisher_default_trust_is_community() {
        assert_eq!(
            returning_publisher_default_trust(),
            PublisherTrustLevel::Community
        );
    }

    #[test]
    fn health_check_quarantine_empty_installed_returns_empty() {
        let catalog = PublisherCatalog::new(vec![]);
        let revoked: Vec<PackageSigningKeyId> = vec![];
        let mut installed: Vec<InstalledPackageRecord> = vec![];
        let result = health_check_quarantine(&mut installed, &catalog, &revoked, Utc::now());
        assert!(result.is_empty());
    }

    #[test]
    fn health_check_non_active_package_skipped() {
        let catalog = PublisherCatalog::new(vec![]);
        let revoked: Vec<PackageSigningKeyId> = vec![];
        let mut installed = vec![InstalledPackageRecord {
            package_id: PackageId("pkg:test:app".into()),
            publisher_root_id: PublisherRootId("pub:test".into()),
            signing_key_id: PackageSigningKeyId("pks:test:release".into()),
            state: PackageInstallState::Quarantined,
            installed_at: Utc::now(),
        }];
        let result = health_check_quarantine(&mut installed, &catalog, &revoked, Utc::now());
        assert!(result.is_empty());
        assert_eq!(installed[0].state, PackageInstallState::Quarantined);
    }

    #[test]
    fn health_check_quarantine_by_revoked_signing_key() {
        let revoked_key_id = PackageSigningKeyId("pks:test:revoked".into());
        let catalog = PublisherCatalog::new(vec![]);
        let revoked = vec![revoked_key_id.clone()];

        let mut installed = vec![InstalledPackageRecord {
            package_id: PackageId("pkg:test:app".into()),
            publisher_root_id: PublisherRootId("pub:test".into()),
            signing_key_id: revoked_key_id,
            state: PackageInstallState::Active,
            installed_at: Utc::now(),
        }];

        let result = health_check_quarantine(&mut installed, &catalog, &revoked, Utc::now());
        assert_eq!(result.len(), 1);
        assert_eq!(installed[0].state, PackageInstallState::Quarantined);
    }

    #[test]
    fn takedown_reason_labels() {
        assert_eq!(
            TakedownReason::MaliciousBehaviorDetected.label(),
            "MALICIOUS_BEHAVIOR_DETECTED"
        );
        assert_eq!(
            TakedownReason::SupplyChainCompromise.label(),
            "SUPPLY_CHAIN_COMPROMISE"
        );
        assert_eq!(
            TakedownReason::CapabilityLieDetected.label(),
            "CAPABILITY_LIE_DETECTED"
        );
        assert_eq!(
            TakedownReason::LegalRequirement.label(),
            "LEGAL_REQUIREMENT"
        );
        assert_eq!(
            TakedownReason::PublisherRequest.label(),
            "PUBLISHER_REQUEST"
        );
        assert_eq!(TakedownReason::KeyCompromise.label(), "KEY_COMPROMISE");
        assert_eq!(
            TakedownReason::AbandonedAfterInactiveTtl.label(),
            "ABANDONED_AFTER_INACTIVE_TTL"
        );
    }

    #[test]
    fn deplatform_canonical_bytes_are_deterministic() {
        let now = Utc::now();
        let grace_end = default_grace_end(now);
        let event = PublisherDeplatformEvent {
            publisher_root_id: PublisherRootId("pub:testvendor".into()),
            reason: TakedownReason::MaliciousBehaviorDetected,
            deplatformed_at: now,
            grace_period_ends_at: grace_end,
            evidence_pointer: "aios-root://review/123".into(),
            aios_root_signature: vec![0u8; 64],
            extended: false,
        };

        let b1 = deplatform_canonical_bytes(&event);
        let b2 = deplatform_canonical_bytes(&event);
        assert_eq!(b1, b2);

        let as_str = String::from_utf8_lossy(&b1);
        assert!(as_str.contains("pub:testvendor"));
        assert!(as_str.contains("MALICIOUS_BEHAVIOR_DETECTED"));
        assert!(as_str.contains("aios-root://review/123"));
    }
}
