//! `UpdateChannel` rollout discipline per S11.1 §3.3.
//!
//! Governs which channel a package may follow, when auto-update is
//! permitted, and what transitions require re-issuance or explicit
//! operator approval.

// Manual `match` used instead of `matches!` in const fn because
// `const PartialEq` is not available for the derived impl on
// `UpdateChannel` / `RepositoryKind`.
#![allow(clippy::match_like_matches_macro)]

use chrono::{DateTime, Utc};

use crate::error::DistributionError;
use crate::repository::{RepositoryKind, UpdateChannel};

// ---------------------------------------------------------------------------
// Auto-update gate
// ---------------------------------------------------------------------------

/// Returns `true` if the given channel permits automatic updates.
///
/// Per §3.3:
/// - `Stable` → true (but only within the publisher's update window).
/// - `Beta` → false (explicit operator opt-in per package; never auto-set /
///   never auto-update).
/// - `RecoveryCritical` → false (updates require recovery-mode approval).
/// - `DeprecatedRetention` → false (no new versions; existing installs
///   continue until auto-quarantine on `eol_at`).
#[must_use]
pub const fn auto_update_allowed(channel: UpdateChannel) -> bool {
    match channel {
        UpdateChannel::Stable => true,
        UpdateChannel::Beta
        | UpdateChannel::RecoveryCritical
        | UpdateChannel::DeprecatedRetention => false,
    }
}

// ---------------------------------------------------------------------------
// Update window
// ---------------------------------------------------------------------------

/// A publisher-defined temporal window during which automatic updates are
/// permitted for `Stable`-channel packages.
///
/// Outside the window, even `Stable` packages are not auto-updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateWindow {
    /// Start of the update window (inclusive).
    pub start: DateTime<Utc>,
    /// End of the update window (inclusive).
    pub end: DateTime<Utc>,
}

impl UpdateWindow {
    /// Returns `true` if `now` falls within the window (inclusive of both
    /// endpoints).
    #[must_use]
    pub fn in_window(&self, now: DateTime<Utc>) -> bool {
        now >= self.start && now <= self.end
    }
}

/// Convenience: `true` iff the channel is `Stable` **and** `now` is
/// within the publisher's update window.
#[must_use]
pub fn stable_auto_update_permitted(window: &UpdateWindow, now: DateTime<Utc>) -> bool {
    window.in_window(now)
}

// ---------------------------------------------------------------------------
// Channel transitions
// ---------------------------------------------------------------------------

/// Returns `true` if moving from `from` to `to` requires re-issuance of
/// the package (new `manifest_canonical_hash`, fresh signature, fresh
/// approval).
///
/// Per §3.3: a `Beta` package **may not** transition to `Stable` without
/// re-issuance. Other transitions are documented but do not automatically
/// require re-issuance (they are governed by publisher policy).
#[must_use]
pub const fn requires_reissue_for_channel_change(from: UpdateChannel, to: UpdateChannel) -> bool {
    match (from, to) {
        (UpdateChannel::Beta, UpdateChannel::Stable) => true,
        _ => false,
    }
}

/// Returns `true` if the operator is widening the per-package channel
/// preference (e.g. moving from `Stable` to `Beta`).
///
/// Channel widening requires explicit operator approval.
/// Narrowing (e.g. `Beta` → `Stable`) is not considered widening and
/// does **not** require approval (though it may require re-issuance —
/// see [`requires_reissue_for_channel_change`]).
#[must_use]
pub const fn channel_widening_requires_approval(from: UpdateChannel, to: UpdateChannel) -> bool {
    // Widening = moving to a less-stable / riskier channel.
    // Use nested or-patterns to satisfy clippy::unnested_or_patterns.
    #[allow(clippy::unnested_or_patterns)]
    match (from, to) {
        (UpdateChannel::Stable, UpdateChannel::Beta)
        | (UpdateChannel::Stable, UpdateChannel::DeprecatedRetention)
        | (UpdateChannel::Beta, UpdateChannel::RecoveryCritical)
        | (UpdateChannel::Beta, UpdateChannel::DeprecatedRetention)
        | (UpdateChannel::RecoveryCritical, UpdateChannel::DeprecatedRetention) => true,
        _ => false,
    }
    // Note: Recovering from DeprecatedRetention to any other channel is not
    // widening — it is a recovery path that requires re-issuance separately.
}

// ---------------------------------------------------------------------------
// Repository-channel compatibility
// ---------------------------------------------------------------------------

