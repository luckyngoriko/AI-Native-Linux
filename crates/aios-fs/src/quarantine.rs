//! Quarantine entry/exit driver for S1.3 §12.

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use strum_macros::{EnumCount, EnumIter};

use crate::chunk::ChunkId;
use crate::error::FsError;
use crate::evidence_emit::FsEvidenceEmitter;
use crate::fs_trait::AiosFs;
use crate::gc::VersionPurgeReason;
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

    /// Decrement one chunk reference count and return the new count.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ChunkUnknown`] when `chunk_id` is not present.
    fn decrement_chunk_refcount(&self, chunk_id: &ChunkId) -> Result<u32, FsError>;

    /// Remove one zero-ref chunk from storage.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ChunkUnknown`] when `chunk_id` is not present and
    /// [`FsError::ChunkStillReferenced`] when its reference count is non-zero.
    fn reclaim_chunk(&self, chunk_id: &ChunkId) -> Result<(), FsError>;

    /// Purge one version and decrement every referenced chunk as a side effect.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::VersionNotFound`] for an unknown version,
    /// [`FsError::VersionAlreadyPurged`] when the version was already purged,
    /// and [`FsError::ChunkUnknown`] if the version references a missing chunk.
    fn purge_version(
        &self,
        version_id: &VersionId,
        reason: VersionPurgeReason,
    ) -> Result<Vec<ChunkId>, FsError>;

    /// Return retired version ids eligible for a GC purge pass.
    ///
    /// # Errors
    ///
    /// Backends may return [`FsError::Internal`] for catalog scan failures.
    #[doc(hidden)]
    fn retired_version_ids_for_gc(&self, _max_versions: usize) -> Result<Vec<VersionId>, FsError> {
        Ok(Vec::new())
    }

    /// Return zero-ref chunk ids eligible for a GC reclaim pass.
    ///
    /// # Errors
    ///
    /// Backends may return [`FsError::Internal`] for catalog scan failures.
    #[doc(hidden)]
    fn zero_ref_chunk_ids_for_gc(&self, _max_chunks: usize) -> Result<Vec<ChunkId>, FsError> {
        Ok(Vec::new())
    }
}

/// Driver for S1.3 §12 quarantine entry/exit transitions.
#[derive(Debug, Clone)]
pub struct QuarantineDriver<F> {
    fs: F,
    evidence_emitter: Option<Arc<FsEvidenceEmitter>>,
}

impl<F> QuarantineDriver<F>
where
    F: MutableAiosFs,
{
    /// Construct a driver bound to a mutable filesystem handle.
    #[must_use]
    pub const fn new(fs: F) -> Self {
        Self {
            fs,
            evidence_emitter: None,
        }
    }

    /// Construct a driver bound to a mutable filesystem handle and evidence emitter.
    #[must_use]
    pub const fn with_evidence_emitter(fs: F, evidence_emitter: Arc<FsEvidenceEmitter>) -> Self {
        Self {
            fs,
            evidence_emitter: Some(evidence_emitter),
        }
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
        let receipt = fs.apply_quarantine_entry(version_id, trigger, reason)?;
        if let Some(emitter) = &self.evidence_emitter {
            emitter
                .emit_quarantine_enter(version_id, trigger, reason, &receipt)
                .await?;
        }
        Ok(receipt)
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
        let receipt = self
            .fs
            .apply_quarantine_exit(version_id, disposition, operator)?;
        if let Some(emitter) = &self.evidence_emitter {
            emitter
                .emit_quarantine_exit(version_id, disposition, &receipt)
                .await?;
        }
        Ok(receipt)
    }
}

pub(crate) fn new_quarantine_id() -> String {
    fresh_prefixed_ulid("qnt_")
}
