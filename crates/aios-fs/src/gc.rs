//! Garbage collection pass driver for S1.3 §7.3.

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::chunk::ChunkId;
use crate::error::FsError;
use crate::evidence_emit::FsEvidenceEmitter;
use crate::fs_trait::AiosFs;
use crate::id::fresh_prefixed_ulid;
use crate::quarantine::MutableAiosFs;
use crate::version::VersionId;

const DEFAULT_MAX_CHUNKS_PER_PASS: usize = 1024;
const DEFAULT_MAX_VERSIONS_PER_PASS: usize = 1024;

/// Driver configuration for one bounded GC pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct GcPassDriver {
    /// Maximum zero-ref chunks to inspect and reclaim in one pass.
    pub max_chunks_per_pass: usize,
    /// Maximum retired versions to inspect and purge in one pass.
    pub max_versions_per_pass: usize,
    /// Optional evidence emitter for successful GC passes.
    #[serde(skip)]
    pub evidence_emitter: Option<Arc<FsEvidenceEmitter>>,
}

impl PartialEq for GcPassDriver {
    fn eq(&self, other: &Self) -> bool {
        self.max_chunks_per_pass == other.max_chunks_per_pass
            && self.max_versions_per_pass == other.max_versions_per_pass
    }
}

impl Eq for GcPassDriver {}

impl GcPassDriver {
    /// Construct a driver with explicit per-pass bounds.
    #[must_use]
    pub const fn new(max_chunks_per_pass: usize, max_versions_per_pass: usize) -> Self {
        Self {
            max_chunks_per_pass,
            max_versions_per_pass,
            evidence_emitter: None,
        }
    }

    /// Construct a driver with explicit per-pass bounds and evidence emission enabled.
    #[must_use]
    pub const fn with_evidence_emitter(
        max_chunks_per_pass: usize,
        max_versions_per_pass: usize,
        evidence_emitter: Arc<FsEvidenceEmitter>,
    ) -> Self {
        Self {
            max_chunks_per_pass,
            max_versions_per_pass,
            evidence_emitter: Some(evidence_emitter),
        }
    }

    /// Construct a driver with conservative in-memory defaults.
    #[must_use]
    pub const fn new_with_defaults() -> Self {
        Self::new(DEFAULT_MAX_CHUNKS_PER_PASS, DEFAULT_MAX_VERSIONS_PER_PASS)
    }

    /// Run one bounded garbage-collection pass.
    ///
    /// # Errors
    ///
    /// Propagates backend catalog errors raised while purging retired versions
    /// or reclaiming zero-ref chunks.
    #[allow(
        clippy::implied_bounds_in_impls,
        clippy::unused_async,
        reason = "T-039 keeps the explicit AiosFs + MutableAiosFs signature requested by the task"
    )]
    pub async fn run_pass(
        &self,
        fs: &(impl AiosFs + MutableAiosFs),
    ) -> Result<GcPassReport, FsError> {
        let pass_id = new_gc_pass_id();
        let started_at = Utc::now();
        let mut reasons = Vec::new();

        let version_ids = fs.retired_version_ids_for_gc(self.max_versions_per_pass)?;
        let versions_inspected = count_len(version_ids.len());
        let mut versions_purged = 0;

        for version_id in version_ids {
            match fs.purge_version(&version_id, VersionPurgeReason::Retired) {
                Ok(_decremented_chunks) => {
                    versions_purged += 1;
                    reasons.push(GcReason::VersionPurged {
                        version_id,
                        reason: VersionPurgeReason::Retired,
                    });
                }
                Err(FsError::VersionAlreadyPurged(_)) => {}
                Err(err) => return Err(err),
            }
        }

        let chunk_ids = fs.zero_ref_chunk_ids_for_gc(self.max_chunks_per_pass)?;
        let chunks_inspected = count_len(chunk_ids.len());
        let mut chunks_reclaimed = 0;

        for chunk_id in chunk_ids {
            match fs.reclaim_chunk(&chunk_id) {
                Ok(()) => {
                    chunks_reclaimed += 1;
                    reasons.push(GcReason::OrphanChunkReclaimed { chunk_id });
                }
                Err(FsError::ChunkStillReferenced { .. } | FsError::ChunkUnknown(_)) => {}
                Err(err) => return Err(err),
            }
        }

        let report = GcPassReport {
            pass_id,
            started_at,
            completed_at: Utc::now(),
            chunks_inspected,
            chunks_reclaimed,
            versions_inspected,
            versions_purged,
            reasons,
        };

        if let Some(emitter) = &self.evidence_emitter {
            emitter.emit_gc_pass(&report).await?;
        }

        Ok(report)
    }
}

/// Result summary for one GC pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct GcPassReport {
    /// Fresh pass id: `"gcp_<ULID>"`.
    pub pass_id: String,
    /// Wall-clock start timestamp.
    pub started_at: DateTime<Utc>,
    /// Wall-clock completion timestamp.
    pub completed_at: DateTime<Utc>,
    /// Number of zero-ref chunk candidates inspected.
    pub chunks_inspected: u64,
    /// Number of chunks removed from storage.
    pub chunks_reclaimed: u64,
    /// Number of retired version candidates inspected.
    pub versions_inspected: u64,
    /// Number of versions whose chunk refs were decremented by this pass.
    pub versions_purged: u64,
    /// Per-entity reasons recorded by the pass.
    pub reasons: Vec<GcReason>,
}

/// Per-entity GC action reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum GcReason {
    /// A zero-ref chunk was reclaimed.
    OrphanChunkReclaimed {
        /// Reclaimed chunk id.
        chunk_id: ChunkId,
    },
    /// A version was purged and its chunk refs were decremented.
    VersionPurged {
        /// Purged version id.
        version_id: VersionId,
        /// Version purge cause.
        reason: VersionPurgeReason,
    },
}

/// Version purge cause observed by GC/refcount accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VersionPurgeReason {
    /// Version is retired and no longer contributes active chunk references.
    Retired,
    /// Version was quarantined and resolved by purge policy.
    Quarantined,
    /// Operator explicitly requested the purge.
    OperatorRequested,
}

fn new_gc_pass_id() -> String {
    fresh_prefixed_ulid("gcp_")
}

fn count_len(len: usize) -> u64 {
    u64::try_from(len).unwrap_or(u64::MAX)
}
