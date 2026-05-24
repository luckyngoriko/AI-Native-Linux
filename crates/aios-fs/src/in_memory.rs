//! In-memory [`AiosFs`](crate::AiosFs) harness for T-037.

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;

use aios_action::{blake3_hash, jcs_canonicalize};

use crate::chunk::{Chunk, ChunkId, ChunkRef};
use crate::error::FsError;
use crate::fs_trait::{
    AiosFs, FsContext, ObjectReadResult, ObjectWriteRequest, ObjectWriteResult, SnapshotSummary,
};
use crate::object::{
    Object, ObjectId, ObjectInit, ObjectKind, ObjectMetadata, PrivacyClass, ScopeBinding,
    ScopeKind, SubjectRef,
};
use crate::pointer::{Pointer, PointerId, PointerKind};
use crate::quarantine::{
    new_quarantine_id, MutableAiosFs, QuarantineDisposition, QuarantineReceipt, QuarantineTrigger,
};
use crate::snapshot_id::SnapshotId;
use crate::transaction::{PointerMoveOp, Transaction, TransactionId, TransactionState, WriteOp};
use crate::version::{Version, VersionId, VersionState};

/// In-process AIOS-FS harness backed by `RwLock<HashMap<...>>` catalogs.
///
/// The harness is deliberately small: no persistence, no real transaction
/// lifecycle driver and no GC pass. It exists so T-038..T-045 can plug real
/// engines into a stable trait surface.
#[derive(Debug, Clone)]
pub struct InMemoryAiosFs {
    state: Arc<RwLock<State>>,
}

impl Default for InMemoryAiosFs {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryAiosFs {
    /// Construct an empty in-memory filesystem and capture the empty head snapshot.
    #[must_use]
    pub fn new() -> Self {
        let mut state = State::default();
        state.capture_snapshot();

        Self {
            state: Arc::new(RwLock::new(state)),
        }
    }

    /// Capture the current state as the head snapshot.
    #[must_use]
    pub fn snapshot(&self) -> SnapshotSummary {
        self.write_state().capture_snapshot()
    }

    /// Harness-only state fixture used by T-037 tests.
    ///
    /// This is not the T-038 quarantine entry/exit driver: it does not move
    /// pointers, enforce authorization, or emit evidence. It only lets the read gate
    /// be tested against a quarantined version before the real transition driver
    /// lands.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::VersionNotFound`] when `version_id` is unknown.
    #[doc(hidden)]
    pub fn force_version_state_for_harness(
        &self,
        version_id: &VersionId,
        state: VersionState,
        quarantine_reason: Option<String>,
    ) -> Result<(), FsError> {
        {
            let mut store = self.write_state();
            let now = Utc::now();

            let version = store
                .versions
                .get_mut(version_id)
                .ok_or_else(|| FsError::VersionNotFound(version_id.clone()))?;

            version.state = state;
            if state == VersionState::Quarantined {
                version.quarantined_at = Some(now);
                version.quarantine_reason = quarantine_reason;
            }

            store.capture_snapshot();
        }

        Ok(())
    }

    /// Harness-only pointer fixture used by T-038 quarantine tests.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ObjectNotFound`] when `object_id` is unknown,
    /// [`FsError::VersionNotFound`] when `version_id` is unknown or belongs to
    /// another object.
    #[doc(hidden)]
    pub fn force_pointer_for_harness(
        &self,
        object_id: &ObjectId,
        kind: PointerKind,
        version_id: &VersionId,
    ) -> Result<PointerId, FsError> {
        let mut store = self.write_state();
        if !store.objects.contains_key(object_id) {
            return Err(FsError::ObjectNotFound(object_id.clone()));
        }

        let version = store
            .versions
            .get(version_id)
            .ok_or_else(|| FsError::VersionNotFound(version_id.clone()))?;
        if version.object_id != *object_id {
            return Err(FsError::VersionNotFound(version_id.clone()));
        }

        let now = Utc::now();
        let pointer_id = PointerId::new();
        store.pointers.insert(
            pointer_id.clone(),
            Pointer {
                pointer_id: pointer_id.clone(),
                object_id: object_id.clone(),
                kind,
                current_version_id: version_id.clone(),
                last_promoted_at: now,
                last_promoted_by_transaction_id: TransactionId::new(),
            },
        );
        store.capture_snapshot();
        drop(store);

        Ok(pointer_id)
    }

