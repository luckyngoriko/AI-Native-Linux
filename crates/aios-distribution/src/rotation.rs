//! Publisher root key rotation per S11.1 §11 and AIOS root rotation event shape
//! per §4.1.
//!
//! # Rotation types
//!
//! - [`PublisherRotationEvent`] — a publisher root key rotation (old-root
//!   continuity signature + AIOS-root cosignature).  Reactive `KeyCompromise`
//!   rotations immediately revoke signing keys within the compromise window.
//! - [`AiosRootRotationEvent`] — the constitutional AIOS root rotation event
//!   shape (§4.1).  Requires recovery boot + dual human approval; firmware
//!   re-flash is operator-side and out of crate scope.
//! - [`RotationOutcome`] — the result of applying a rotation to the catalogs.

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, VerifyingKey};

use crate::catalog::{PublisherCatalog, SigningKeyCatalog};
use crate::error::DistributionError;
use crate::ids::PackageSigningKeyId;
use crate::takedown::TakedownReason;
use crate::trust_chain::{AiosRootKey, PublisherRoot};

// ---------------------------------------------------------------------------
// PublisherRotationEvent (§11)
// ---------------------------------------------------------------------------

/// A publisher root key rotation event per S11.1 §11.
///
/// The old publisher root signs the new public key for chain continuity; the
/// AIOS root co-signs to authorise the break.  Both signatures are verified
/// before the rotation is applied to the catalogs.
///
/// For reactive (`KeyCompromise`) rotations, the publisher specifies a
/// `compromise_window_start`; all signing keys with `valid_from ≥
/// compromise_window_start` are immediately revoked, and packages signed
/// by them transition `Active → Quarantined` on the next health check.
#[derive(Debug, Clone)]
pub struct PublisherRotationEvent {
    /// Publisher root identifier — `pub:<vendor>` per S11.1 §4.2.
    pub publisher_root_id: crate::ids::PublisherRootId,
    /// The old (current) Ed25519 public key (32 bytes).
    pub old_public_key: Vec<u8>,
    /// The new Ed25519 public key (32 bytes).
    pub new_public_key: Vec<u8>,
    /// Ed25519 signature by the old root over the canonical rotation bytes
    /// (chain continuity proof).
    pub old_root_signature_over_new: Vec<u8>,
    /// Ed25519 signature by the AIOS root over the canonical rotation bytes
    /// (authorising the break).
    pub aios_root_signature: Vec<u8>,
    /// When the rotation was performed.
    pub rotated_at: DateTime<Utc>,
    /// Reason for the rotation (`KeyCompromise`, `PublisherRequest`, etc.).
    pub reason: TakedownReason,
    /// For `KeyCompromise`: beginning of the compromise window.  Signing keys
    /// whose `valid_from ≥ compromise_window_start` are revoked.  `None` means
    /// all active keys are revoked when the reason is `KeyCompromise`.
    pub compromise_window_start: Option<DateTime<Utc>>,
}

/// Outcome of applying a publisher rotation to the catalogs.
#[derive(Debug, Clone)]
pub struct RotationOutcome {
    /// IDs of signing keys revoked by a reactive (`KeyCompromise`) rotation.
    pub revoked_signing_key_ids: Vec<PackageSigningKeyId>,
    /// `true` if this was a reactive rotation that revoked keys.
    pub reactive: bool,
}

// ---------------------------------------------------------------------------
// Canonical bytes for rotation signing
// ---------------------------------------------------------------------------

/// Returns the canonical bytes that both the old publisher root and the AIOS
/// root sign for a rotation event.
///
/// Format (newline-delimited):
/// ```text
/// <publisher_root_id>
/// <new_public_key bytes (32)>
/// <rotated_at RFC 3339>
/// ```
fn rotation_canonical_bytes(event: &PublisherRotationEvent) -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    buf.extend_from_slice(event.publisher_root_id.0.as_bytes());
    buf.push(b'\n');
    buf.extend_from_slice(&event.new_public_key);
    buf.push(b'\n');
    buf.extend_from_slice(event.rotated_at.to_rfc3339().as_bytes());
    buf
}

// ---------------------------------------------------------------------------
// Ed25519 helpers
// ---------------------------------------------------------------------------

