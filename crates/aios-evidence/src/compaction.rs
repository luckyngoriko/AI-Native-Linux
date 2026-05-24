//! Retention-class compaction worker (T-015, S3.1 §11.5 / §12 / §13).
//!
//! Periodic pass that consumes [`crate::SealedSegment::is_compaction_eligible`]
//! (T-012) and actually removes the eligible content per the retention-class
//! rules of S3.1 §12, gated by the §11.5 operator-approval discipline.
//!
//! ## Policy modes
//!
//! Three closed [`CompactionPolicy`] modes (`auto`, `operator_approval`,
//! `disabled`) cover the operational spectrum the spec distinguishes:
//!
//! - **`auto`** — eligible segments are compacted immediately on the next
//!   `tick`. Suitable for dev/test/ephemeral deployments only. Production
//!   evidence logs must NOT run in this mode.
//! - **`operator_approval`** — the worker emits a `COMPACTION_APPROVAL_REQUIRED`
//!   evidence record on the first tick where a segment becomes eligible and
//!   then waits for an explicit out-of-band approval (recorded via
//!   [`CompactionWorker::approve_segment`]) before proceeding. Production
//!   default per §11.5.
//! - **`disabled`** — no compaction happens. Eligible segments accumulate. The
//!   archival deployment where every retention class collapses into "keep
//!   forever in practice".
//!
//! ## What compaction does and does not do
//!
//! Per S3.1 §12 compaction **may** remove the body of receipts that have
//! aged past their retention class **but must never**:
//!
//! - delete the receipt identity (`receipt_id`),
//! - rewrite past decisions or results,
//! - remove denials or failures (those carry the FOREVER retention class and
//!   therefore are never eligible),
//! - break the hash chain (the sealed-segment metadata is the chain witness,
//!   not the receipts themselves),
//! - modify sealed segments (their seal hash and signature stay intact).
//!
//! This worker enforces those rules by:
//!
//! 1. **never** touching segments whose retention class is
//!    [`crate::RetentionClass::Forever`] — `is_compaction_eligible` returns
//!    `false` for those segments at every wall-clock,
//! 2. preserving the sealed-segment metadata row in storage (the chain header
//!    survives forever; only the receipt rows that aged out are removed),
//! 3. pruning the warm-tier `record_type_index` rows that reference the
//!    removed receipts (orphan index entries would break `Query` reads).
//!
//! ## Cold-tier offload (out of scope for T-015)
//!
//! S3.1 §7.4 contemplates a future cold-tier where compacted segments live in
//! alternative blob storage (S3-compatible or similar). The current worker
//! removes the receipts from the warm `receipts` column family directly; a
//! later sub-task will plug an `Option<&dyn ColdTierBackend>` into
//! [`CompactionBackend::compact_segment`] so the rows are copied to cold
//! storage before deletion. The trait signature is already future-proof: it
//! returns the counts the caller wants for the [`CompactionReport`] and is
//! free to do the cold-tier write internally.
//!
//! ## Running the worker in production
//!
//! T-015 ships the synchronous primitive [`CompactionWorker::tick`]. The
//! production binary M3+ wraps this in a `tokio::time::interval` task:
//!
//! ```ignore
//! let worker = CompactionWorker::new(CompactionPolicy::OperatorApproval);
//! let backend = std::sync::Arc::new(tokio::sync::Mutex::new(backend));
//! tokio::spawn(async move {
//!     let mut tick = tokio::time::interval(std::time::Duration::from_secs(3_600));
//!     loop {
//!         tick.tick().await;
//!         let mut guard = backend.lock().await;
//!         if let Err(e) = worker.tick(&mut *guard, chrono::Utc::now()) {
//!             tracing::error!(error = ?e, "compaction tick failed");
//!         }
//!     }
//! });
//! ```
//!
//! The synchronous body is intentional: it makes the worker trivially testable
//! (the integration test for §12 just advances `now` by 3 years and calls
//! `tick` directly) and keeps the backend's internal locking the single source
//! of truth for concurrency.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::EvidenceError;
use crate::record::RetentionClass;
use crate::segment::SegmentId;

// =====================================================================
// CompactionPolicy
// =====================================================================

