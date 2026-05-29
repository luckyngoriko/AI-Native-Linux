//! Install pipeline FSM — closed state transition matrix per S11.1 §3.6.
//!
//! [`can_transition`] implements every forward transition defined in the spec.
//! [`apply`] is the single mutation surface: it checks `can_transition` and
//! either mutates the state or returns [`DistributionErrorCode::InstallStateInvalidTransition`].
//!
//! Terminal states (`Removed`, `InstallFailed`) have no outgoing transitions.
//! `Active` and `Quarantined` are NOT terminal per spec §3.6.

use crate::error::DistributionError;

#[cfg(test)]
use crate::error::DistributionErrorCode;
use crate::install_state::PackageInstallState;

/// Returns `true` iff the FSM permits a transition from `from` to `to`.
///
/// # Allowed forward transitions (S11.1 §3.6)
///
/// ```text
/// Draft → Validating
/// Validating → AwaitingApproval | InstallFailed
/// AwaitingApproval → Approved | InstallFailed
/// Approved → Installing
/// Installing → Active | InstallFailed
/// Active → Quarantined | Uninstalling
/// Quarantined → Uninstalling | Validating   (operator removal + contest path)
/// Uninstalling → Removed
/// Removed → (none, terminal)
/// InstallFailed → (none, terminal)
/// ```
///
/// All other `(from, to)` pairs return `false`.
#[must_use]
#[allow(clippy::match_same_arms)]
pub const fn can_transition(from: PackageInstallState, to: PackageInstallState) -> bool {
    match (from, to) {
        // §3.6 forward paths
        (PackageInstallState::Draft, PackageInstallState::Validating) => true,

        (PackageInstallState::Validating, PackageInstallState::AwaitingApproval) => true,
        (PackageInstallState::Validating, PackageInstallState::InstallFailed) => true,

        (PackageInstallState::AwaitingApproval, PackageInstallState::Approved) => true,
        (PackageInstallState::AwaitingApproval, PackageInstallState::InstallFailed) => true,

        (PackageInstallState::Approved, PackageInstallState::Installing) => true,

        (PackageInstallState::Installing, PackageInstallState::Active) => true,
        (PackageInstallState::Installing, PackageInstallState::InstallFailed) => true,

        (PackageInstallState::Active, PackageInstallState::Quarantined) => true,
        (PackageInstallState::Active, PackageInstallState::Uninstalling) => true,

        // §3.6: Quarantined → Uninstalling (operator removes), Quarantined → Validating (contest/re-validation)
        (PackageInstallState::Quarantined, PackageInstallState::Uninstalling) => true,
        (PackageInstallState::Quarantined, PackageInstallState::Validating) => true,

        (PackageInstallState::Uninstalling, PackageInstallState::Removed) => true,

        // Terminal: Removed, InstallFailed → nothing
        // All other pairs → false
        _ => false,
    }
}

