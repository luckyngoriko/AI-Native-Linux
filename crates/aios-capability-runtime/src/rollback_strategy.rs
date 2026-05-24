//! `RollbackStrategy` — closed enum per S10.1 §7.2.
//!
//! Each adapter declares per-`action_kind` how the runtime should attempt
//! rollback after a `FAILED` transition. The variants mirror the proto IDL
//! in §7.2 verbatim:
//!
//! ```proto
//! enum RollbackStrategy {
//!   ROLLBACK_STRATEGY_UNSPECIFIED = 0;
//!   NONE                          = 1;
//!   IDEMPOTENT_REVERSE            = 2;
//!   CHECKPOINT_BASED              = 3;
//!   EXTERNAL_REQUIRED             = 4;
//! }
//! ```
//!
//! Per §7.2: `EXTERNAL_REQUIRED` is treated as
//! [`crate::RollbackOutcome::NotApplicable`] from the FSM's perspective — the
//! runtime cannot roll back, the operator must — and the runtime emits a
//! `ROLLBACK_ATTEMPTED` record with `note = NOT_APPLICABLE`.
//!
//! The wire form is `SCREAMING_SNAKE_CASE`, matching the proto IDL. Decoders
//! fail closed on unknown values.
//!
//! ## Test seam — [`RollbackFailureMode`]
//!
//! Production wiring of an adapter's `Rollback(...)` RPC is M5+ scope: T-032
//! does not yet have a live adapter that performs rollback. Until then, the
//! rollback driver is exercised through [`RollbackFailureMode`], a closed
//! injection enum the test harness feeds into
//! [`crate::RollbackDriver::with_failure_mode`] to choose between simulating
//! a successful adapter rollback and simulating an adapter rollback that
//! itself fails. This is the same pattern T-029 used for the
//! `ResourceBudgetExceeded` paths before the real dispatcher landed; the
//! seam disappears in M5 once a real adapter handle owns the
//! `Rollback(...)` RPC.

use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::error::RuntimeError;

/// `RollbackStrategy` — S10.1 §7.2 closed enum, five values.
///
/// Adapters declare one [`RollbackStrategy`] per `action_kind` in their
/// [`crate::adapter_manifest::AdapterActionDeclaration::rollback_strategy`].
/// The runtime calls the adapter's `Rollback(...)` RPC after a `FAILED`
/// transition when `strategy != NONE`, then maps the returned
/// [`crate::RollbackOutcome`] onto the §4.2 table per the §7.2 outcome
/// table.
///
/// **Order matters.** The variants are declared in the same order as the
/// proto IDL in §7.2, so [`strum_macros::EnumIter`] produces a stable
/// iteration that the round-trip tests pin.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount, Default,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollbackStrategy {
    /// `ROLLBACK_STRATEGY_UNSPECIFIED` — reserved zero-value indicator for
    /// proto3 wire compatibility. The runtime treats this as
    /// [`Self::None`] when encountered in a manifest (fail-safe default).
    #[default]
    #[serde(rename = "ROLLBACK_STRATEGY_UNSPECIFIED")]
    Unspecified,
    /// `NONE` — the action is destructive without a rollback path and is
    /// **never auto-rolled back**. The runtime stays in `FAILED` on
    /// execution / verification failure.
    None,
    /// `IDEMPOTENT_REVERSE` — the adapter can compute the reverse action
    /// from the original action and current adapter state (e.g.
    /// `service.start` ↔ `service.stop`).
    IdempotentReverse,
    /// `CHECKPOINT_BASED` — the adapter took a checkpoint pre-execute and
    /// can restore it (e.g. AIOS-FS object snapshot, package db
    /// transaction).
    CheckpointBased,
    /// `EXTERNAL_REQUIRED` — rollback requires operator intervention (e.g.
    /// restore from off-site backup, physical hardware swap). The runtime
    /// cannot auto-roll-back; the §7.2 table treats this as
    /// [`crate::RollbackOutcome::NotApplicable`] and emits
    /// `ROLLBACK_ATTEMPTED` with `note = NOT_APPLICABLE`.
    ExternalRequired,
}