/// Validates that `channel` is compatible with `repo`.
///
/// Per §3.3: `RecoveryCritical` is only valid for `AiosRecoveryRepo`.
/// Any other combination with `RecoveryCritical` is an error.
///
/// # Errors
///
/// Returns `RepositoryKindMismatch` if `RecoveryCritical` is used with a
/// non-recovery repository.
pub fn validate_channel_for_repo(
    channel: UpdateChannel,
    repo: RepositoryKind,
) -> Result<(), DistributionError> {
    if channel == UpdateChannel::RecoveryCritical && repo != RepositoryKind::AiosRecoveryRepo {
        return Err(DistributionError::RepositoryKindMismatch(format!(
            "RecoveryCritical channel is only valid for AiosRecoveryRepo; got {repo:?} with RecoveryCritical"
        )));
    }
    Ok(())
}

/// Returns `true` if the channel requires the system to be in recovery
/// mode.
///
/// Per §3.3: `RecoveryCritical` packages require recovery-mode approval.
#[must_use]
pub const fn recovery_critical_requires_recovery(channel: UpdateChannel) -> bool {
    match channel {
        UpdateChannel::RecoveryCritical => true,
        UpdateChannel::Stable | UpdateChannel::Beta | UpdateChannel::DeprecatedRetention => false,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::similar_names,
    reason = "unit tests in the same module"
)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_window(start_h: i64, end_h: i64) -> UpdateWindow {
        UpdateWindow {
            start: Utc.timestamp_opt(start_h * 3600, 0).unwrap(),
            end: Utc.timestamp_opt(end_h * 3600, 0).unwrap(),
        }
    }

    fn now_h(h: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(h * 3600, 0).unwrap()
    }

    #[test]
    fn auto_update_allowed_stable_true() {
        assert!(auto_update_allowed(UpdateChannel::Stable));
    }

    #[test]
    fn auto_update_allowed_beta_false() {
        assert!(!auto_update_allowed(UpdateChannel::Beta));
    }

    #[test]
    fn auto_update_allowed_recovery_critical_false() {
        assert!(!auto_update_allowed(UpdateChannel::RecoveryCritical));
    }

    #[test]
    fn auto_update_allowed_deprecated_false() {
        assert!(!auto_update_allowed(UpdateChannel::DeprecatedRetention));
    }

    #[test]
    fn stable_in_window_true() {
        let window = make_window(0, 24);
        assert!(stable_auto_update_permitted(&window, now_h(12)));
    }

    #[test]
    fn stable_outside_window_false() {
        let window = make_window(0, 24);
        assert!(!stable_auto_update_permitted(&window, now_h(25)));
    }

    #[test]
    fn stable_at_boundary() {
        let window = make_window(0, 24);
        assert!(stable_auto_update_permitted(&window, now_h(0)));
        assert!(stable_auto_update_permitted(&window, now_h(24)));
    }

    #[test]
    fn beta_to_stable_requires_reissue() {
        assert!(requires_reissue_for_channel_change(
            UpdateChannel::Beta,
            UpdateChannel::Stable
        ));
    }

    #[test]
    fn stable_to_stable_no_reissue() {
        assert!(!requires_reissue_for_channel_change(
            UpdateChannel::Stable,
            UpdateChannel::Stable
        ));
    }

    #[test]
    fn stable_to_beta_is_widening() {
        assert!(channel_widening_requires_approval(
            UpdateChannel::Stable,
            UpdateChannel::Beta
        ));
    }

    #[test]
    fn beta_to_stable_not_widening() {
        assert!(!channel_widening_requires_approval(
            UpdateChannel::Beta,
            UpdateChannel::Stable
        ));
    }

    #[test]
    fn recovery_critical_on_recovery_repo_ok() {
        assert!(validate_channel_for_repo(
            UpdateChannel::RecoveryCritical,
            RepositoryKind::AiosRecoveryRepo
        )
        .is_ok());
    }

    #[test]
    fn recovery_critical_on_verified_repo_err() {
        let result = validate_channel_for_repo(
            UpdateChannel::RecoveryCritical,
            RepositoryKind::AiosVerifiedRepo,
        );
        assert!(result.is_err());
    }

    #[test]
    fn stable_on_verified_repo_ok() {
        assert!(
            validate_channel_for_repo(UpdateChannel::Stable, RepositoryKind::AiosVerifiedRepo)
                .is_ok()
        );
    }

    #[test]
    fn recovery_critical_requires_recovery_true() {
        assert!(recovery_critical_requires_recovery(
            UpdateChannel::RecoveryCritical
        ));
    }

    #[test]
    fn stable_does_not_require_recovery() {
        assert!(!recovery_critical_requires_recovery(UpdateChannel::Stable));
    }

    #[test]
    fn beta_does_not_require_recovery() {
        assert!(!recovery_critical_requires_recovery(UpdateChannel::Beta));
    }
}