/// Closed S3.1 §11.5 compaction discipline modes.
///
/// Wire names match the spec's lowercase tokens
/// (`auto`, `operator_approval`, `disabled`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum CompactionPolicy {
    /// Compaction auto-runs when an eligible segment is detected.
    ///
    /// Suitable for dev/test/ephemeral deployments. **NEVER suitable for
    /// production evidence logs** — the §11.5 operator-approval contract
    /// requires explicit human authorisation before constitutional records
    /// are removed.
    #[serde(rename = "auto")]
    Auto,

    /// Compaction requires explicit operator approval per segment.
    ///
    /// The worker emits a [`crate::RecordType::CompactionApprovalRequired`]
    /// record on the first tick where a segment becomes eligible, queues
    /// that segment internally, and waits for an externally-emitted approval
    /// (recorded via [`CompactionWorker::approve_segment`]) before
    /// proceeding on the next tick. Production default.
    #[default]
    #[serde(rename = "operator_approval")]
    OperatorApproval,

    /// Compaction is disabled entirely; eligible segments accumulate.
    ///
    /// Suitable for archival deployments where everything is FOREVER in
    /// practice. The worker still walks the chain on `tick` so the report
    /// counts the eligible-but-skipped segments — operators can use the
    /// count to detect a misconfiguration.
    #[serde(rename = "disabled")]
    Disabled,
}

impl CompactionPolicy {
    /// Stable lowercase wire token, matching the serde rename.
    #[must_use]
    pub const fn as_wire_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::OperatorApproval => "operator_approval",
            Self::Disabled => "disabled",
        }
    }
}

// =====================================================================
// CompactionReport
// =====================================================================

/// Per-tick summary of what the worker did.
///
/// Operator-visible (serialised into telemetry) so every field carries the
/// `#[serde]` rename that matches the §20 metrics dictionary where one
/// exists. T-015 ships the struct shape; the §20 metric emission itself is
/// a future task (the production binary owns the metric registry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CompactionReport {
    /// Sealed segments whose retention horizon has passed (the universe
    /// `tick` walked, before any policy filtering).
    pub eligible_segments: usize,
    /// Sealed segments actually compacted on this tick.
    ///
    /// Equal to `eligible_segments` under `Auto`; equal to the count of
    /// previously-approved-and-still-eligible segments under
    /// `OperatorApproval`; always zero under `Disabled`.
    pub compacted_segments: usize,
    /// Sealed segments that were eligible AND require operator approval
    /// AND have not yet been approved.
    ///
    /// Zero under `Auto` (no approval required) and `Disabled` (the worker
    /// short-circuits before emitting approval requests).
    pub awaiting_approval: usize,
    /// Total receipts whose rows were removed from the warm `receipts`
    /// column family on this tick.
    pub receipts_removed: usize,
    /// Total `record_type_index` rows pruned on this tick — must equal
    /// `receipts_removed` in a healthy index (every receipt has exactly one
    /// `record_type_index` row).
    pub index_entries_pruned: usize,
}

// =====================================================================
// CompactionBackend — the storage trait
// =====================================================================