    /// Harness-only object pointer rebinding used to exercise T-037 read gates
    /// after T-038 pointer moves.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ObjectNotFound`] when `object_id` is unknown and
    /// [`FsError::PointerNotFound`] when `pointer_id` is unknown or belongs to
    /// another object.
    #[doc(hidden)]
    pub fn force_object_current_pointer_for_harness(
        &self,
        object_id: &ObjectId,
        pointer_id: &PointerId,
    ) -> Result<(), FsError> {
        let mut store = self.write_state();
        let pointer = store
            .pointers
            .get(pointer_id)
            .ok_or_else(|| FsError::PointerNotFound(pointer_id.clone()))?;
        if pointer.object_id != *object_id {
            return Err(FsError::PointerNotFound(pointer_id.clone()));
        }

        let object = store
            .objects
            .get_mut(object_id)
            .ok_or_else(|| FsError::ObjectNotFound(object_id.clone()))?;
        object.current_pointer_id = pointer_id.clone();
        store.capture_snapshot();
        drop(store);

        Ok(())
    }

    fn read_state(&self) -> RwLockReadGuard<'_, State> {
        match self.state.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write_state(&self) -> RwLockWriteGuard<'_, State> {
        match self.state.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl AiosFs for InMemoryAiosFs {
    async fn read_object(
        &self,
        object_id: &ObjectId,
        snapshot_id: Option<&SnapshotId>,
    ) -> Result<ObjectReadResult, FsError> {
        let (object, version, chunks, current_snapshot_id) = {
            let store = self.read_state();
            let current_snapshot_id = store.head_snapshot_id();
            ensure_snapshot_current(snapshot_id, &current_snapshot_id)?;

            let object = store
                .objects
                .get(object_id)
                .ok_or_else(|| FsError::ObjectNotFound(object_id.clone()))?;
            let pointer = store
                .pointers
                .get(&object.current_pointer_id)
                .ok_or_else(|| FsError::PointerNotFound(object.current_pointer_id.clone()))?;
            let version = store
                .versions
                .get(&pointer.current_version_id)
                .ok_or_else(|| FsError::VersionNotFound(pointer.current_version_id.clone()))?;

            deny_quarantined_read_unless_recovery(&store, object, version)?;

            let chunks = version
                .chunk_refs
                .iter()
                .map(|chunk_ref| {
                    store
                        .chunks
                        .get(&chunk_ref.0)
                        .cloned()
                        .ok_or_else(|| FsError::ChunkUnknown(chunk_ref.0.clone()))
                })
                .collect::<Result<Vec<_>, _>>()?;

            let object = object.clone();
            let version = version.clone();
            drop(store);

            (object, version, chunks, current_snapshot_id)
        };

        Ok(ObjectReadResult {
            object,
            version,
            chunks,
            snapshot_id: current_snapshot_id,
        })
    }

    async fn write_object(
        &self,
        write: ObjectWriteRequest,
        context: &FsContext,
    ) -> Result<ObjectWriteResult, FsError> {
        {
            let mut store = self.write_state();
            let current_snapshot_id = store.head_snapshot_id();
            ensure_snapshot_current(context.expected_snapshot_id.as_ref(), &current_snapshot_id)?;

            let now = Utc::now();
            let transaction_id = TransactionId::new();
            let version_id = VersionId::new();
            let action_id = write
                .action_id
                .clone()
                .or_else(|| context.action_id.clone());

            let object_id = match write.object_id.clone() {
                Some(existing_object_id) => {
                    prepare_existing_object_write(
                        &store,
                        &existing_object_id,
                        &write.parent_version_ids,
                    )?;
                    existing_object_id
                }
                None => ObjectId::new(),
            };

            let chunk_ids_written = register_chunks(&mut store, &write.chunks, now);
            let content_hash = content_hash_for_chunk_refs(&write.chunks);

            let version = Version {
                version_id: version_id.clone(),
                object_id: object_id.clone(),
                parent_version_ids: write.parent_version_ids.clone(),
                chunk_refs: write.chunks.clone(),
                content_hash,
                metadata_delta: write.metadata_delta.clone(),
                created_by_action_id: action_id.clone(),
                created_by_transaction_id: Some(transaction_id.clone()),
                created_at: now,
                state: VersionState::Verified,
                quarantined_at: None,
                quarantine_reason: None,
            };

            let pointer_move = if write.object_id.is_some() {
                promote_existing_object_pointer(
                    &mut store,
                    &object_id,
                    &version_id,
                    &transaction_id,
                    now,
                )?
            } else {
                create_object_pointer_pair(
                    &mut store,
                    &write,
                    &object_id,
                    &version_id,
                    &transaction_id,
                    now,
                );
                None
            };

            store.versions.insert(version_id.clone(), version);
            store.transactions.insert(
                transaction_id.clone(),
                Transaction {
                    transaction_id: transaction_id.clone(),
                    subject: context.subject.clone(),
                    action_id,
                    started_at: now,
                    completed_at: None,
                    state: TransactionState::PendingTx,
                    writes: vec![WriteOp {
                        object_id: object_id.clone(),
                        created_version_id: version_id.clone(),
                        chunk_ids_written,
                    }],
                    pointer_moves: pointer_move.into_iter().collect(),
                    evidence_receipt_id: None,
                },
            );

            let snapshot_id_after = store.capture_snapshot().snapshot_id;
            drop(store);

            Ok(ObjectWriteResult {
                object_id,
                version_id,
                transaction_id,
                snapshot_id_after,
            })
        }
    }

    async fn list_versions(&self, object_id: &ObjectId) -> Result<Vec<Version>, FsError> {
        let versions = {
            let store = self.read_state();
            if !store.objects.contains_key(object_id) {
                return Err(FsError::ObjectNotFound(object_id.clone()));
            }

            let mut versions: Vec<Version> = store
                .versions
                .values()
                .filter(|version| version.object_id == *object_id)
                .cloned()
                .collect();
            drop(store);

            versions.sort_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.version_id.as_str().cmp(right.version_id.as_str()))
            });
            versions
        };