/// Applies a state transition via the FSM.
///
/// If `can_transition(*current, to)` is `true`, mutates `*current = to` and
/// returns `Ok(())`. Otherwise returns [`DistributionError`] with
/// [`DistributionErrorCode::InstallStateInvalidTransition`].
///
/// # Errors
///
/// Returns `Err(DistributionError::InstallStateInvalidTransition(...))` when
/// the transition is not permitted by the FSM.
pub fn apply(
    current: &mut PackageInstallState,
    to: PackageInstallState,
) -> Result<(), DistributionError> {
    if can_transition(*current, to) {
        *current = to;
        Ok(())
    } else {
        Err(DistributionError::InstallStateInvalidTransition(format!(
            "invalid transition: {} → {}",
            current.label(),
            to.label()
        )))
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

    // ── Valid forward transitions ──────────────────────────────────────────

    #[test]
    fn draft_to_validating() {
        assert!(can_transition(
            PackageInstallState::Draft,
            PackageInstallState::Validating
        ));
    }

    #[test]
    fn validating_to_awaiting_approval() {
        assert!(can_transition(
            PackageInstallState::Validating,
            PackageInstallState::AwaitingApproval
        ));
    }

    #[test]
    fn validating_to_install_failed() {
        assert!(can_transition(
            PackageInstallState::Validating,
            PackageInstallState::InstallFailed
        ));
    }

    #[test]
    fn awaiting_approval_to_approved() {
        assert!(can_transition(
            PackageInstallState::AwaitingApproval,
            PackageInstallState::Approved
        ));
    }

    #[test]
    fn awaiting_approval_to_install_failed() {
        assert!(can_transition(
            PackageInstallState::AwaitingApproval,
            PackageInstallState::InstallFailed
        ));
    }

    #[test]
    fn approved_to_installing() {
        assert!(can_transition(
            PackageInstallState::Approved,
            PackageInstallState::Installing
        ));
    }

    #[test]
    fn installing_to_active() {
        assert!(can_transition(
            PackageInstallState::Installing,
            PackageInstallState::Active
        ));
    }

    #[test]
    fn installing_to_install_failed() {
        assert!(can_transition(
            PackageInstallState::Installing,
            PackageInstallState::InstallFailed
        ));
    }

    #[test]
    fn active_to_quarantined() {
        assert!(can_transition(
            PackageInstallState::Active,
            PackageInstallState::Quarantined
        ));
    }

    #[test]
    fn active_to_uninstalling() {
        assert!(can_transition(
            PackageInstallState::Active,
            PackageInstallState::Uninstalling
        ));
    }

    #[test]
    fn quarantined_to_uninstalling() {
        assert!(can_transition(
            PackageInstallState::Quarantined,
            PackageInstallState::Uninstalling
        ));
    }

    #[test]
    fn quarantined_to_validating() {
        assert!(can_transition(
            PackageInstallState::Quarantined,
            PackageInstallState::Validating
        ));
    }

    #[test]
    fn uninstalling_to_removed() {
        assert!(can_transition(
            PackageInstallState::Uninstalling,
            PackageInstallState::Removed
        ));
    }

    // ── Invalid transitions (representative set) ───────────────────────────

    #[test]
    fn active_to_draft_invalid() {
        assert!(!can_transition(
            PackageInstallState::Active,
            PackageInstallState::Draft
        ));
    }

    #[test]
    fn removed_to_anything_invalid() {
        for target in &[
            PackageInstallState::Draft,
            PackageInstallState::Validating,
            PackageInstallState::Active,
            PackageInstallState::Uninstalling,
        ] {
            assert!(
                !can_transition(PackageInstallState::Removed, *target),
                "Removed → {} should be invalid",
                target.label()
            );
        }
    }

    #[test]
    fn install_failed_to_anything_invalid() {
        for target in &[
            PackageInstallState::Draft,
            PackageInstallState::Validating,
            PackageInstallState::Active,
            PackageInstallState::Approved,
        ] {
            assert!(
                !can_transition(PackageInstallState::InstallFailed, *target),
                "InstallFailed → {} should be invalid",
                target.label()
            );
        }
    }

    #[test]
    fn draft_to_active_invalid() {
        assert!(!can_transition(
            PackageInstallState::Draft,
            PackageInstallState::Active
        ));
    }

    #[test]
    fn active_to_install_failed_invalid() {
        assert!(!can_transition(
            PackageInstallState::Active,
            PackageInstallState::InstallFailed
        ));
    }

    #[test]
    fn approved_to_active_invalid() {
        assert!(!can_transition(
            PackageInstallState::Approved,
            PackageInstallState::Active
        ));
    }

    // ── apply mutations ────────────────────────────────────────────────────

    #[test]
    fn apply_mutates_on_valid_transition() {
        let mut state = PackageInstallState::Draft;
        let result = apply(&mut state, PackageInstallState::Validating);
        assert!(result.is_ok());
        assert_eq!(state, PackageInstallState::Validating);
    }

    #[test]
    fn apply_errors_on_invalid_transition() {
        let mut state = PackageInstallState::Active;
        let result = apply(&mut state, PackageInstallState::Draft);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(
            err.code(),
            DistributionErrorCode::InstallStateInvalidTransition
        );
        // State unchanged
        assert_eq!(state, PackageInstallState::Active);
    }

    #[test]
    fn apply_removed_is_terminal() {
        let mut state = PackageInstallState::Removed;
        // Removed → Draft is invalid
        let result = apply(&mut state, PackageInstallState::Draft);
        assert!(result.is_err());
        assert_eq!(state, PackageInstallState::Removed);
    }
}
