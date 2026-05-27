//! In-memory reference backend for the gRPC `EvidenceLog` surface (T-011).
//!
//! This backend is the §22 MVP golden-path implementation: spin up a Tokio
//! runtime, instantiate [`InMemoryEvidenceLog`] with an ephemeral signing key,
//! and serve over `tonic::transport::Server`. It is the harness the
//! integration tests under `tests/grpc_roundtrip.rs` drive end-to-end.
//!
//! ## Scope
//!
//! - **`Append`** — validates record type + subject, mints a sealed
//!   [`crate::EvidenceReceipt`] linked to the chain head, persists it.
//! - **`ReadReceipt`** — id lookup; `tonic::Code::NotFound` on miss.
//! - **`Subscribe`** — server-streaming. Optional replay from a bookmark
//!   `resume_from_receipt_id`; then live updates via a tokio broadcast
//!   channel. Filters (`record_types_filter`, `subject_filter`,
//!   `correlation_id_filter`) applied per S3.1 §9.
//! - **`Query`** — server-streaming. Historic-only, bounded by the
//!   `limit` field (default 1000 per §10). Sorted by append order.
//! - **`VerifyChain`** — walks the in-memory chain and re-verifies link
//!   hashes via [`crate::ReceiptChain::verify_integrity`].
//! - **`RebuildIndex`** — no-op (no indexes in this backend); returns the
//!   current receipt count as `receipts_indexed`.
//! - **`GetLogInfo`** — current segment id, count, degraded flag.
//!
//! ## Non-scope (production backend)
//!
//! - `RocksDB` segment writer (S3.1 §7.2).
//! - Cold-tier compaction (§12).
//! - Privacy-ceiling filter on `Query` (§10 trailer count).
//! - Per-segment Ed25519 sealing (T-010 surface; will plug in when the
//!   §22 MVP path drives `SegmentChain` end-to-end).
//! - Tantivy full-text search (`Query.text_match`).
//!
//! Each non-scope item maps to a future task; the public Rust API is
//! stable across that change because all of them live behind the
//! [`crate::service::proto::evidence_log_server::EvidenceLog`] trait.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use ed25519_dalek::SigningKey;
use tokio::sync::{broadcast, RwLock};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::compaction::{CompactionBackend, COMPACTION_WORKER_SUBJECT};
use crate::privacy::PrivacyCeiling;
use crate::record::{RecordType, RetentionClass};
use crate::segment::{SealedSegment, Segment, SegmentId};
use crate::service::conversions::{
    receipt_to_proto, record_type_from_proto_i32, DEFAULT_RETENTION,
};
use crate::service::proto;
use crate::service::SUPPRESSED_COUNT_TRAILER;
use crate::EvidenceError;
use crate::{ReceiptBuilder, ReceiptChain};

/// Default broadcast channel capacity for `Subscribe`. Matches the §9.3
/// per-subscriber buffer size; slow consumers see dropped events and an
/// optional `subscriber_dropped_event` (not yet implemented in T-011).
const DEFAULT_SUBSCRIBE_BROADCAST_CAPACITY: usize = 1024;

/// Default per-response `Query` page size when the request leaves `limit`
/// at zero (proto3 default). Mirrors the §10 documentation.
const DEFAULT_QUERY_LIMIT: u32 = 1000;

/// In-memory `EvidenceLog` backend.
///
/// All state lives inside a `tokio::sync::RwLock` so async RPC handlers can
/// take read or write access without blocking the runtime. Cloning the
/// backend is cheap (`Arc`) and gives multiple tonic services a shared view.
///
/// ## Sealed-segment store (T-015)
///
/// Sealed segments live in a separate `std::sync::Mutex<Vec<SealedSegment>>`
/// so the synchronous [`CompactionBackend`] trait can read and mutate them
/// without needing to acquire an `async` lock. This mirrors the `RocksDB`
/// backend's split between the `current_segment` CF (open snapshot, async
/// state) and the `segments` CF (sealed, plain `Arc<DB>` access). The open
/// chain is untouched by compaction; only sealed-segment receipts age out.
#[derive(Clone)]
pub struct InMemoryEvidenceLog {
    state: Arc<RwLock<State>>,
    signing_key: Arc<SigningKey>,
    /// `tokio::sync::broadcast` powers `Subscribe`: each append fans out a
    /// fresh proto receipt to every live subscriber. Capacity is bounded to
    /// match the §9.3 backpressure semantics.
    live_tx: broadcast::Sender<proto::EvidenceReceipt>,
    started_at: chrono::DateTime<Utc>,
    log_id: String,
    /// T-015 — sealed-segment store. Two parallel structures keyed by chain
    /// order:
    ///
    /// - `sealed`: the [`SealedSegment`] envelope (metadata: id, seal hash,
    ///   signature, `sealed_at`, etc.) — preserved forever even after
    ///   compaction (§12 tombstone rule).
    /// - `sealed_receipts`: the receipts that aged out are removed from this
    ///   map by [`CompactionBackend::compact_segment`] but the receipt
    ///   identities are preserved in the sealed segment envelope (which still
    ///   carries `receipts()` as the original immutable list — that field is
    ///   the on-the-wire tombstone, while this map is the warm-tier "body
    ///   present?" predicate).
    sealed_segments: Arc<std::sync::Mutex<SealedStore>>,
}

#[derive(Default)]
struct State {
    /// The append-only receipt chain. T-011 keeps a single "current open
    /// segment" view; segment sealing is wired via T-010 but not exercised
    /// over the gRPC surface until the production-backend task.
    chain: ReceiptChain,
    /// Cross-segment chain head: the previous segment's id + seal hash,
    /// used by [`InMemoryEvidenceLog::seal_current_segment`] (T-015) to
    /// link the next segment back to its predecessor per S3.1 §5.2.
    previous_segment_link: Option<(SegmentId, String)>,
}

/// T-015 sealed-segment store. The `sealed` vector preserves chain order so
/// [`CompactionBackend::list_sealed_segments`] returns segments in the same
/// order the `RocksDB` backend would.
#[derive(Default)]
struct SealedStore {
    sealed: Vec<SealedSegment>,
    /// Per-segment counter of receipts still in the warm tier. Decremented
    /// to zero when the segment is compacted. The sealed envelope itself
    /// retains the receipt identities for chain-tombstone discovery.
    warm_receipt_count: std::collections::HashMap<SegmentId, usize>,
    /// Mirror of `warm_receipt_count` for the `record_type_index` rows.
    /// In the in-memory backend the two counts are always equal because
    /// every receipt has exactly one index row.
    warm_index_count: std::collections::HashMap<SegmentId, usize>,
}