        Ok(versions)
    }

    async fn resolve_pointer(&self, pointer_id: &PointerId) -> Result<Pointer, FsError> {
        self.read_state()
            .pointers
            .get(pointer_id)
            .cloned()
            .ok_or_else(|| FsError::PointerNotFound(pointer_id.clone()))
    }

    async fn get_snapshot(&self, snapshot_id: &SnapshotId) -> Result<SnapshotSummary, FsError> {
        self.read_state()
            .snapshots
            .get(snapshot_id)
            .cloned()
            .ok_or_else(|| FsError::Internal(format!("snapshot not found: {snapshot_id}")))
    }
}

impl MutableAiosFs for InMemoryAiosFs {
    fn apply_quarantine_entry(
        &self,
        version_id: &VersionId,
        trigger: QuarantineTrigger,
        reason: &str,
    ) -> Result<QuarantineReceipt, FsError> {
        let mut store = self.write_state();
        let now = Utc::now();
        let transaction_id = TransactionId::new();
        let version = store
            .versions
            .get(version_id)
            .cloned()
            .ok_or_else(|| FsError::VersionNotFound(version_id.clone()))?;

        if version.state == VersionState::Quarantined {
            return Err(FsError::QuarantineAlreadyApplied(version_id.clone()));
        }

        let affected_pointer_ids = current_or_stable_pointers_to(&store, version_id);
        let fallback_target = if affected_pointer_ids.is_empty() {
            None
        } else {
            quarantine_pointer_fallback(&store, &version)?
        };

        if !affected_pointer_ids.is_empty() && fallback_target.is_none() {
            return Err(FsError::NoPriorStablePointer(version.object_id));
        }

        let version_mut = store
            .versions
            .get_mut(version_id)
            .ok_or_else(|| FsError::VersionNotFound(version_id.clone()))?;
        version_mut.state = VersionState::Quarantined;
        version_mut.quarantined_at = Some(now);
        version_mut.quarantine_reason = Some(reason.to_owned());

        if let Some(target_version_id) = fallback_target {
            for pointer_id in affected_pointer_ids {
                let pointer = store
                    .pointers
                    .get_mut(&pointer_id)
                    .ok_or_else(|| FsError::PointerNotFound(pointer_id.clone()))?;
                pointer.current_version_id = target_version_id.clone();
                pointer.last_promoted_at = now;
                pointer.last_promoted_by_transaction_id = transaction_id.clone();
            }
        }

        store.capture_snapshot();
        drop(store);

        Ok(QuarantineReceipt {
            quarantine_id: new_quarantine_id(),
            version_id: version_id.clone(),
            transitioned_at: now,
            trigger: Some(trigger),
            disposition: None,
            reason: reason.to_owned(),
        })
    }