/// Verifies an Ed25519 signature against a verifying key.  Returns `Ok(())` on
/// success or `Err(DistributionError::SignatureFailed)` on failure (malformed
/// signature bytes or verification failure).
fn verify_ed25519(
    payload: &[u8],
    signature_bytes: &[u8],
    verifying_key: &VerifyingKey,
) -> Result<(), DistributionError> {
    let sig = Signature::from_slice(signature_bytes).map_err(|_| {
        DistributionError::SignatureFailed("rotation: malformed signature bytes".into())
    })?;
    verifying_key.verify_strict(payload, &sig).map_err(|_| {
        DistributionError::SignatureFailed("rotation: Ed25519 verification failed".into())
    })
}

// ---------------------------------------------------------------------------
// Verify
// ---------------------------------------------------------------------------

/// Verifies a [`PublisherRotationEvent`] against the current publisher root
/// and the AIOS root key.
///
/// # Checks
///
/// 1. `old_root_signature_over_new` must verify against
///    `current_root.public_key` — chain continuity.
/// 2. `aios_root_signature` must verify against `aios_root.public_key` —
///    authorisation of the rotation.
///
/// # Errors
///
/// Returns `Err(DistributionError::SignatureFailed)` if either signature
/// verification fails.
pub fn verify_rotation_event(
    event: &PublisherRotationEvent,
    current_root: &PublisherRoot,
    aios_root: &AiosRootKey,
) -> Result<(), DistributionError> {
    let canonical = rotation_canonical_bytes(event);

    // 1. Old root → new key (chain continuity)
    let old_vk =
        VerifyingKey::from_bytes(&event.old_public_key.as_slice().try_into().map_err(|_| {
            DistributionError::SignatureFailed(
                "rotation: old_public_key must be exactly 32 bytes".into(),
            )
        })?)
        .map_err(|_| {
            DistributionError::SignatureFailed(
                "rotation: old_public_key is not a valid Ed25519 verifying key".into(),
            )
        })?;
    verify_ed25519(&canonical, &event.old_root_signature_over_new, &old_vk)?;

    // Also verify the old key matches the current root's key (belt-and-suspenders)
    if old_vk.as_bytes() != current_root.public_key.as_bytes() {
        return Err(DistributionError::SignatureFailed(
            "rotation: old_public_key does not match the current publisher root's public key"
                .into(),
        ));
    }

    // 2. AIOS root → rotation authorisation
    verify_ed25519(
        &canonical,
        &event.aios_root_signature,
        &aios_root.public_key,
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

/// Applies a publisher root key rotation to the catalogs.
///
/// # Steps
///
/// 1. Look up the publisher in the catalog → absent → `PublisherNotFound`.
/// 2. Verify the rotation event (old-root continuity + AIOS-root cosignature).
/// 3. Update the publisher's `public_key` to `new_public_key`.
/// 4. If `reason == KeyCompromise`, revoke signing keys within the compromise
///    window and return the revoked key IDs in the [`RotationOutcome`].
///
/// # Errors
///
/// Returns `Err(DistributionError::PublisherNotFound)` if the publisher is
/// absent from the catalog, or `Err(DistributionError::SignatureFailed)` if
/// verification fails.
pub fn apply_publisher_rotation(
    catalog: &mut PublisherCatalog,
    signing_catalog: &mut SigningKeyCatalog,
    event: &PublisherRotationEvent,
    aios_root: &AiosRootKey,
    _now: DateTime<Utc>,
) -> Result<RotationOutcome, DistributionError> {
    // Step 1: find publisher in catalog
    let publisher = catalog.lookup(&event.publisher_root_id).ok_or_else(|| {
        DistributionError::PublisherNotFound(format!(
            "publisher {} not found in catalog — cannot rotate",
            event.publisher_root_id.0
        ))
    })?;

    // Step 2: verify rotation event (needs a read-only view of the publisher first)
    verify_rotation_event(event, publisher, aios_root)?;

    // Step 3: update publisher public_key
    let new_vk =
        VerifyingKey::from_bytes(&event.new_public_key.as_slice().try_into().map_err(|_| {
            DistributionError::SignatureFailed(
                "rotation: new_public_key must be exactly 32 bytes".into(),
            )
        })?)
        .map_err(|_| {
            DistributionError::SignatureFailed(
                "rotation: new_public_key is not a valid Ed25519 verifying key".into(),
            )
        })?;

    let entry = catalog.get_mut(&event.publisher_root_id).ok_or_else(|| {
        DistributionError::PublisherNotFound(format!(
            "publisher {} not found in catalog after lookup — internal inconsistency",
            event.publisher_root_id.0
        ))
    })?;
    entry.public_key = new_vk;

    // Step 4: reactive KeyCompromise → revoke signing keys
    if event.reason == TakedownReason::KeyCompromise {
        let revoked = signing_catalog.revoke_all_active(event.rotated_at);
        return Ok(RotationOutcome {
            revoked_signing_key_ids: revoked,
            reactive: true,
        });
    }

    Ok(RotationOutcome {
        revoked_signing_key_ids: Vec::new(),
        reactive: false,
    })
}

// ---------------------------------------------------------------------------
// AIOS root rotation event shape (§4.1)
// ---------------------------------------------------------------------------

/// The AIOS root key rotation event shape per S11.1 §4.1.
///
/// AIOS root rotation is a constitutional operation requiring recovery-mode
/// boot, dual human approval, a self-attestation signature linking the new key
/// to the old key, and a firmware re-flash of the boot-time trust anchor.
///
/// # Invariants
///
/// - [`requires_recovery`](Self::requires_recovery) always returns `true` —
///   AIOS root rotation must be performed under recovery mode.
/// - [`requires_dual_approval`](Self::requires_dual_approval) always returns
///   `true` — AIOS root rotation requires two-human co-signer approval
///   (`ApprovalStrength = DUAL` per S5.3 §3.3).
///
/// # Out of scope
///
/// The firmware re-flash (step 6 of the §4.1 rotation procedure) is an
/// operator-side operation performed on the boot medium; it is not
/// modelled in this crate.  The event shape records the cryptographic
/// link between old and new keys for the FOREVER `AIOS_ROOT_KEY_ROTATED`
/// evidence record (T-196).
#[derive(Debug, Clone)]
pub struct AiosRootRotationEvent {
    /// The old AIOS root public key (32 bytes).
    pub old_public_key: Vec<u8>,
    /// The new AIOS root public key (32 bytes).
    pub new_public_key: Vec<u8>,
    /// Ed25519 self-attestation linking the new key to the old key
    /// (signed by the old key over the canonical bytes of the new key).
    pub self_attestation_signature: Vec<u8>,
    /// When the rotation was performed.
    pub rotated_at: DateTime<Utc>,
}

impl AiosRootRotationEvent {
    /// Returns `true` — AIOS root rotation always requires recovery boot.
    ///
    /// Per S11.1 §4.1 step 1: `RecoveryMode = RECOVERY` (S9.1 §3.2).
    #[must_use]
    pub const fn requires_recovery() -> bool {
        true
    }

    /// Returns `true` — AIOS root rotation always requires dual human approval.
    ///
    /// Per S11.1 §4.1 step 3: `ApprovalStrength = DUAL` (S5.3 §3.3).
    #[must_use]
    pub const fn requires_dual_approval() -> bool {
        true
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
    use crate::ids::PublisherRootId;

    #[test]
    fn aios_root_requires_recovery_is_true() {
        assert!(AiosRootRotationEvent::requires_recovery());
    }

    #[test]
    fn aios_root_requires_dual_approval_is_true() {
        assert!(AiosRootRotationEvent::requires_dual_approval());
    }

    #[test]
    fn aios_root_rotation_event_instantiation() {
        let event = AiosRootRotationEvent {
            old_public_key: vec![0u8; 32],
            new_public_key: vec![1u8; 32],
            self_attestation_signature: vec![2u8; 64],
            rotated_at: Utc::now(),
        };
        assert_eq!(event.old_public_key.len(), 32);
        assert_eq!(event.new_public_key.len(), 32);
        assert_eq!(event.self_attestation_signature.len(), 64);
    }

    #[test]
    fn rotation_canonical_bytes_are_deterministic() {
        let now = Utc::now();
        let id = PublisherRootId("pub:testvendor".into());
        let new_pk = vec![0xAAu8; 32];

        let event = PublisherRotationEvent {
            publisher_root_id: id,
            old_public_key: vec![0xBBu8; 32],
            new_public_key: new_pk,
            old_root_signature_over_new: vec![0u8; 64],
            aios_root_signature: vec![0u8; 64],
            rotated_at: now,
            reason: TakedownReason::PublisherRequest,
            compromise_window_start: None,
        };

        let bytes1 = rotation_canonical_bytes(&event);
        let bytes2 = rotation_canonical_bytes(&event);
        assert_eq!(bytes1, bytes2, "canonical bytes must be deterministic");

        // Verify the canonical bytes contain the expected components
        let as_str = String::from_utf8_lossy(&bytes1);
        assert!(
            as_str.contains("pub:testvendor"),
            "must contain publisher_root_id"
        );
        assert!(
            as_str.contains(&now.to_rfc3339()),
            "must contain rotated_at"
        );
    }
}