/// Backend-specific compaction operations.
///
/// The [`CompactionWorker`] is backend-agnostic; it calls these methods to
/// actually inspect and mutate persistent state. Both the in-memory
/// reference backend and the production `RocksDB` backend implement this
/// trait with identical semantics so the worker behaves the same under
/// either harness.
///
/// ## Invariants the impl must preserve
///
/// 1. `list_sealed_segments` returns segments in **chain order** (oldest
///    first). The worker iterates this list once per tick; out-of-order
///    iteration would produce non-deterministic [`CompactionReport`] values.
/// 2. `compact_segment` MUST:
///    - remove every receipt row belonging to the segment from the warm
///      `receipts` CF / map,
///    - remove every `record_type_index` row that points at those receipts,
///    - preserve the sealed-segment metadata row in the `segments` CF / map
///      (the chain header survives forever — §11.5 / §12 rule "must not
///      break the hash chain"),
///    - be atomic from the caller's perspective (one `WriteBatch` for the
///      `RocksDB` backend; one `&mut self` mutation for the in-memory backend).
/// 3. `emit_approval_required` MUST append a real
///    [`crate::RecordType::CompactionApprovalRequired`] evidence record
///    through the backend's normal append pipeline (BLAKE3 + JCS + Ed25519
///    signature, chain-linked) — it MUST NOT short-circuit the cryptographic
///    discipline. The record carries the subject id of the worker's
///    constitutional signer (`_system:service:evidence-compaction-worker`)
///    and the segment id in its payload.
pub trait CompactionBackend {
    /// List every sealed segment in chain order with the data the worker
    /// needs to decide eligibility:
    ///
    /// - `SegmentId` — opaque identity passed to `compact_segment`,
    /// - `RetentionClass` — the per-segment class the worker compares
    ///   against the spec's retention horizons,
    /// - `DateTime<Utc>` — the segment's `sealed_at` timestamp.
    ///
    /// # Errors
    ///
    /// Implementation-defined backend I/O errors are passed through. The
    /// worker itself never produces an error from this call.
    fn list_sealed_segments(
        &self,
    ) -> Result<Vec<(SegmentId, RetentionClass, DateTime<Utc>)>, EvidenceError>;

    /// Remove all receipts belonging to a sealed segment from the warm
    /// `receipts` CF AND prune their `record_type_index` entries.
    ///
    /// The sealed-segment metadata row (in the `segments` CF) MUST be
    /// preserved — its seal hash and signature are the chain witness that
    /// keeps the receipts' identities discoverable as tombstones per §12.
    ///
    /// Returns `(receipts_removed, index_entries_pruned)`.
    ///
    /// # Errors
    ///
    /// - The segment id is unknown to the backend.
    /// - The backend fails to write the deletion batch.
    fn compact_segment(&mut self, segment_id: &SegmentId) -> Result<(usize, usize), EvidenceError>;

    /// Emit a [`crate::RecordType::CompactionApprovalRequired`] evidence
    /// record for the given segment.
    ///
    /// The record is appended via the backend's normal Append pipeline so
    /// it gets a content hash, Ed25519 signature, and chain link. The
    /// receipt subject is the worker's constitutional id
    /// (`_system:service:evidence-compaction-worker`); the segment id
    /// appears in the receipt's opaque payload alongside the
    /// retention-class and sealed-at timestamp.
    ///
    /// # Errors
    ///
    /// - Append-path failure (chain broken, signature failure, encoding).
    fn emit_approval_required(&mut self, segment_id: &SegmentId) -> Result<(), EvidenceError>;
}

// =====================================================================
// CompactionWorker
// =====================================================================

/// Retention-class compaction worker.
///
/// Owns the policy and the in-memory bookkeeping that powers the operator
/// approval queue. The worker holds no reference to the backend so multiple
/// workers can share a backend (or a single worker can be driven over
/// multiple distinct backends in a test harness).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionWorker {
    policy: CompactionPolicy,
    /// Segments that have been observed as eligible and reported via
    /// `emit_approval_required`, but for which no approval has yet been
    /// recorded. Used under `OperatorApproval` to avoid emitting duplicate
    /// approval-required records every tick.
    awaiting_approval: HashSet<SegmentId>,
    /// Segments for which an external approval has been recorded via
    /// [`Self::approve_segment`]. On the next `tick` they will actually be
    /// compacted (and removed from this set when they are).
    approved: HashSet<SegmentId>,
}

impl CompactionWorker {
    /// Construct a fresh worker.
    ///
    /// Production passes [`CompactionPolicy::OperatorApproval`] (the
    /// constitutional default per §11.5); tests pass whichever mode they
    /// exercise.
    #[must_use]
    pub fn new(policy: CompactionPolicy) -> Self {
        Self {
            policy,
            awaiting_approval: HashSet::new(),
            approved: HashSet::new(),
        }
    }

    /// Active compaction policy.
    #[must_use]
    pub const fn policy(&self) -> CompactionPolicy {
        self.policy
    }