    fn apply_quarantine_exit(
        &self,
        version_id: &VersionId,
        disposition: QuarantineDisposition,
        operator: &SubjectRef,
    ) -> Result<QuarantineReceipt, FsError> {
        let mut store = self.write_state();
        let now = Utc::now();
        let version = store
            .versions
            .get_mut(version_id)
            .ok_or_else(|| FsError::VersionNotFound(version_id.clone()))?;

        if version.state != VersionState::Quarantined {
            return Err(FsError::QuarantineNotApplied(version_id.clone()));
        }

        version.state = match disposition {
            QuarantineDisposition::Released => VersionState::Verified,
            QuarantineDisposition::Purged => VersionState::RetiredVersion,
        };
        version.quarantined_at = None;
        version.quarantine_reason = None;

        store.capture_snapshot();
        drop(store);

        Ok(QuarantineReceipt {
            quarantine_id: new_quarantine_id(),
            version_id: version_id.clone(),
            transitioned_at: now,
            trigger: None,
            disposition: Some(disposition),
            reason: format!("operator={}", operator.0),
        })
    }
}

#[derive(Debug, Default)]
struct State {
    objects: HashMap<ObjectId, Object>,
    versions: HashMap<VersionId, Version>,
    chunks: HashMap<ChunkId, Chunk>,
    pointers: HashMap<PointerId, Pointer>,
    transactions: HashMap<TransactionId, Transaction>,
    snapshots: HashMap<SnapshotId, SnapshotSummary>,
    head_snapshot_id: Option<SnapshotId>,
}

impl State {
    fn capture_snapshot(&mut self) -> SnapshotSummary {
        let snapshot_id = self.compute_snapshot_id();
        let summary = SnapshotSummary {
            snapshot_id: snapshot_id.clone(),
            at: Utc::now(),
            object_count: self.objects.len() as u64,
            pointer_count: self.pointers.len() as u64,
        };

        self.head_snapshot_id = Some(snapshot_id.clone());
        self.snapshots.insert(snapshot_id, summary.clone());
        summary
    }

    fn head_snapshot_id(&self) -> SnapshotId {
        self.head_snapshot_id
            .clone()
            .unwrap_or_else(|| self.compute_snapshot_id())
    }

    fn compute_snapshot_id(&self) -> SnapshotId {
        SnapshotId::compute(
            self.object_snapshot_entries(),
            self.pointer_snapshot_entries(),
            self.version_snapshot_entries(),
        )
    }

    fn object_snapshot_entries(&self) -> Vec<String> {
        self.objects
            .values()
            .map(|object| {
                format!(
                    "{}:{}",
                    object.object_id.as_str(),
                    object.current_pointer_id.as_str()
                )
            })
            .collect()
    }

    fn pointer_snapshot_entries(&self) -> Vec<String> {
        self.pointers
            .values()
            .map(|pointer| {
                format!(
                    "{}:{}:{}:{}",
                    pointer.pointer_id.as_str(),
                    pointer.object_id.as_str(),
                    pointer_kind_label(pointer.kind),
                    pointer.current_version_id.as_str()
                )
            })
            .collect()
    }

    fn version_snapshot_entries(&self) -> Vec<String> {
        self.versions
            .values()
            .map(|version| {
                let chunks = version
                    .chunk_refs
                    .iter()
                    .map(|chunk_ref| chunk_ref.0.as_str())
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "{}:{}:{}:{}",
                    version.version_id.as_str(),
                    version.object_id.as_str(),
                    version_state_label(version.state),
                    chunks
                )
            })
            .collect()
    }
}

fn current_or_stable_pointers_to(store: &State, version_id: &VersionId) -> Vec<PointerId> {
    let mut pointer_ids: Vec<PointerId> = store
        .pointers
        .values()
        .filter(|pointer| {
            matches!(pointer.kind, PointerKind::Current | PointerKind::Stable)
                && pointer.current_version_id == *version_id
        })
        .map(|pointer| pointer.pointer_id.clone())
        .collect();

    pointer_ids.sort_by(|left, right| left.as_str().cmp(right.as_str()));
    pointer_ids
}