impl InMemoryEvidenceLog {
    /// Construct a new backend with a caller-supplied Ed25519 signing key.
    ///
    /// Production: the key arrives from S5.2 Vault Broker for subject
    /// `_system:service:evidence-segment-signer`. Tests pass an ephemeral
    /// keypair built from a fixed seed for determinism.
    #[must_use]
    pub fn new(signing_key: SigningKey) -> Self {
        let (live_tx, _live_rx) = broadcast::channel(DEFAULT_SUBSCRIBE_BROADCAST_CAPACITY);
        Self {
            state: Arc::new(RwLock::new(State::default())),
            signing_key: Arc::new(signing_key),
            live_tx,
            started_at: Utc::now(),
            // T-011: mint a per-instance log identifier from the EvidenceReceiptId
            // generator (`evr_<ULID>`) re-purposed as an instance tag. The hash
            // prefix gives ops a stable, sortable id without pulling `ulid`
            // into this module's direct deps.
            log_id: format!(
                "aios-evidence-log/{}",
                aios_action::EvidenceReceiptId::new().as_str()
            ),
            sealed_segments: Arc::new(std::sync::Mutex::new(SealedStore::default())),
        }
    }

    /// Seal the current open segment (T-015 / S3.1 §7).
    ///
    /// Consumes every receipt currently in the open chain into a new
    /// [`SealedSegment`], appends it to the sealed-segment store, links it
    /// to the previous segment via §5.2 cross-segment hash chain, and
    /// resets the open chain.
    ///
    /// # Errors
    ///
    /// - [`EvidenceError::EmptySegment`] when the open chain has no
    ///   receipts.
    /// - Any error returned by [`crate::Segment::seal`].
    pub async fn seal_current_segment(
        &self,
        retention_class: RetentionClass,
    ) -> Result<SealedSegment, EvidenceError> {
        let mut guard = self.state.write().await;
        let receipts: Vec<_> = guard.chain.receipts().to_vec();
        if receipts.is_empty() {
            return Err(EvidenceError::EmptySegment);
        }
        let mut segment = Segment::new(retention_class);
        for r in receipts {
            segment.append(r)?;
        }
        let (prev_id, prev_seal) = match &guard.previous_segment_link {
            Some((id, seal)) => (Some(id.clone()), Some(seal.clone())),
            None => (None, None),
        };
        let sealed = segment.seal(prev_id.as_ref(), prev_seal.as_deref(), &self.signing_key)?;

        // Update cross-segment link for the next seal.
        guard.previous_segment_link = Some((
            sealed.segment_id().clone(),
            sealed.segment_seal_hash().to_owned(),
        ));
        // Reset the open chain — sealed-segment receipts are no longer in
        // the live chain (they live in the sealed envelope).
        guard.chain = ReceiptChain::new();
        drop(guard);

        // Record in the sealed-segment store; warm receipt counts mirror
        // the receipt list size.
        let warm = sealed.receipt_count();
        let mut store = self.sealed_segments.lock().map_err(|e| {
            EvidenceError::EncodingFailed(format!("sealed-store mutex poisoned: {e}"))
        })?;
        store
            .warm_receipt_count
            .insert(sealed.segment_id().clone(), warm);
        store
            .warm_index_count
            .insert(sealed.segment_id().clone(), warm);
        store.sealed.push(sealed.clone());
        drop(store);

        Ok(sealed)
    }

    /// Snapshot of the sealed-segment count. Operator telemetry.
    #[must_use]
    pub fn sealed_segment_count(&self) -> usize {
        self.sealed_segments.lock().map_or(0, |s| s.sealed.len())
    }

    /// Snapshot of the warm-tier receipt count for a given sealed segment.
    /// Returns `None` when the segment id is unknown to the backend.
    ///
    /// After [`CompactionBackend::compact_segment`] runs, this returns
    /// `Some(0)` for the compacted segment (the entry is preserved so
    /// auditors can distinguish "segment compacted" from "segment never
    /// existed").
    #[must_use]
    pub fn warm_receipt_count_for(&self, segment_id: &SegmentId) -> Option<usize> {
        self.sealed_segments
            .lock()
            .ok()
            .and_then(|s| s.warm_receipt_count.get(segment_id).copied())
    }

    /// Stable identifier of this evidence log instance. Used by `GetLogInfo`.
    #[must_use]
    pub fn log_id(&self) -> &str {
        &self.log_id
    }

    /// The Ed25519 public key derived from the configured signing key. Used
    /// by tests and operators to verify receipts produced by this backend.
    #[must_use]
    pub fn verifying_key(&self) -> ed25519_dalek::VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Convenience: how many receipts are currently in memory.
    pub async fn receipt_count(&self) -> usize {
        self.state.read().await.chain.len()
    }
}

// =====================================================================
// CompactionBackend (T-015) — in-memory implementation
// =====================================================================
//
// The trait methods are synchronous; the in-memory backend's async state is
// guarded by `tokio::sync::RwLock`. We acquire it via `blocking_write()` /
// `blocking_read()`. Per tokio's documentation, these methods MUST be called
// outside an async runtime task or inside `tokio::task::spawn_blocking`.
// Tests drive the worker via `spawn_blocking`; production wraps the
// periodic tick task in the same primitive (see the module-level docs in
// `crate::compaction`).

impl CompactionBackend for InMemoryEvidenceLog {
    fn list_sealed_segments(
        &self,
    ) -> Result<Vec<(SegmentId, RetentionClass, chrono::DateTime<Utc>)>, EvidenceError> {
        let store = self.sealed_segments.lock().map_err(|e| {
            EvidenceError::EncodingFailed(format!("sealed-store mutex poisoned: {e}"))
        })?;
        // Only surface segments whose warm-tier receipts are still present.
        // Already-compacted segments are tombstoned in `sealed` and would
        // otherwise drive the worker to re-emit approval-required records
        // every tick.
        Ok(store
            .sealed
            .iter()
            .filter(|s| {
                store
                    .warm_receipt_count
                    .get(s.segment_id())
                    .copied()
                    .unwrap_or(0)
                    > 0
            })
            .map(|s| (s.segment_id().clone(), s.retention_class(), s.sealed_at()))
            .collect())
    }

    fn compact_segment(&mut self, segment_id: &SegmentId) -> Result<(usize, usize), EvidenceError> {
        let mut store = self.sealed_segments.lock().map_err(|e| {
            EvidenceError::EncodingFailed(format!("sealed-store mutex poisoned: {e}"))
        })?;
        // The sealed segment metadata MUST survive — only the warm-tier
        // receipt-body and index counts go to zero (§12 tombstone rule).
        let removed = store
            .warm_receipt_count
            .insert(segment_id.clone(), 0)
            .ok_or_else(|| {
                EvidenceError::EncodingFailed(format!(
                    "compact_segment: unknown segment `{}`",
                    segment_id.as_str()
                ))
            })?;
        let pruned = store
            .warm_index_count
            .insert(segment_id.clone(), 0)
            .ok_or_else(|| {
                EvidenceError::EncodingFailed(format!(
                    "compact_segment: unknown segment index `{}`",
                    segment_id.as_str()
                ))
            })?;
        drop(store);
        Ok((removed, pruned))
    }