impl RollbackStrategy {
    /// Parse the manifest's opaque [`String`] `rollback_strategy` field into a
    /// typed [`RollbackStrategy`].
    ///
    /// The manifest holds the wire-form `SCREAMING_SNAKE_CASE` token per
    /// [`crate::adapter_manifest::AdapterActionDeclaration::rollback_strategy`].
    /// An empty string or `ROLLBACK_STRATEGY_UNSPECIFIED` parses to
    /// [`Self::Unspecified`]; an unrecognised token returns
    /// [`RuntimeError::ManifestInvalid`] (fail-closed per §10.5).
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::ManifestInvalid`] when the input is not a
    /// recognised wire-form token.
    pub fn parse_manifest_value(raw: &str) -> Result<Self, RuntimeError> {
        match raw.trim() {
            "" | "ROLLBACK_STRATEGY_UNSPECIFIED" => Ok(Self::Unspecified),
            "NONE" => Ok(Self::None),
            "IDEMPOTENT_REVERSE" => Ok(Self::IdempotentReverse),
            "CHECKPOINT_BASED" => Ok(Self::CheckpointBased),
            "EXTERNAL_REQUIRED" => Ok(Self::ExternalRequired),
            other => Err(RuntimeError::ManifestInvalid(format!(
                "unknown rollback_strategy: {other:?}"
            ))),
        }
    }

    /// `true` iff this strategy admits an auto-rollback attempt. Per §7.2:
    /// [`Self::None`] and [`Self::Unspecified`] do not; the rest do (with
    /// [`Self::ExternalRequired`] resolving to
    /// [`crate::RollbackOutcome::NotApplicable`] at attempt time).
    #[must_use]
    pub const fn admits_auto_rollback(&self) -> bool {
        matches!(
            self,
            Self::IdempotentReverse | Self::CheckpointBased | Self::ExternalRequired
        )
    }
}

/// `RollbackFailureMode` — closed test seam for
/// [`crate::RollbackDriver`].
///
/// Selects the simulated outcome of the adapter's `Rollback(...)` RPC. The
/// production wiring of a live adapter handle is M5+ scope; until then this
/// enum lets tests exercise the §7.2 outcome table deterministically.
///
/// The wire form is `SCREAMING_SNAKE_CASE` for forensic consistency with the
/// rest of the runtime, though this enum is never persisted on the
/// evidence log: it is a runtime-only configuration knob on the driver.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount, Default,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RollbackFailureMode {
    /// Simulate a successful adapter rollback —
    /// [`crate::RollbackOutcome::Succeeded`]. Drives
    /// `FAILED → ROLLED_BACK` (T19).
    #[default]
    SucceedSimulated,
    /// Simulate an adapter rollback that itself fails —
    /// [`crate::RollbackOutcome::Failed`]. Drives
    /// `FAILED → ROLLBACK_FAILED` (T20), emits FOREVER evidence, raises
    /// the operator-alert counter.
    FailSimulated,
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use strum::{EnumCount, IntoEnumIterator};

    #[test]
    fn rollback_strategy_has_five_variants() {
        assert_eq!(RollbackStrategy::COUNT, 5);
    }

    #[test]
    fn rollback_failure_mode_has_two_variants() {
        assert_eq!(RollbackFailureMode::COUNT, 2);
    }

    #[test]
    fn strategy_wire_form_roundtrip() {
        for s in RollbackStrategy::iter() {
            let wire = serde_json::to_string(&s).expect("serialize");
            let back: RollbackStrategy = serde_json::from_str(&wire).expect("deserialize");
            assert_eq!(s, back, "round-trip for {s:?} wire={wire}");
        }
    }

    #[test]
    fn parse_manifest_value_accepts_known_tokens() {
        assert_eq!(
            RollbackStrategy::parse_manifest_value("NONE").expect("parse"),
            RollbackStrategy::None
        );
        assert_eq!(
            RollbackStrategy::parse_manifest_value("IDEMPOTENT_REVERSE").expect("parse"),
            RollbackStrategy::IdempotentReverse
        );
        assert_eq!(
            RollbackStrategy::parse_manifest_value("CHECKPOINT_BASED").expect("parse"),
            RollbackStrategy::CheckpointBased
        );
        assert_eq!(
            RollbackStrategy::parse_manifest_value("EXTERNAL_REQUIRED").expect("parse"),
            RollbackStrategy::ExternalRequired
        );
        assert_eq!(
            RollbackStrategy::parse_manifest_value("ROLLBACK_STRATEGY_UNSPECIFIED").expect("parse"),
            RollbackStrategy::Unspecified
        );
        assert_eq!(
            RollbackStrategy::parse_manifest_value("").expect("parse"),
            RollbackStrategy::Unspecified
        );
    }

    #[test]
    fn parse_manifest_value_rejects_unknown_token() {
        let err = RollbackStrategy::parse_manifest_value("IDEMPOTENT_REAPPLY")
            .expect_err("unknown token must error");
        assert!(matches!(err, RuntimeError::ManifestInvalid(_)));
    }

    #[test]
    fn admits_auto_rollback_truth_table() {
        assert!(!RollbackStrategy::Unspecified.admits_auto_rollback());
        assert!(!RollbackStrategy::None.admits_auto_rollback());
        assert!(RollbackStrategy::IdempotentReverse.admits_auto_rollback());
        assert!(RollbackStrategy::CheckpointBased.admits_auto_rollback());
        assert!(RollbackStrategy::ExternalRequired.admits_auto_rollback());
    }
}