fn quarantine_pointer_fallback(
    store: &State,
    version: &Version,
) -> Result<Option<VersionId>, FsError> {
    if let Some(rollback_target) = pointer_target_for_kind(
        store,
        &version.object_id,
        PointerKind::Rollback,
        &version.version_id,
    )? {
        return Ok(Some(rollback_target));
    }

    if let Some(stable_pointer_target) = pointer_target_for_kind(
        store,
        &version.object_id,
        PointerKind::Stable,
        &version.version_id,
    )? {
        return Ok(Some(stable_pointer_target));
    }

    prior_verified_parent(store, version)
}

fn pointer_target_for_kind(
    store: &State,
    object_id: &ObjectId,
    kind: PointerKind,
    excluded_version_id: &VersionId,
) -> Result<Option<VersionId>, FsError> {
    let mut pointers: Vec<&Pointer> = store
        .pointers
        .values()
        .filter(|pointer| {
            pointer.object_id == *object_id
                && pointer.kind == kind
                && pointer.current_version_id != *excluded_version_id
        })
        .collect();
    pointers.sort_by(|left, right| left.pointer_id.as_str().cmp(right.pointer_id.as_str()));

    if let Some(pointer) = pointers.first() {
        let target = version_for_object(store, object_id, &pointer.current_version_id)?;
        return Ok(Some(target.version_id.clone()));
    }

    Ok(None)
}

fn prior_verified_parent(store: &State, version: &Version) -> Result<Option<VersionId>, FsError> {
    for parent_version_id in &version.parent_version_ids {
        let parent = version_for_object(store, &version.object_id, parent_version_id)?;
        if parent.state == VersionState::Verified {
            return Ok(Some(parent.version_id.clone()));
        }
    }

    Ok(None)
}

fn version_for_object<'a>(
    store: &'a State,
    object_id: &ObjectId,
    version_id: &VersionId,
) -> Result<&'a Version, FsError> {
    let version = store
        .versions
        .get(version_id)
        .ok_or_else(|| FsError::VersionNotFound(version_id.clone()))?;
    if version.object_id != *object_id {
        return Err(FsError::VersionNotFound(version_id.clone()));
    }

    Ok(version)
}

fn ensure_snapshot_current(
    found: Option<&SnapshotId>,
    expected: &SnapshotId,
) -> Result<(), FsError> {
    if let Some(found) = found {
        if found != expected {
            return Err(FsError::SnapshotStale {
                expected: expected.clone(),
                found: found.clone(),
            });
        }
    }

    Ok(())
}

fn prepare_existing_object_write(
    store: &State,
    object_id: &ObjectId,
    parent_version_ids: &[VersionId],
) -> Result<(), FsError> {
    if !store.objects.contains_key(object_id) {
        return Err(FsError::ObjectNotFound(object_id.clone()));
    }

    if parent_version_ids.is_empty() {
        return Err(FsError::WriteRequiresParent);
    }

    for parent_version_id in parent_version_ids {
        let parent = store
            .versions
            .get(parent_version_id)
            .ok_or_else(|| FsError::VersionNotFound(parent_version_id.clone()))?;

        if parent.object_id != *object_id {
            return Err(FsError::VersionNotFound(parent_version_id.clone()));
        }
    }

    Ok(())
}

fn register_chunks(store: &mut State, chunk_refs: &[ChunkRef], now: DateTime<Utc>) -> Vec<ChunkId> {
    let mut chunk_ids = Vec::with_capacity(chunk_refs.len());

    for chunk_ref in chunk_refs {
        let chunk_id = chunk_ref.0.clone();
        chunk_ids.push(chunk_id.clone());

        store
            .chunks
            .entry(chunk_id.clone())
            .and_modify(|chunk| {
                chunk.ref_count = chunk.ref_count.saturating_add(1);
            })
            .or_insert(Chunk {
                chunk_id,
                size_bytes: 0,
                ref_count: 1,
                created_at: now,
            });
    }

    chunk_ids
}

fn promote_existing_object_pointer(
    store: &mut State,
    object_id: &ObjectId,
    version_id: &VersionId,
    transaction_id: &TransactionId,
    now: DateTime<Utc>,
) -> Result<Option<PointerMoveOp>, FsError> {
    let pointer_id = store
        .objects
        .get(object_id)
        .ok_or_else(|| FsError::ObjectNotFound(object_id.clone()))?
        .current_pointer_id
        .clone();
    let pointer = store
        .pointers
        .get_mut(&pointer_id)
        .ok_or_else(|| FsError::PointerNotFound(pointer_id.clone()))?;
    let expected_current_version_id = pointer.current_version_id.clone();

    pointer.current_version_id = version_id.clone();
    pointer.last_promoted_at = now;
    pointer.last_promoted_by_transaction_id = transaction_id.clone();

    Ok(Some(PointerMoveOp {
        pointer_id,
        expected_current_version_id,
        new_version_id: version_id.clone(),
    }))
}