    fn emit_approval_required(&mut self, segment_id: &SegmentId) -> Result<(), EvidenceError> {
        // Append the COMPACTION_APPROVAL_REQUIRED record through the normal
        // signed+chained pipeline — segment id is encoded in the receipt
        // subject canonical-id grammar (`<subject>/<segment_id>`) so a
        // future S5.1 NamespaceScope wiring can extract it cleanly.
        //
        // The state lock is acquired via `blocking_write()`; the worker
        // runs in a blocking task per the module-level docs.
        let mut guard = self.state.blocking_write();
        let previous = guard.chain.receipts().last().cloned();
        let subject = format!("{COMPACTION_WORKER_SUBJECT}/{}", segment_id.as_str());
        let payload = serde_json::json!({
            "segment_id": segment_id.as_str(),
            "reason": "retention_horizon_passed",
        });
        let receipt = ReceiptBuilder::new(
            RecordType::CompactionApprovalRequired,
            RetentionClass::Forever,
            subject,
        )
        .with_payload(payload)
        .seal_signed(previous.as_ref(), &self.signing_key)?;
        guard.chain.append(receipt.clone())?;
        drop(guard);

        // Broadcast to live subscribers — operators get the
        // approval-required signal via Subscribe in real time.
        let wire = receipt_to_proto(&receipt);
        let _ = self.live_tx.send(wire);

        Ok(())
    }
}

#[async_trait]
#[allow(
    clippy::result_large_err,
    reason = "tonic::Status is the canonical gRPC error type (176 bytes); \
              the lint is irrelevant for a generated service surface"
)]
impl proto::evidence_log_server::EvidenceLog for InMemoryEvidenceLog {
    // -----------------------------------------------------------------
    // Append
    // -----------------------------------------------------------------

    async fn append(
        &self,
        request: Request<proto::AppendRequest>,
    ) -> Result<Response<proto::EvidenceReceipt>, Status> {
        let req = request.into_inner();
        let record_type = record_type_from_proto_i32(req.record_type)?;
        let subject = req.subject;
        if subject.trim().is_empty() {
            return Err(Status::invalid_argument(
                "AppendRequest.subject must be non-empty (S5.1 canonical id)",
            ));
        }
        // T-011: the proto's RecordPayload one-of is opaque; we persist the
        // record_type + subject + optional action_id only. Wave 14 wires
        // the full payload schemas through.
        let mut builder = ReceiptBuilder::new(record_type, DEFAULT_RETENTION, subject);
        if !req.action_id.is_empty() {
            let action_id = aios_action::ActionId::parse(&req.action_id)
                .map_err(|e| Status::invalid_argument(format!("invalid action_id: {e}")))?;
            builder = builder.with_action_id(action_id);
        }

        // Acquire write lock, link to the tail of the chain.
        let mut guard = self.state.write().await;
        let previous = guard.chain.receipts().last().cloned();
        let receipt = builder
            .seal_signed(previous.as_ref(), &self.signing_key)
            .map_err(Status::from)?;
        guard.chain.append(receipt.clone()).map_err(Status::from)?;
        drop(guard);

        let wire = receipt_to_proto(&receipt);
        // Broadcast to live subscribers; failure to send (no subscribers) is
        // not an error.
        let _ = self.live_tx.send(wire.clone());
        Ok(Response::new(wire))
    }

    // -----------------------------------------------------------------
    // ReadReceipt
    // -----------------------------------------------------------------

    async fn read_receipt(
        &self,
        request: Request<proto::ReadReceiptRequest>,
    ) -> Result<Response<proto::EvidenceReceipt>, Status> {
        let id = request.into_inner().receipt_id;
        if id.is_empty() {
            return Err(Status::invalid_argument(
                "ReadReceiptRequest.receipt_id must be non-empty",
            ));
        }
        let hit = {
            let guard = self.state.read().await;
            guard
                .chain
                .receipts()
                .iter()
                .find(|r| r.receipt_id().as_str() == id)
                .map(receipt_to_proto)
        };
        hit.map_or_else(
            || {
                Err(Status::not_found(format!(
                    "no evidence receipt with id `{id}`"
                )))
            },
            |p| Ok(Response::new(p)),
        )
    }

    // -----------------------------------------------------------------
    // Subscribe (server-streaming)
    // -----------------------------------------------------------------

    type SubscribeStream =
        Pin<Box<dyn Stream<Item = Result<proto::EvidenceReceipt, Status>> + Send + 'static>>;

    async fn subscribe(
        &self,
        request: Request<proto::SubscribeRequest>,
    ) -> Result<Response<Self::SubscribeStream>, Status> {
        let req = request.into_inner();

        // Build per-subscriber filter set once.
        let record_filter: Vec<RecordType> = req
            .record_types_filter
            .iter()
            .map(|v| record_type_from_proto_i32(*v))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::from)?;
        let subject_filter = req.subject_filter.clone();
        let correlation_filter = req.correlation_id_filter.clone();
        let resume_from = req.resume_from_receipt_id.clone();

        // T-014 — privacy ceiling for live subscribers (§10 + §11.4).
        // Receipts that fail `ceiling.admits(..)` are silently dropped from
        // the stream. The §23.2 `suppressed_count` is not surfaced on
        // `Subscribe` because the stream has no logical "end" trailer; the
        // counter is only meaningful on the bounded `Query` path.
        let ceiling = PrivacyCeiling::from_caller(
            req.caller_subject.clone(),
            optional_group(&req.caller_primary_group),
            req.caller_is_ai,
            req.caller_is_recovery_mode,
        );

        // Replay buffer: receipts after `resume_from` (or all, if blank), filtered.
        let guard = self.state.read().await;
        let mut replay: Vec<proto::EvidenceReceipt> = Vec::new();
        let mut replaying = resume_from.is_empty(); // start emitting at index 0 if no bookmark
        for r in guard.chain.receipts() {
            if !replaying {
                if r.receipt_id().as_str() == resume_from {
                    replaying = true;
                }
                continue;
            }
            if !ceiling.admits(r) {
                continue;
            }
            let wire = receipt_to_proto(r);
            if subscribe_filters_pass(&wire, &record_filter, &subject_filter, &correlation_filter) {
                replay.push(wire);
            }
        }
        drop(guard);