    /// Approve a specific segment for compaction.
    ///
    /// Only meaningful under [`CompactionPolicy::OperatorApproval`]. Under
    /// `Auto` the call is a no-op (compaction happens on the next tick
    /// regardless); under `Disabled` the call is recorded but `tick` will
    /// still skip the segment.
    ///
    /// Approving an unknown segment is allowed: the approval simply sits
    /// in the set until that segment becomes eligible. This matches the
    /// out-of-band reality where the operator approves before the
    /// retention horizon strictly elapses.
    ///
    /// # Errors
    ///
    /// Never returns an error today; the signature is `Result` to keep
    /// the API stable when production wires audit logging into the
    /// approval path.
    pub fn approve_segment(&mut self, segment_id: &SegmentId) -> Result<(), EvidenceError> {
        self.approved.insert(segment_id.clone());
        // An approved segment is no longer in the "still waiting" set,
        // even if it was previously reported.
        self.awaiting_approval.remove(segment_id);
        Ok(())
    }

    /// How many segments are currently queued awaiting approval. Operator
    /// telemetry / dashboard signal.
    #[must_use]
    pub fn awaiting_approval_count(&self) -> usize {
        self.awaiting_approval.len()
    }

    /// How many approvals have been recorded but not yet consumed by a
    /// tick.
    #[must_use]
    pub fn approved_pending_count(&self) -> usize {
        self.approved.len()
    }

    /// Run one compaction cycle.
    ///
    /// 1. Walk the segment chain via
    ///    [`CompactionBackend::list_sealed_segments`].
    /// 2. For each sealed segment, evaluate eligibility against the
    ///    retention class and the supplied wall-clock `now`.
    /// 3. Apply the policy:
    ///    - `Auto`: compact every eligible segment immediately.
    ///    - `OperatorApproval`: if already approved, compact; otherwise
    ///      emit an approval-required record (once per segment) and queue
    ///      it.
    ///    - `Disabled`: never compact; just count the eligibility for
    ///      telemetry.
    /// 4. Return a [`CompactionReport`] summarising the tick.
    ///
    /// # Errors
    ///
    /// Passes through any [`EvidenceError`] raised by the backend during
    /// the walk, the approval-required emit, or the compaction itself.
    pub fn tick<B: CompactionBackend>(
        &mut self,
        backend: &mut B,
        now: DateTime<Utc>,
    ) -> Result<CompactionReport, EvidenceError> {
        let segments = backend.list_sealed_segments()?;
        let mut report = CompactionReport::default();

        for (segment_id, retention_class, sealed_at) in segments {
            if !is_eligible(retention_class, sealed_at, now) {
                continue;
            }
            report.eligible_segments += 1;

            match self.policy {
                CompactionPolicy::Auto => {
                    let (removed, pruned) = backend.compact_segment(&segment_id)?;
                    report.compacted_segments += 1;
                    report.receipts_removed += removed;
                    report.index_entries_pruned += pruned;
                    // Whether the operator pre-approved or not, the segment
                    // is now gone — drop any bookkeeping for it.
                    self.approved.remove(&segment_id);
                    self.awaiting_approval.remove(&segment_id);
                }
                CompactionPolicy::OperatorApproval => {
                    if self.approved.contains(&segment_id) {
                        let (removed, pruned) = backend.compact_segment(&segment_id)?;
                        report.compacted_segments += 1;
                        report.receipts_removed += removed;
                        report.index_entries_pruned += pruned;
                        self.approved.remove(&segment_id);
                        self.awaiting_approval.remove(&segment_id);
                    } else if self.awaiting_approval.contains(&segment_id) {
                        // Already reported on a previous tick — keep
                        // waiting silently. Avoid emitting duplicate
                        // approval-required records every tick.
                        report.awaiting_approval += 1;
                    } else {
                        backend.emit_approval_required(&segment_id)?;
                        self.awaiting_approval.insert(segment_id.clone());
                        report.awaiting_approval += 1;
                    }
                }
                CompactionPolicy::Disabled => {
                    // Eligibility is observed and counted, but no action
                    // is taken.
                }
            }
        }

        Ok(report)
    }
}

// =====================================================================
// Helpers
// =====================================================================