fn create_object_pointer_pair(
    store: &mut State,
    write: &ObjectWriteRequest,
    object_id: &ObjectId,
    version_id: &VersionId,
    transaction_id: &TransactionId,
    now: DateTime<Utc>,
) {
    let pointer_id = PointerId::new();
    let pointer = Pointer {
        pointer_id: pointer_id.clone(),
        object_id: object_id.clone(),
        kind: PointerKind::Current,
        current_version_id: version_id.clone(),
        last_promoted_at: now,
        last_promoted_by_transaction_id: transaction_id.clone(),
    };
    let object = Object::new(ObjectInit {
        object_id: object_id.clone(),
        kind: ObjectKind::File,
        created_at: now,
        created_by: write.subject.clone(),
        current_pointer_id: pointer_id.clone(),
        metadata: metadata_from_delta(object_id, &write.metadata_delta),
        privacy_class: PrivacyClass::Sensitive,
        scope_binding: ScopeBinding {
            scope_kind: ScopeKind::System,
            group_id: None,
            user_id: None,
        },
    });

    store.pointers.insert(pointer_id, pointer);
    store.objects.insert(object_id.clone(), object);
}

fn metadata_from_delta(object_id: &ObjectId, delta: &serde_json::Value) -> ObjectMetadata {
    let name = delta
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| object_id.as_str().to_owned(), ToOwned::to_owned);
    let labels = delta
        .get("labels")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let mime = delta
        .get("mime")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| "application/octet-stream".to_owned(), ToOwned::to_owned);

    ObjectMetadata {
        name,
        labels,
        mime,
        extra: delta.clone(),
    }
}

fn content_hash_for_chunk_refs(chunk_refs: &[ChunkRef]) -> String {
    let ordered_chunk_ids: Vec<&str> = chunk_refs
        .iter()
        .map(|chunk_ref| chunk_ref.0.as_str())
        .collect();
    let canonical = canonicalize_for_hash(&ordered_chunk_ids);

    // T-037 receives only ChunkRef metadata, not raw chunk payloads. The full writer
    // driver that can hash canonical concatenated bytes lands after this harness; for
    // now the in-memory version hash is deterministic over the ordered chunk ids.
    blake3_hash(canonical.as_bytes())
}

fn canonicalize_for_hash<T: Serialize>(value: &T) -> String {
    match jcs_canonicalize(value) {
        Ok(canonical) => canonical,
        Err(err) => format!("{{\"canonicalization_error\":\"{err}\"}}"),
    }
}

fn deny_quarantined_read_unless_recovery(
    store: &State,
    object: &Object,
    version: &Version,
) -> Result<(), FsError> {
    if version.state != VersionState::Quarantined {
        return Ok(());
    }

    let subject = version
        .created_by_transaction_id
        .as_ref()
        .and_then(|transaction_id| store.transactions.get(transaction_id))
        .map_or(&object.created_by, |transaction| &transaction.subject);

    // M6 will land the real L4 identity check. T-037 keeps the recovery-set
    // heuristic intentionally narrow and prefix-based per the task brief.
    if is_recovery_subject(subject) {
        return Ok(());
    }

    Err(FsError::QuarantineViolation(format!(
        "read of quarantined version {} denied",
        version.version_id
    )))
}

fn is_recovery_subject(subject: &SubjectRef) -> bool {
    subject.0.starts_with("_system:recovery") || subject.0.starts_with("agent:recovery")
}

const fn pointer_kind_label(kind: PointerKind) -> &'static str {
    match kind {
        PointerKind::Current => "CURRENT",
        PointerKind::Stable => "STABLE",
        PointerKind::Candidate => "CANDIDATE",
        PointerKind::Rollback => "ROLLBACK",
        PointerKind::Quarantine => "QUARANTINE",
    }
}

const fn version_state_label(state: VersionState) -> &'static str {
    match state {
        VersionState::Staged => "STAGED",
        VersionState::Verified => "VERIFIED",
        VersionState::Quarantined => "QUARANTINED",
        VersionState::RetiredVersion => "RETIRED_VERSION",
    }
}