        // Subscribe to live updates. Lagged subscribers (Err on the broadcast
        // stream) are silently dropped — the per-consumer
        // `subscriber_dropped_event` documented in S3.1 §9.3 is not yet
        // emitted in T-011.
        let rx = self.live_tx.subscribe();
        let live = BroadcastStream::new(rx).filter_map(move |item| {
            item.ok().and_then(|wire| {
                // T-014 ceiling check on live broadcast events. Uses the
                // wire-form predicate so we don't need to resolve the sealed
                // receipt out of the chain (which would require an extra
                // async lock acquire per event).
                let rt = record_type_from_proto_i32(wire.record_type).ok()?;
                if !ceiling.admits_wire(&wire.subject, rt) {
                    return None;
                }
                if subscribe_filters_pass(
                    &wire,
                    &record_filter,
                    &subject_filter,
                    &correlation_filter,
                ) {
                    Some(Ok(wire))
                } else {
                    None
                }
            })
        });

        let combined = tokio_stream::iter(replay.into_iter().map(Ok)).chain(live);
        Ok(Response::new(Box::pin(combined)))
    }

    // -----------------------------------------------------------------
    // Query (server-streaming)
    // -----------------------------------------------------------------

    type QueryStream =
        Pin<Box<dyn Stream<Item = Result<proto::EvidenceReceipt, Status>> + Send + 'static>>;

    async fn query(
        &self,
        request: Request<proto::QueryRequest>,
    ) -> Result<Response<Self::QueryStream>, Status> {
        let req = request.into_inner();
        let record_filter: Vec<RecordType> = req
            .record_types_filter
            .iter()
            .map(|v| record_type_from_proto_i32(*v))
            .collect::<Result<Vec<_>, _>>()
            .map_err(Status::from)?;

        let limit = if req.limit == 0 {
            DEFAULT_QUERY_LIMIT
        } else {
            req.limit
        };

        // T-014 — build the per-caller ceiling.
        let ceiling = PrivacyCeiling::from_caller(
            req.subject.clone(),
            optional_group(&req.caller_primary_group),
            req.caller_is_ai,
            req.caller_is_recovery_mode,
        );

        let guard = self.state.read().await;
        let mut hits: Vec<proto::EvidenceReceipt> = Vec::new();
        // T-014 — number of receipts the privacy ceiling silently filtered
        // from results matching all the other filters. Returned via the
        // `x-aios-suppressed-count` initial-metadata header. See spec §23.2.
        let mut suppressed_count: u64 = 0;
        for r in guard.chain.receipts() {
            // Action id filter.
            if !req.action_id_filter.is_empty() {
                match r.action_id() {
                    Some(a) if a.as_str() == req.action_id_filter => {}
                    _ => continue,
                }
            }
            // Subject filter.
            if !req.subject_filter.is_empty() && r.subject_canonical_id() != req.subject_filter {
                continue;
            }
            // Record-type filter.
            if !record_filter.is_empty() && !record_filter.contains(&r.record_type()) {
                continue;
            }
            // Time-range filter.
            if let Some(from) = req.from_time.as_ref() {
                let from_dt = crate::service::conversions::timestamp_to_datetime(from);
                if r.recorded_at() < from_dt {
                    continue;
                }
            }
            if let Some(to) = req.to_time.as_ref() {
                let to_dt = crate::service::conversions::timestamp_to_datetime(to);
                if r.recorded_at() > to_dt {
                    continue;
                }
            }
            // T-014 — privacy ceiling. Receipts matching the user's filters
            // but failing the per-caller ceiling are silently dropped and
            // counted in the trailer.
            if !ceiling.admits(r) {
                suppressed_count = suppressed_count.saturating_add(1);
                continue;
            }
            hits.push(receipt_to_proto(r));
            if u32::try_from(hits.len()).unwrap_or(u32::MAX) >= limit {
                break;
            }
        }
        drop(guard);

        // tonic-rs server-streaming wants Send + 'static; build an owned stream.
        let stream = tokio_stream::iter(hits.into_iter().map(Ok));
        let mut response = Response::new(Box::pin(stream) as Self::QueryStream);
        attach_suppressed_count(&mut response, suppressed_count);
        Ok(response)
    }

    // -----------------------------------------------------------------
    // VerifyChain
    // -----------------------------------------------------------------

    async fn verify_chain(
        &self,
        request: Request<proto::VerifyChainRequest>,
    ) -> Result<Response<proto::VerifyChainResponse>, Status> {
        // T-011 keeps a single in-memory chain; the segment range fields are
        // accepted but not yet differentiated. The whole chain is walked.
        let _req = request.into_inner();
        let guard = self.state.read().await;
        let walked = guard.chain.len();
        let result = guard.chain.verify_integrity();
        drop(guard);
        match result {
            Ok(()) => Ok(Response::new(proto::VerifyChainResponse {
                consistent: true,
                receipts_checked: walked as u64,
                first_anomalous_receipt_id: String::new(),
                detection_method: "in_memory_walk_link_hash".to_owned(),
            })),
            Err(EvidenceError::ChainBroken {
                index,
                actual,
                expected,
            }) => {
                let guard = self.state.read().await;
                let first_anomalous = guard
                    .chain
                    .receipts()
                    .get(index)
                    .map(|r| r.receipt_id().as_str().to_owned())
                    .unwrap_or_default();
                drop(guard);
                Ok(Response::new(proto::VerifyChainResponse {
                    consistent: false,
                    receipts_checked: walked as u64,
                    first_anomalous_receipt_id: first_anomalous,
                    detection_method: format!(
                        "chain_broken_link_at_{index}: actual=`{actual}` expected=`{expected}`"
                    ),
                }))
            }
            Err(other) => Err(Status::from(other)),
        }
    }

    // -----------------------------------------------------------------
    // RebuildIndex
    // -----------------------------------------------------------------

    async fn rebuild_index(
        &self,
        _request: Request<proto::RebuildIndexRequest>,
    ) -> Result<Response<proto::RebuildIndexResponse>, Status> {
        // No indexes in the in-memory backend; treat as a no-op that returns
        // the current chain length. Production rebuilds Tantivy + RocksDB
        // secondary indexes (§17).
        let len = self.state.read().await.chain.len();
        Ok(Response::new(proto::RebuildIndexResponse {
            receipts_indexed: len as u64,
            completed_at: Some(crate::service::conversions::datetime_to_timestamp(
                Utc::now(),
            )),
        }))
    }

    // -----------------------------------------------------------------
    // GetLogInfo
    // -----------------------------------------------------------------

    async fn get_log_info(
        &self,
        _request: Request<()>,
    ) -> Result<Response<proto::LogInfo>, Status> {
        let count = self.state.read().await.chain.len();
        Ok(Response::new(proto::LogInfo {
            log_id: self.log_id.clone(),
            supported_schema_versions: vec!["aios.evidence.v1alpha1".to_owned()],
            default_schema_version: "aios.evidence.v1alpha1".to_owned(),
            // T-011: in-memory backend has a single open segment with no id
            // until segment sealing is wired through gRPC. Leave empty.
            active_segment_id: String::new(),
            active_segment_record_count: count as u64,
            degraded: false,
            started_at: Some(crate::service::conversions::datetime_to_timestamp(
                self.started_at,
            )),
        }))
    }
}