/// Free-function eligibility predicate.
///
/// Mirrors the logic on [`crate::SealedSegment::is_compaction_eligible`]
/// but accepts the data the [`CompactionBackend`] surfaces directly
/// (segment id, class, sealed-at). Same horizons, same
/// `Forever`-is-never-eligible rule.
#[must_use]
pub fn is_eligible(
    retention_class: RetentionClass,
    sealed_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> bool {
    let age = now.signed_duration_since(sealed_at);
    match retention_class {
        // 24 months ≈ 730 days. Same fixed-day approximation used by
        // SealedSegment::is_compaction_eligible.
        RetentionClass::Standard24M => age >= chrono::Duration::days(730),
        // 60 months ≈ 1825 days.
        RetentionClass::Extended60M => age >= chrono::Duration::days(1825),
        // FOREVER is constitutional — never eligible.
        RetentionClass::Forever => false,
    }
}

/// Constitutional subject the compaction worker emits records under.
///
/// Production: this is the canonical id the Vault Broker (S5.2) issues a
/// signing capability for. Tests use the in-memory backend's ephemeral
/// signer; the subject string flows through unchanged.
pub const COMPACTION_WORKER_SUBJECT: &str = "_system:service:evidence-compaction-worker";

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    /// Test-only fake backend. Tracks the sealed-segment list directly and
    /// records every mutation in counters the tests assert on. The
    /// `emit_approval_required` path increments a counter rather than
    /// appending a real receipt — the integration test exercises the
    /// real append path through the `InMemoryEvidenceLog`.
    #[derive(Default)]
    struct FakeBackend {
        segments: Vec<(SegmentId, RetentionClass, DateTime<Utc>)>,
        /// Receipts per segment id, for the compact path to drain.
        receipts_per_segment: std::collections::HashMap<SegmentId, usize>,
        /// Index entries per segment id, for the compact path to drain.
        index_per_segment: std::collections::HashMap<SegmentId, usize>,
        /// Segments compacted, in order.
        compacted: Vec<SegmentId>,
        /// Approval-required emits, in order.
        approvals_emitted: Vec<SegmentId>,
    }

    impl FakeBackend {
        fn add_segment(
            &mut self,
            id: &str,
            class: RetentionClass,
            sealed_at: DateTime<Utc>,
            receipts: usize,
        ) -> SegmentId {
            let sid = SegmentId::from_content(id.as_bytes());
            self.segments.push((sid.clone(), class, sealed_at));
            self.receipts_per_segment.insert(sid.clone(), receipts);
            self.index_per_segment.insert(sid.clone(), receipts);
            sid
        }
    }

    impl CompactionBackend for FakeBackend {
        fn list_sealed_segments(
            &self,
        ) -> Result<Vec<(SegmentId, RetentionClass, DateTime<Utc>)>, EvidenceError> {
            Ok(self.segments.clone())
        }

        fn compact_segment(
            &mut self,
            segment_id: &SegmentId,
        ) -> Result<(usize, usize), EvidenceError> {
            let removed = self
                .receipts_per_segment
                .remove(segment_id)
                .unwrap_or_default();
            let pruned = self
                .index_per_segment
                .remove(segment_id)
                .unwrap_or_default();
            self.compacted.push(segment_id.clone());
            Ok((removed, pruned))
        }

        fn emit_approval_required(&mut self, segment_id: &SegmentId) -> Result<(), EvidenceError> {
            self.approvals_emitted.push(segment_id.clone());
            Ok(())
        }
    }

    fn an_hour_ago(days_old: i64) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::days(days_old) - chrono::Duration::hours(1)
    }

    #[test]
    fn compaction_policy_wire_strings_match_spec() {
        assert_eq!(CompactionPolicy::Auto.as_wire_str(), "auto");
        assert_eq!(
            CompactionPolicy::OperatorApproval.as_wire_str(),
            "operator_approval"
        );
        assert_eq!(CompactionPolicy::Disabled.as_wire_str(), "disabled");
        // serde round-trip
        for p in [
            CompactionPolicy::Auto,
            CompactionPolicy::OperatorApproval,
            CompactionPolicy::Disabled,
        ] {
            let s = serde_json::to_string(&p).expect("ser");
            let back: CompactionPolicy = serde_json::from_str(&s).expect("de");
            assert_eq!(back, p);
        }
    }

    #[test]
    fn compaction_policy_default_is_operator_approval() {
        // §11.5 default: production must require explicit approval before
        // removing constitutional records.
        assert_eq!(
            CompactionPolicy::default(),
            CompactionPolicy::OperatorApproval
        );
    }

    #[test]
    fn is_eligible_forever_is_never_eligible() {
        // Even a million-day-old FOREVER segment is not eligible.
        let one_million_days_ago = Utc::now() - chrono::Duration::days(1_000_000);
        assert!(!is_eligible(
            RetentionClass::Forever,
            one_million_days_ago,
            Utc::now()
        ));
    }

    #[test]
    fn is_eligible_standard_24m_horizon_is_730_days() {
        let sealed_at = Utc::now() - chrono::Duration::days(729);
        assert!(!is_eligible(
            RetentionClass::Standard24M,
            sealed_at,
            Utc::now()
        ));
        let sealed_at = Utc::now() - chrono::Duration::days(731);
        assert!(is_eligible(
            RetentionClass::Standard24M,
            sealed_at,
            Utc::now()
        ));
    }

    #[test]
    fn is_eligible_extended_60m_horizon_is_1825_days() {
        let sealed_at = Utc::now() - chrono::Duration::days(1824);
        assert!(!is_eligible(
            RetentionClass::Extended60M,
            sealed_at,
            Utc::now()
        ));
        let sealed_at = Utc::now() - chrono::Duration::days(1826);
        assert!(is_eligible(
            RetentionClass::Extended60M,
            sealed_at,
            Utc::now()
        ));
    }

    #[test]
    fn tick_auto_mode_compacts_eligible_segment_immediately() {
        let mut backend = FakeBackend::default();
        let eligible =
            backend.add_segment("seg-a", RetentionClass::Standard24M, an_hour_ago(800), 5);
        // Plus an ineligible recent one and a FOREVER one — neither must be touched.
        backend.add_segment("seg-b", RetentionClass::Standard24M, an_hour_ago(7), 3);
        backend.add_segment("seg-c", RetentionClass::Forever, an_hour_ago(10_000), 9);

        let mut worker = CompactionWorker::new(CompactionPolicy::Auto);
        let report = worker.tick(&mut backend, Utc::now()).expect("tick");

        assert_eq!(report.eligible_segments, 1);
        assert_eq!(report.compacted_segments, 1);
        assert_eq!(report.awaiting_approval, 0);
        assert_eq!(report.receipts_removed, 5);
        assert_eq!(report.index_entries_pruned, 5);
        assert_eq!(backend.compacted, vec![eligible]);
        // No approval-required records emitted in Auto mode.
        assert!(backend.approvals_emitted.is_empty());
    }

    #[test]
    fn tick_operator_approval_emits_approval_required_first_then_waits() {
        let mut backend = FakeBackend::default();
        let eligible =
            backend.add_segment("seg-x", RetentionClass::Standard24M, an_hour_ago(800), 4);

        let mut worker = CompactionWorker::new(CompactionPolicy::OperatorApproval);
        let first = worker.tick(&mut backend, Utc::now()).expect("first tick");

        assert_eq!(first.eligible_segments, 1);
        assert_eq!(
            first.compacted_segments, 0,
            "no compaction without approval"
        );
        assert_eq!(first.awaiting_approval, 1);
        assert_eq!(first.receipts_removed, 0);
        assert_eq!(backend.approvals_emitted, vec![eligible.clone()]);
        // Subsequent ticks without approval must NOT re-emit the request.
        let second = worker.tick(&mut backend, Utc::now()).expect("second tick");
        assert_eq!(second.awaiting_approval, 1);
        assert_eq!(second.compacted_segments, 0);
        assert_eq!(
            backend.approvals_emitted,
            vec![eligible],
            "duplicate approval-required emission would spam the chain"
        );
        assert_eq!(worker.awaiting_approval_count(), 1);
    }

    #[test]
    fn tick_operator_approval_after_approve_segment_actually_compacts() {
        let mut backend = FakeBackend::default();
        let eligible =
            backend.add_segment("seg-y", RetentionClass::Standard24M, an_hour_ago(800), 7);

        let mut worker = CompactionWorker::new(CompactionPolicy::OperatorApproval);
        let _ = worker.tick(&mut backend, Utc::now()).expect("tick 1");
        assert_eq!(worker.awaiting_approval_count(), 1);

        worker.approve_segment(&eligible).expect("approve");
        // Approval removes the segment from the awaiting set immediately.
        assert_eq!(worker.awaiting_approval_count(), 0);
        assert_eq!(worker.approved_pending_count(), 1);

        let after = worker.tick(&mut backend, Utc::now()).expect("tick 2");
        assert_eq!(after.eligible_segments, 1);
        assert_eq!(after.compacted_segments, 1);
        assert_eq!(after.receipts_removed, 7);
        assert_eq!(after.index_entries_pruned, 7);
        assert_eq!(backend.compacted, vec![eligible]);
        // Approval consumed.
        assert_eq!(worker.approved_pending_count(), 0);
    }

    #[test]
    fn tick_disabled_policy_leaves_eligible_segments_untouched() {
        let mut backend = FakeBackend::default();
        backend.add_segment("seg-d", RetentionClass::Standard24M, an_hour_ago(2_000), 10);

        let mut worker = CompactionWorker::new(CompactionPolicy::Disabled);
        let report = worker.tick(&mut backend, Utc::now()).expect("tick");

        // Eligibility is still observed for telemetry…
        assert_eq!(report.eligible_segments, 1);
        // …but nothing is removed and no approval is asked for.
        assert_eq!(report.compacted_segments, 0);
        assert_eq!(report.awaiting_approval, 0);
        assert_eq!(report.receipts_removed, 0);
        assert!(backend.compacted.is_empty());
        assert!(backend.approvals_emitted.is_empty());
    }

    #[test]
    fn report_zero_when_no_eligible_segments() {
        let mut backend = FakeBackend::default();
        backend.add_segment("recent", RetentionClass::Standard24M, an_hour_ago(7), 3);
        backend.add_segment("forever", RetentionClass::Forever, an_hour_ago(100_000), 9);

        let mut worker = CompactionWorker::new(CompactionPolicy::Auto);
        let report = worker.tick(&mut backend, Utc::now()).expect("tick");
        assert_eq!(report, CompactionReport::default());
    }

    #[test]
    fn approve_unknown_segment_is_recorded_and_waits_for_eligibility() {
        let unknown = SegmentId::from_content(b"never-existed");
        let mut worker = CompactionWorker::new(CompactionPolicy::OperatorApproval);
        worker.approve_segment(&unknown).expect("approve");
        assert_eq!(worker.approved_pending_count(), 1);

        // A tick over an empty backend touches nothing.
        let mut backend = FakeBackend::default();
        let report = worker.tick(&mut backend, Utc::now()).expect("tick");
        assert_eq!(report, CompactionReport::default());
        // Approval stays parked.
        assert_eq!(worker.approved_pending_count(), 1);
    }

    #[test]
    fn auto_mode_compacts_multiple_eligible_segments_in_one_tick() {
        let mut backend = FakeBackend::default();
        let a = backend.add_segment("a", RetentionClass::Standard24M, an_hour_ago(800), 2);
        let b = backend.add_segment("b", RetentionClass::Extended60M, an_hour_ago(2_000), 4);
        let c = backend.add_segment("c", RetentionClass::Standard24M, an_hour_ago(1_200), 6);

        let mut worker = CompactionWorker::new(CompactionPolicy::Auto);
        let report = worker.tick(&mut backend, Utc::now()).expect("tick");
        assert_eq!(report.eligible_segments, 3);
        assert_eq!(report.compacted_segments, 3);
        assert_eq!(report.receipts_removed, 2 + 4 + 6);
        assert_eq!(report.index_entries_pruned, 2 + 4 + 6);

        // Order preserved (chain order from the backend list).
        assert_eq!(backend.compacted, vec![a, b, c]);
    }

    #[test]
    fn operator_approval_handles_mixed_approved_and_unapproved_segments() {
        let mut backend = FakeBackend::default();
        let approved = backend.add_segment("ap", RetentionClass::Standard24M, an_hour_ago(800), 3);
        let pending = backend.add_segment("pe", RetentionClass::Standard24M, an_hour_ago(900), 5);

        let mut worker = CompactionWorker::new(CompactionPolicy::OperatorApproval);
        worker.approve_segment(&approved).expect("approve");

        let report = worker.tick(&mut backend, Utc::now()).expect("tick");
        assert_eq!(report.eligible_segments, 2);
        assert_eq!(report.compacted_segments, 1);
        assert_eq!(report.awaiting_approval, 1);
        assert_eq!(report.receipts_removed, 3);
        assert_eq!(backend.compacted, vec![approved]);
        assert_eq!(backend.approvals_emitted, vec![pending]);
    }

    #[test]
    fn worker_records_subject_constant() {
        // Constitutional subject id for compaction-worker emissions.
        assert_eq!(
            COMPACTION_WORKER_SUBJECT,
            "_system:service:evidence-compaction-worker"
        );
    }

    #[test]
    fn compaction_report_serialises_with_stable_field_names() {
        let r = CompactionReport {
            eligible_segments: 3,
            compacted_segments: 2,
            awaiting_approval: 1,
            receipts_removed: 17,
            index_entries_pruned: 17,
        };
        let s = serde_json::to_string(&r).expect("ser");
        assert!(s.contains("\"eligible_segments\":3"));
        assert!(s.contains("\"compacted_segments\":2"));
        assert!(s.contains("\"awaiting_approval\":1"));
        assert!(s.contains("\"receipts_removed\":17"));
        assert!(s.contains("\"index_entries_pruned\":17"));
        let back: CompactionReport = serde_json::from_str(&s).expect("de");
        assert_eq!(back, r);
    }

    #[test]
    fn operator_approval_with_pre_approved_segment_skips_emit() {
        // If the operator approves a segment BEFORE the first tick that
        // discovers it, the worker must NOT emit a redundant
        // approval-required record — go straight to compaction.
        let mut backend = FakeBackend::default();
        let pre = backend.add_segment("pre", RetentionClass::Standard24M, an_hour_ago(800), 4);

        let mut worker = CompactionWorker::new(CompactionPolicy::OperatorApproval);
        worker.approve_segment(&pre).expect("pre-approve");

        let report = worker.tick(&mut backend, Utc::now()).expect("tick");
        assert_eq!(report.compacted_segments, 1);
        assert_eq!(report.awaiting_approval, 0);
        assert!(
            backend.approvals_emitted.is_empty(),
            "pre-approved segment must not trigger an approval-required emission"
        );
    }

    #[test]
    fn report_default_is_zero_filled() {
        let r = CompactionReport::default();
        assert_eq!(r.eligible_segments, 0);
        assert_eq!(r.compacted_segments, 0);
        assert_eq!(r.awaiting_approval, 0);
        assert_eq!(r.receipts_removed, 0);
        assert_eq!(r.index_entries_pruned, 0);
    }

    #[test]
    fn worker_clone_preserves_pending_state() {
        let mut w = CompactionWorker::new(CompactionPolicy::OperatorApproval);
        let s = SegmentId::from_content(b"x");
        w.approve_segment(&s).expect("approve");
        let cloned = w.clone();
        assert_eq!(cloned.approved_pending_count(), 1);
        assert_eq!(cloned.policy(), CompactionPolicy::OperatorApproval);
    }

    #[test]
    fn auto_mode_ignores_previously_emitted_approval_state() {
        // If a worker started in OperatorApproval mode and accumulated an
        // awaiting-approval entry, then was reconstructed in Auto mode
        // (a config flip in production), Auto must compact eligible
        // segments regardless of the stale set.
        let mut backend = FakeBackend::default();
        let seg = backend.add_segment("z", RetentionClass::Standard24M, an_hour_ago(800), 2);

        let mut w = CompactionWorker::new(CompactionPolicy::Auto);
        // Simulate carried-over state from a previous OperatorApproval run.
        w.awaiting_approval.insert(seg.clone());

        let report = w.tick(&mut backend, Utc::now()).expect("tick");
        assert_eq!(report.compacted_segments, 1);
        assert_eq!(backend.compacted, vec![seg]);
    }
}
