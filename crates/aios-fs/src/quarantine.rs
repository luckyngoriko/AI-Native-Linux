//! Quarantine entry/exit driver for S1.3 §12.

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::error::FsError;
use crate::fs_trait::AiosFs;
use crate::id::fresh_prefixed_ulid;
use crate::object::SubjectRef;
use crate::version::VersionId;

/// Categories that can drive a version into `QUARANTINED` per S1.3 §12.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, EnumCount)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuarantineTrigger {
    /// Validation failure during write, such as schema or signature mismatch.
    ValidationFailure,
    /// Integrity check failure, such as content hash mismatch.
    IntegrityFailure,
    /// Post-commit policy violation.
    PolicyViolation,
    /// External attestation failure.
    AttestationFailure,
    /// Operator-initiated manual quarantine.
    OperatorManual,
}

/// Operator disposition applied when a version exits quarantine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuarantineDisposition {
    /// Release the version back to normal verified availability.
    Released,
    /// Resolve quarantine by purging/retiring the version.
    Purged,
}

/// Receipt returned for quarantine entry and exit transitions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct QuarantineReceipt {
    /// Fresh transition receipt id.
    pub quarantine_id: String,
    /// Version affected by the transition.
    pub version_id: VersionId,
    /// Timestamp at which the transition was applied.
    pub transitioned_at: DateTime<Utc>,
    /// Entry trigger; `None` for exit receipts.
    pub trigger: Option<QuarantineTrigger>,
    /// Exit disposition; `None` for entry receipts.
    pub disposition: Option<QuarantineDisposition>,
    /// Entry reason or exit operator note.
    pub reason: String,
}

/// Additive mutation surface used by [`QuarantineDriver`] without changing
/// the frozen T-037 [`AiosFs`] trait.
pub trait MutableAiosFs: AiosFs {
    /// Apply a quarantine entry transition atomically.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::VersionNotFound`] for an unknown version,
    /// [`FsError::QuarantineAlreadyApplied`] for an already-quarantined
    /// version, and [`FsError::NoPriorStablePointer`] when affected
    /// `CURRENT`/`STABLE` pointers have no rollback or prior stable target.
    fn apply_quarantine_entry(
        &self,
        version_id: &VersionId,
        trigger: QuarantineTrigger,
        reason: &str,
    ) -> Result<QuarantineReceipt, FsError>;

    /// Apply a quarantine exit transition atomically.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::VersionNotFound`] for an unknown version and
    /// [`FsError::QuarantineNotApplied`] when the version is not quarantined.
    fn apply_quarantine_exit(
        &self,
        version_id: &VersionId,
        disposition: QuarantineDisposition,
        operator: &SubjectRef,
    ) -> Result<QuarantineReceipt, FsError>;
}

/// Driver for S1.3 §12 quarantine entry/exit transitions.
#[derive(Debug, Clone)]
pub struct QuarantineDriver<F> {
    fs: F,
}

impl<F> QuarantineDriver<F>
where
    F: MutableAiosFs,
{
    /// Construct a driver bound to a mutable filesystem handle.
    #[must_use]
    pub const fn new(fs: F) -> Self {
        Self { fs }
    }

    /// Enter quarantine and apply the §12.2 pointer move rule.
    ///
    /// # Errors
    ///
    /// Propagates the backend mutation errors documented on
    /// [`MutableAiosFs::apply_quarantine_entry`].
    #[allow(
        clippy::unused_async,
        reason = "T-038 exposes async driver calls to match the AiosFs async surface"
    )]
    pub async fn enter<M>(
        &self,
        version_id: &VersionId,
        trigger: QuarantineTrigger,
        reason: &str,
        fs: &M,
    ) -> Result<QuarantineReceipt, FsError>
    where
        M: MutableAiosFs + ?Sized,
    {
        fs.apply_quarantine_entry(version_id, trigger, reason)
    }

    /// Exit quarantine with the supplied operator disposition.
    ///
    /// # Errors
    ///
    /// Propagates the backend mutation errors documented on
    /// [`MutableAiosFs::apply_quarantine_exit`].
    #[allow(
        clippy::unused_async,
        reason = "T-038 exposes async driver calls to match the AiosFs async surface"
    )]
    pub async fn exit(
        &self,
        version_id: &VersionId,
        disposition: QuarantineDisposition,
        operator: &SubjectRef,
    ) -> Result<QuarantineReceipt, FsError> {
        self.fs
            .apply_quarantine_exit(version_id, disposition, operator)
    }
}

pub(crate) fn new_quarantine_id() -> String {
    fresh_prefixed_ulid("qnt_")
}