/// T-014 helper — map a proto3-default-empty string to `Option<String>` so
/// the privacy ceiling can distinguish "caller declared no group" (`None`)
/// from "caller declared an empty group" (which is the same thing in proto3
/// wire semantics, so we treat both as `None`).
fn optional_group(raw: &str) -> Option<String> {
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_owned())
    }
}

/// T-014 helper — attach the `x-aios-suppressed-count` initial-metadata
/// header (decimal `u64`) to a `Query` response. Spec §10 + §23.2.
///
/// **Trailer vs header note.** The spec says "stream trailer", and gRPC has
/// both initial metadata (sent before the stream) and trailing metadata
/// (sent after the stream's final message). tonic 0.12 server-streaming has
/// first-class support for setting initial metadata via
/// [`tonic::Response::metadata_mut`], but the only path to set trailing
/// metadata is via the `Status` on the final stream-ending message — which
/// requires the server-side handler to construct the count *after* the
/// stream is fully drained. Since T-014 materializes the full hit list into
/// `Vec<EvidenceReceipt>` before returning the stream, the count is known
/// at response-construction time and is published via initial metadata for
/// simplicity. Clients read it identically via `response.metadata().get(..)`.
/// When a future task switches `Query` to a streaming-from-disk
/// implementation, this helper will be replaced with a tail-of-stream
/// trailer emit through `tonic::Status::with_metadata`.
fn attach_suppressed_count<T>(response: &mut Response<T>, count: u64) {
    // Numeric ASCII parse is infallible on tonic's allowed set; if the
    // header insertion ever returns an error, the only safe response is
    // to leave the count off the response (the client will see "0
    // suppressed") rather than tear down the RPC. The
    // `MetadataValue::from(u64)` path can fail only on invalid ASCII —
    // which a `u64::to_string()` cannot produce — so the let-else is
    // defensive only.
    let Ok(value) = count
        .to_string()
        .parse::<tonic::metadata::MetadataValue<_>>()
    else {
        return;
    };
    response
        .metadata_mut()
        .insert(SUPPRESSED_COUNT_TRAILER, value);
}

/// Helper: apply the three `Subscribe` filters to a wire receipt.
///
/// Returns `true` when the receipt should pass through to the subscriber.
fn subscribe_filters_pass(
    wire: &proto::EvidenceReceipt,
    record_filter: &[RecordType],
    subject_filter: &str,
    correlation_filter: &str,
) -> bool {
    if !record_filter.is_empty() {
        match record_type_from_proto_i32(wire.record_type) {
            Ok(rt) if record_filter.contains(&rt) => {}
            _ => return false,
        }
    }
    if !subject_filter.is_empty() && wire.subject != subject_filter {
        return false;
    }
    if !correlation_filter.is_empty() && wire.correlation_id != correlation_filter {
        return false;
    }
    true
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
    use crate::service::proto::evidence_log_server::EvidenceLog as EvidenceLogService;
    use ed25519_dalek::SigningKey;

    fn test_backend() -> InMemoryEvidenceLog {
        let sk = SigningKey::from_bytes(&[7u8; 32]);
        InMemoryEvidenceLog::new(sk)
    }

    fn append_req(record_type: proto::RecordType, subject: &str) -> proto::AppendRequest {
        proto::AppendRequest {
            schema_version: "aios.evidence.v1alpha1".to_owned(),
            payload: None,
            record_type: i32::from(record_type),
            subject: subject.to_owned(),
            action_id: String::new(),
            correlation_id: String::new(),
            trace_id: String::new(),
            simulated: false,
        }
    }

    // ─── append ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn append_mints_receipt_and_links_chain() {
        let b = test_backend();

        let r1 = b
            .append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:operator-1",
            )))
            .await
            .expect("append 1")
            .into_inner();

        let r2 = b
            .append(Request::new(append_req(
                proto::RecordType::PolicyDecision,
                "service:policy",
            )))
            .await
            .expect("append 2")
            .into_inner();

        assert!(r1.receipt_id.starts_with("evr_"));
        assert!(r2.receipt_id.starts_with("evr_"));
        assert_ne!(r1.receipt_id, r2.receipt_id);
        // r1 is genesis -> previous hash empty.
        assert!(r1.previous_receipt_hash.is_empty());
        // r2 must link.
        assert!(!r2.previous_receipt_hash.is_empty());
        assert_eq!(r2.previous_receipt_hash.len(), 32);
    }

    #[tokio::test]
    async fn append_rejects_empty_subject() {
        let b = test_backend();
        let err = b
            .append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "",
            )))
            .await
            .expect_err("must reject");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    #[tokio::test]
    async fn append_rejects_unspecified_record_type() {
        let b = test_backend();
        let err = b
            .append(Request::new(append_req(
                proto::RecordType::Unspecified,
                "human:operator-1",
            )))
            .await
            .expect_err("must reject");
        assert_eq!(err.code(), tonic::Code::Internal);
    }

    #[tokio::test]
    async fn append_with_action_id_binds_action() {
        let b = test_backend();
        let aid = aios_action::ActionId::new();
        let mut req = append_req(
            proto::RecordType::ExecutionStarted,
            "service:capability-runtime",
        );
        req.action_id = aid.as_str().to_owned();
        let r = b
            .append(Request::new(req))
            .await
            .expect("append")
            .into_inner();
        assert_eq!(r.action_id, aid.as_str());
    }

    // ─── read_receipt ──────────────────────────────────────────────────

    #[tokio::test]
    async fn read_receipt_returns_the_receipt_when_present() {
        let b = test_backend();
        let r = b
            .append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:operator-1",
            )))
            .await
            .expect("append")
            .into_inner();
        let back = b
            .read_receipt(Request::new(proto::ReadReceiptRequest {
                receipt_id: r.receipt_id.clone(),
            }))
            .await
            .expect("read")
            .into_inner();
        assert_eq!(back.receipt_id, r.receipt_id);
        assert_eq!(back.payload_hash, r.payload_hash);
    }

    #[tokio::test]
    async fn read_receipt_returns_not_found_for_unknown_id() {
        let b = test_backend();
        let err = b
            .read_receipt(Request::new(proto::ReadReceiptRequest {
                receipt_id: "evr_does_not_exist".to_owned(),
            }))
            .await
            .expect_err("must miss");
        assert_eq!(err.code(), tonic::Code::NotFound);
    }

    #[tokio::test]
    async fn read_receipt_rejects_empty_id() {
        let b = test_backend();
        let err = b
            .read_receipt(Request::new(proto::ReadReceiptRequest {
                receipt_id: String::new(),
            }))
            .await
            .expect_err("must reject");
        assert_eq!(err.code(), tonic::Code::InvalidArgument);
    }

    // ─── query ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn query_returns_receipts_filtered_by_record_type() {
        let b = test_backend();
        for _ in 0..3 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:operator-1",
            )))
            .await
            .expect("a1");
        }
        for _ in 0..2 {
            b.append(Request::new(append_req(
                proto::RecordType::PolicyDecision,
                "service:policy",
            )))
            .await
            .expect("a2");
        }

        let stream = b
            .query(Request::new(proto::QueryRequest {
                record_types_filter: vec![i32::from(proto::RecordType::PolicyDecision)],
                subject_filter: String::new(),
                correlation_id_filter: String::new(),
                action_id_filter: String::new(),
                from_time: None,
                to_time: None,
                text_match: String::new(),
                limit: 0,
                subject: String::new(),
                caller_primary_group: String::new(),
                caller_is_ai: false,
                caller_is_recovery_mode: false,
            }))
            .await
            .expect("query")
            .into_inner();

        let collected: Vec<_> = stream.collect::<Vec<_>>().await;
        assert_eq!(collected.len(), 2);
        for item in collected {
            let w = item.expect("ok");
            assert_eq!(w.record_type, i32::from(proto::RecordType::PolicyDecision));
        }
    }

    #[tokio::test]
    async fn query_returns_receipts_filtered_by_subject() {
        let b = test_backend();
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:alice",
        )))
        .await
        .expect("alice");
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:bob",
        )))
        .await
        .expect("bob");
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:alice",
        )))
        .await
        .expect("alice2");

        let stream = b
            .query(Request::new(proto::QueryRequest {
                record_types_filter: vec![],
                subject_filter: "human:alice".to_owned(),
                correlation_id_filter: String::new(),
                action_id_filter: String::new(),
                from_time: None,
                to_time: None,
                text_match: String::new(),
                limit: 0,
                subject: String::new(),
                caller_primary_group: String::new(),
                caller_is_ai: false,
                caller_is_recovery_mode: false,
            }))
            .await
            .expect("query")
            .into_inner();
        let collected: Vec<_> = stream.collect::<Vec<_>>().await;
        assert_eq!(collected.len(), 2);
    }

    #[tokio::test]
    async fn query_respects_limit() {
        let b = test_backend();
        for _ in 0..10 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:operator-1",
            )))
            .await
            .expect("append");
        }
        let stream = b
            .query(Request::new(proto::QueryRequest {
                record_types_filter: vec![],
                subject_filter: String::new(),
                correlation_id_filter: String::new(),
                action_id_filter: String::new(),
                from_time: None,
                to_time: None,
                text_match: String::new(),
                limit: 4,
                subject: String::new(),
                caller_primary_group: String::new(),
                caller_is_ai: false,
                caller_is_recovery_mode: false,
            }))
            .await
            .expect("query")
            .into_inner();
        let collected: Vec<_> = stream.collect::<Vec<_>>().await;
        assert_eq!(collected.len(), 4);
    }

    // ─── verify_chain ──────────────────────────────────────────────────

    #[tokio::test]
    async fn verify_chain_reports_consistent_on_healthy_chain() {
        let b = test_backend();
        for i in 0..3 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                &format!("human:operator-{i}"),
            )))
            .await
            .expect("a");
        }
        let r = b
            .verify_chain(Request::new(proto::VerifyChainRequest {
                segment_id_from: String::new(),
                segment_id_to: String::new(),
            }))
            .await
            .expect("verify")
            .into_inner();
        assert!(r.consistent);
        assert_eq!(r.receipts_checked, 3);
        assert_eq!(r.first_anomalous_receipt_id, "");
    }

    // ─── subscribe ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn subscribe_streams_live_events_and_filters_record_type() {
        let b = test_backend();
        // Prime two pre-existing entries.
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:operator-1",
        )))
        .await
        .expect("seed1");
        b.append(Request::new(append_req(
            proto::RecordType::PolicyDecision,
            "service:policy",
        )))
        .await
        .expect("seed2");

        let stream = b
            .subscribe(Request::new(proto::SubscribeRequest {
                record_types_filter: vec![i32::from(proto::RecordType::PolicyDecision)],
                subject_filter: String::new(),
                correlation_id_filter: String::new(),
                resume_from_receipt_id: String::new(),
                max_buffered: 0,
                caller_subject: String::new(),
                caller_primary_group: String::new(),
                caller_is_ai: false,
                caller_is_recovery_mode: false,
            }))
            .await
            .expect("sub")
            .into_inner();

        // We need to read 1 from replay (the PolicyDecision seed), then add a
        // live event and read that. Cap the wait so a deadlock cannot stall.
        let collected = tokio::time::timeout(std::time::Duration::from_millis(200), async move {
            let mut s = stream;
            let mut out: Vec<proto::EvidenceReceipt> = Vec::new();
            // First should be the seeded PolicyDecision (filter matched).
            if let Some(item) = s.next().await {
                out.push(item.expect("ok"));
            }
            // Now push another live event; spawn a producer.
            let b2 = b.clone();
            tokio::spawn(async move {
                let _ = b2
                    .append(Request::new(append_req(
                        proto::RecordType::PolicyDecision,
                        "service:policy",
                    )))
                    .await;
            });
            if let Some(item) = s.next().await {
                out.push(item.expect("ok"));
            }
            out
        })
        .await
        .expect("timeout");

        assert_eq!(collected.len(), 2);
        for w in collected {
            assert_eq!(w.record_type, i32::from(proto::RecordType::PolicyDecision));
        }
    }

    // ─── privacy ceiling on query (T-014) ──────────────────────────────

    fn query_for_caller(
        caller: &str,
        group: &str,
        is_ai: bool,
        recovery: bool,
    ) -> proto::QueryRequest {
        proto::QueryRequest {
            record_types_filter: vec![],
            subject_filter: String::new(),
            correlation_id_filter: String::new(),
            action_id_filter: String::new(),
            from_time: None,
            to_time: None,
            text_match: String::new(),
            limit: 0,
            subject: caller.to_owned(),
            caller_primary_group: group.to_owned(),
            caller_is_ai: is_ai,
            caller_is_recovery_mode: recovery,
        }
    }

    #[tokio::test]
    async fn query_privacy_ceiling_admits_self_records_and_filters_others() {
        let b = test_backend();
        // alice writes 2 records; bob writes 1 record.
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:alice",
        )))
        .await
        .expect("a1");
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:alice",
        )))
        .await
        .expect("a2");
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:bob",
        )))
        .await
        .expect("b1");

        // alice queries — sees only her 2 records; the 1 from bob is suppressed.
        let response = b
            .query(Request::new(query_for_caller(
                "human:alice",
                "",
                false,
                false,
            )))
            .await
            .expect("query");
        let suppressed = response
            .metadata()
            .get(SUPPRESSED_COUNT_TRAILER)
            .expect("trailer present")
            .to_str()
            .expect("ascii");
        assert_eq!(suppressed, "1");
        let collected: Vec<_> = response.into_inner().collect::<Vec<_>>().await;
        assert_eq!(collected.len(), 2);
    }

    #[tokio::test]
    async fn query_privacy_ceiling_public_records_admitted_to_non_ai() {
        let b = test_backend();
        // Public PolicyDecision record from "system:policy".
        b.append(Request::new(append_req(
            proto::RecordType::PolicyDecision,
            "service:policy",
        )))
        .await
        .expect("policy");
        // Private record from another subject.
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:bob",
        )))
        .await
        .expect("bob");

        let response = b
            .query(Request::new(query_for_caller(
                "human:alice",
                "",
                false,
                false,
            )))
            .await
            .expect("query");
        let suppressed = response
            .metadata()
            .get(SUPPRESSED_COUNT_TRAILER)
            .expect("trailer present")
            .to_str()
            .expect("ascii");
        // bob's ActionReceived is filtered; PolicyDecision is admitted.
        assert_eq!(suppressed, "1");
        let collected: Vec<_> = response.into_inner().collect::<Vec<_>>().await;
        assert_eq!(collected.len(), 1);
        assert_eq!(
            collected[0].as_ref().expect("ok").record_type,
            i32::from(proto::RecordType::PolicyDecision)
        );
    }

    #[tokio::test]
    async fn query_privacy_ceiling_group_admits_same_group() {
        let b = test_backend();
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:ops/alice",
        )))
        .await
        .expect("a");
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:ops/bob",
        )))
        .await
        .expect("b");
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:finance/dave",
        )))
        .await
        .expect("d");

        // alice in ops group sees her own + bob's; dave is suppressed.
        let response = b
            .query(Request::new(query_for_caller(
                "human:ops/alice",
                "ops",
                false,
                false,
            )))
            .await
            .expect("query");
        let suppressed = response
            .metadata()
            .get(SUPPRESSED_COUNT_TRAILER)
            .expect("trailer present")
            .to_str()
            .expect("ascii");
        assert_eq!(suppressed, "1");
        let collected: Vec<_> = response.into_inner().collect::<Vec<_>>().await;
        assert_eq!(collected.len(), 2);
    }

    #[tokio::test]
    async fn query_empty_subject_admits_all_for_backward_compat() {
        // T-014 backward compat: empty `subject` (caller) = system caller,
        // which admits all receipts. Protects T-007..T-013 wire baseline.
        let b = test_backend();
        for _ in 0..3 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:alice",
            )))
            .await
            .expect("a");
        }
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:bob",
        )))
        .await
        .expect("b");

        let response = b
            .query(Request::new(query_for_caller("", "", false, false)))
            .await
            .expect("query");
        let suppressed = response
            .metadata()
            .get(SUPPRESSED_COUNT_TRAILER)
            .expect("trailer present")
            .to_str()
            .expect("ascii");
        assert_eq!(suppressed, "0");
        let collected: Vec<_> = response.into_inner().collect::<Vec<_>>().await;
        assert_eq!(collected.len(), 4);
    }

    #[tokio::test]
    async fn query_privacy_ceiling_ai_caller_cannot_see_other_ai() {
        let b = test_backend();
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "ai:agent-7",
        )))
        .await
        .expect("a7");
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "ai:agent-9",
        )))
        .await
        .expect("a9");

        let response = b
            .query(Request::new(query_for_caller(
                "ai:agent-7",
                "",
                /*is_ai=*/ true,
                false,
            )))
            .await
            .expect("query");
        let suppressed = response
            .metadata()
            .get(SUPPRESSED_COUNT_TRAILER)
            .expect("trailer present")
            .to_str()
            .expect("ascii");
        assert_eq!(suppressed, "1");
        let collected: Vec<_> = response.into_inner().collect::<Vec<_>>().await;
        assert_eq!(collected.len(), 1);
    }

    // ─── rebuild_index ─────────────────────────────────────────────────

    #[tokio::test]
    async fn rebuild_index_reports_current_receipt_count() {
        let b = test_backend();
        for _ in 0..5 {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                "human:operator-1",
            )))
            .await
            .expect("a");
        }
        let r = b
            .rebuild_index(Request::new(proto::RebuildIndexRequest {
                include_full_text: false,
            }))
            .await
            .expect("rebuild")
            .into_inner();
        assert_eq!(r.receipts_indexed, 5);
        assert!(r.completed_at.is_some());
    }

    // ─── get_log_info ──────────────────────────────────────────────────

    #[tokio::test]
    async fn get_log_info_reports_log_id_and_count() {
        let b = test_backend();
        b.append(Request::new(append_req(
            proto::RecordType::ActionReceived,
            "human:operator-1",
        )))
        .await
        .expect("a");
        let info = b
            .get_log_info(Request::new(()))
            .await
            .expect("info")
            .into_inner();
        assert!(info.log_id.starts_with("aios-evidence-log/"));
        assert_eq!(info.active_segment_record_count, 1);
        assert!(!info.degraded);
        assert!(info.started_at.is_some());
        assert_eq!(
            info.supported_schema_versions,
            vec!["aios.evidence.v1alpha1".to_owned()]
        );
    }

    // ─── T-015 — CompactionBackend on InMemoryEvidenceLog ──────────────

    use crate::compaction::{CompactionPolicy, CompactionWorker};

    /// Append `n` receipts, then seal the open segment with the given
    /// retention class. Returns the sealed segment.
    async fn append_and_seal(
        b: &InMemoryEvidenceLog,
        n: usize,
        retention: RetentionClass,
    ) -> SealedSegment {
        for i in 0..n {
            b.append(Request::new(append_req(
                proto::RecordType::ActionReceived,
                &format!("human:op-{i}"),
            )))
            .await
            .expect("append");
        }
        b.seal_current_segment(retention)
            .await
            .expect("seal_current_segment")
    }

    #[tokio::test]
    async fn t015_inmemory_seal_creates_sealed_segment() {
        let b = test_backend();
        let sealed = append_and_seal(&b, 3, RetentionClass::Standard24M).await;
        assert_eq!(b.sealed_segment_count(), 1);
        // The sealed segment also carries the terminal SEGMENT_SEALED witness.
        assert!(sealed.receipt_count() >= 3);
        // Warm receipt count mirrors the sealed receipt count.
        assert_eq!(
            b.warm_receipt_count_for(sealed.segment_id()),
            Some(sealed.receipt_count())
        );
    }

    #[tokio::test]
    async fn t015_inmemory_compact_segment_drops_warm_counts_to_zero() {
        let b = test_backend();
        let sealed = append_and_seal(&b, 4, RetentionClass::Standard24M).await;
        let id = sealed.segment_id().clone();
        // Drive the sync trait directly. No async lock contention because the
        // test holds no live read/write guard.
        let mut b_clone = b.clone();
        let backend_ref = &mut b_clone;
        let (removed, pruned) = backend_ref.compact_segment(&id).expect("compact");
        assert_eq!(removed, sealed.receipt_count());
        assert_eq!(pruned, sealed.receipt_count());
        assert_eq!(b.warm_receipt_count_for(&id), Some(0));
        // Sealed segment envelope is still there — tombstone preserved.
        assert_eq!(b.sealed_segment_count(), 1);
    }

    #[tokio::test]
    async fn t015_inmemory_list_sealed_segments_returns_chain_order() {
        let b = test_backend();
        let s1 = append_and_seal(&b, 2, RetentionClass::Standard24M).await;
        let s2 = append_and_seal(&b, 3, RetentionClass::Extended60M).await;
        let s3 = append_and_seal(&b, 4, RetentionClass::Forever).await;

        let listing = b.list_sealed_segments().expect("list");
        assert_eq!(listing.len(), 3);
        assert_eq!(&listing[0].0, s1.segment_id());
        assert_eq!(&listing[1].0, s2.segment_id());
        assert_eq!(&listing[2].0, s3.segment_id());
        assert_eq!(listing[0].1, RetentionClass::Standard24M);
        assert_eq!(listing[1].1, RetentionClass::Extended60M);
        assert_eq!(listing[2].1, RetentionClass::Forever);
    }

    #[tokio::test]
    async fn t015_inmemory_emit_approval_required_appends_signed_receipt() {
        let b = test_backend();
        let sealed = append_and_seal(&b, 2, RetentionClass::Standard24M).await;
        // After sealing, the live chain is empty — emit a fresh
        // approval-required record and verify it lands in the chain.
        let id = sealed.segment_id().clone();
        let b_for_emit = b.clone();
        tokio::task::spawn_blocking(move || {
            let mut bm = b_for_emit;
            bm.emit_approval_required(&id).expect("emit");
        })
        .await
        .expect("join");

        assert_eq!(b.receipt_count().await, 1);
        // The receipt must verify with the backend's verifying key (Ed25519
        // signature path intact).
        let guard = b.state.read().await;
        let r = guard
            .chain
            .receipts()
            .first()
            .expect("at least one")
            .clone();
        drop(guard);
        assert_eq!(r.record_type(), RecordType::CompactionApprovalRequired);
        assert!(r.subject_canonical_id().contains(COMPACTION_WORKER_SUBJECT));
        r.verify_signature(&b.verifying_key()).expect("sig verify");
    }

    #[tokio::test]
    async fn t015_inmemory_worker_auto_mode_compacts_aged_segments() {
        let b = test_backend();
        // Seal three segments — two will be artificially aged via the
        // worker's pluggable `now` argument; the FOREVER one must never
        // be touched.
        let s1 = append_and_seal(&b, 2, RetentionClass::Standard24M).await;
        let s2 = append_and_seal(&b, 3, RetentionClass::Extended60M).await;
        let s3 = append_and_seal(&b, 4, RetentionClass::Forever).await;

        // Advance "now" by 6 years (well past both Standard24M = 730 days
        // and Extended60M = 1825 days horizons).
        let far_future = Utc::now() + chrono::Duration::days(6 * 365);

        let report = tokio::task::spawn_blocking({
            let backend = b.clone();
            move || {
                let mut backend = backend;
                let mut worker = CompactionWorker::new(CompactionPolicy::Auto);
                worker.tick(&mut backend, far_future)
            }
        })
        .await
        .expect("join")
        .expect("tick");

        assert_eq!(report.eligible_segments, 2);
        assert_eq!(report.compacted_segments, 2);
        assert_eq!(report.awaiting_approval, 0);
        // Forever segment intact.
        assert_eq!(
            b.warm_receipt_count_for(s3.segment_id()),
            Some(s3.receipt_count())
        );
        // The two STANDARD/EXTENDED segments lost their warm content.
        assert_eq!(b.warm_receipt_count_for(s1.segment_id()), Some(0));
        assert_eq!(b.warm_receipt_count_for(s2.segment_id()), Some(0));
    }

    #[tokio::test]
    async fn t015_inmemory_worker_operator_approval_emits_then_waits_then_compacts() {
        let b = test_backend();
        let sealed = append_and_seal(&b, 3, RetentionClass::Standard24M).await;
        let id = sealed.segment_id().clone();
        let far_future = Utc::now() + chrono::Duration::days(3 * 365);

        // Tick 1: emit COMPACTION_APPROVAL_REQUIRED, segment still has
        // warm receipts.
        let (worker, report) = tokio::task::spawn_blocking({
            let backend = b.clone();
            move || {
                let mut backend = backend;
                let mut worker = CompactionWorker::new(CompactionPolicy::OperatorApproval);
                let r = worker.tick(&mut backend, far_future).expect("tick 1");
                (worker, r)
            }
        })
        .await
        .expect("join");
        assert_eq!(report.eligible_segments, 1);
        assert_eq!(report.compacted_segments, 0);
        assert_eq!(report.awaiting_approval, 1);
        assert_eq!(
            b.warm_receipt_count_for(&id),
            Some(sealed.receipt_count()),
            "warm receipts preserved before approval"
        );
        // The chain now carries the approval-required record.
        let chain_len_after_emit = b.receipt_count().await;
        assert!(chain_len_after_emit >= 1);

        // Approve, then tick 2 — segment now compacts.
        let id2 = id.clone();
        let report2 = tokio::task::spawn_blocking({
            let backend = b.clone();
            move || {
                let mut backend = backend;
                let mut worker = worker;
                worker.approve_segment(&id2).expect("approve");
                worker.tick(&mut backend, far_future)
            }
        })
        .await
        .expect("join")
        .expect("tick 2");
        assert_eq!(report2.compacted_segments, 1);
        assert_eq!(b.warm_receipt_count_for(&id), Some(0));
    }
}
