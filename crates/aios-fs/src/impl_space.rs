//! Implementation-space bindings for S2.2 abstract-to-physical storage targets.

#![allow(
    clippy::module_name_repetitions,
    reason = "AIOS-FS public names mirror the spec vocabulary"
)]

use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::chunk::ChunkId;
use crate::error::FsError;
use crate::object::{ObjectId, SubjectRef};
use crate::version::VersionId;

/// Physical storage target for an AIOS-FS abstract object, chunk, or version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ImplSpaceTarget {
    /// Local filesystem path metadata. No I/O is performed by this M5 harness.
    LocalFile {
        /// Absolute or implementation-relative local path.
        path: String,
    },
    /// Encrypted blob metadata. Key handling is owned by later Vault work.
    EncryptedBlob {
        /// Backend blob identifier.
        blob_id: String,
        /// Capability id that authorizes key access.
        key_capability_id: String,
    },
    /// Remote object-store blob metadata. Network access is out of scope here.
    RemoteBlob {
        /// Remote blob URL.
        url: String,
        /// Optional backend entity tag.
        etag: Option<String>,
    },
    /// AIOS-FS managed handle for storage owned by the runtime.
    AiosFsManaged {
        /// Opaque runtime-managed handle.
        handle: String,
    },
}

/// Abstract source whose backing implementation-space binding is being recorded.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ImplSpaceSource {
    /// Binding for an object id.
    Object(ObjectId),
    /// Binding for a chunk id.
    Chunk(ChunkId),
    /// Binding for a version id.
    Version(VersionId),
}

/// Current integrity state of one implementation-space binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrityState {
    /// The binding was verified.
    Verified,
    /// The binding may be outdated and needs verification.
    Stale,
    /// Integrity verification failed.
    IntegrityFailed,
    /// No integrity result is known yet.
    Unknown,
}

/// Recorded mapping from an abstract AIOS-FS source to a physical target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct ImplSpaceBinding {
    /// Fresh binding id: `"ispb_<ULID>"`.
    pub binding_id: String,
    /// Abstract object, chunk, or version id being bound.
    pub object_or_chunk_id: ImplSpaceSource,
    /// Physical metadata target for the abstract source.
    pub target: ImplSpaceTarget,
    /// Binding creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Subject that recorded the binding.
    pub created_by: SubjectRef,
    /// Last timestamp at which the binding was verified.
    pub last_verified_at: Option<DateTime<Utc>>,
    /// Stored integrity state. T-041 verify returns this state without I/O.
    pub integrity_state: IntegrityState,
}

/// S2.2 implementation-space catalog.
///
/// Implementations are `Send + Sync` so the catalog can be shared behind an
/// `Arc<dyn ImplSpace>` by later RPC and projection tasks.
#[async_trait]
pub trait ImplSpace: Send + Sync {
    /// Resolve all known bindings for a source.
    ///
    /// # Errors
    ///
    /// Backend implementations may return storage/catalog errors. The in-memory
    /// implementation returns an empty vector for unknown sources.
    async fn resolve(&self, source: &ImplSpaceSource) -> Result<Vec<ImplSpaceBinding>, FsError>;

    /// Record a new binding.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::Internal`] when a binding id is already present.
    async fn record_binding(&self, binding: ImplSpaceBinding) -> Result<(), FsError>;

    /// Verify a binding and return its current integrity state.
    ///
    /// # Errors
    ///
    /// Returns [`FsError::ImplSpaceBindingNotFound`] when `binding_id` is unknown.
    async fn verify(&self, binding_id: &str) -> Result<IntegrityState, FsError>;

    /// List all bindings recorded for a source.
    ///
    /// # Errors
    ///
    /// Backend implementations may return storage/catalog errors. The in-memory
    /// implementation returns an empty vector for unknown sources.
    async fn list_for(&self, source: &ImplSpaceSource) -> Result<Vec<ImplSpaceBinding>, FsError>;
}

/// In-memory implementation-space catalog for T-041 tests and later harness work.
#[derive(Debug, Clone)]
pub struct InMemoryImplSpace {
    bindings: Arc<RwLock<HashMap<ImplSpaceSource, Vec<ImplSpaceBinding>>>>,
}

impl Default for InMemoryImplSpace {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryImplSpace {
    /// Construct an empty implementation-space catalog.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Construct a catalog with three canonical metadata-only fixtures.
    #[must_use]
    pub fn with_fixtures() -> Self {
        let fixture = Self::new();

        {
            let mut bindings = fixture.write_bindings();
            for binding in canonical_bindings() {
                bindings
                    .entry(binding.object_or_chunk_id.clone())
                    .or_default()
                    .push(binding);
            }
        }

        fixture
    }

    fn read_bindings(
        &self,
    ) -> RwLockReadGuard<'_, HashMap<ImplSpaceSource, Vec<ImplSpaceBinding>>> {
        match self.bindings.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn write_bindings(
        &self,
    ) -> RwLockWriteGuard<'_, HashMap<ImplSpaceSource, Vec<ImplSpaceBinding>>> {
        match self.bindings.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[async_trait]
impl ImplSpace for InMemoryImplSpace {
    async fn resolve(&self, source: &ImplSpaceSource) -> Result<Vec<ImplSpaceBinding>, FsError> {
        self.list_for(source).await
    }

    async fn record_binding(&self, binding: ImplSpaceBinding) -> Result<(), FsError> {
        let mut bindings = self.write_bindings();
        if bindings
            .values()
            .flatten()
            .any(|stored| stored.binding_id == binding.binding_id)
        {
            return Err(FsError::Internal(format!(
                "duplicate impl-space binding: {}",
                binding.binding_id
            )));
        }

        bindings
            .entry(binding.object_or_chunk_id.clone())
            .or_default()
            .push(binding);
        drop(bindings);
        Ok(())
    }

    async fn verify(&self, binding_id: &str) -> Result<IntegrityState, FsError> {
        self.read_bindings()
            .values()
            .flatten()
            .find(|binding| binding.binding_id == binding_id)
            .map(|binding| binding.integrity_state)
            .ok_or_else(|| FsError::ImplSpaceBindingNotFound(binding_id.to_owned()))
    }

    async fn list_for(&self, source: &ImplSpaceSource) -> Result<Vec<ImplSpaceBinding>, FsError> {
        Ok(self
            .read_bindings()
            .get(source)
            .cloned()
            .unwrap_or_default())
    }
}

fn canonical_bindings() -> Vec<ImplSpaceBinding> {
    let created_at = fixture_time();
    let created_by = SubjectRef("family:alice".to_owned());
    vec![
        ImplSpaceBinding {
            binding_id: "ispb_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
            object_or_chunk_id: ImplSpaceSource::Object(fixture_object_id()),
            target: ImplSpaceTarget::LocalFile {
                path: "/aios/store/objects/obj_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
            },
            created_at,
            created_by: created_by.clone(),
            last_verified_at: Some(created_at),
            integrity_state: IntegrityState::Verified,
        },
        ImplSpaceBinding {
            binding_id: "ispb_01HXY8K2JPQ7N3M4R5S6T7V8W8".to_owned(),
            object_or_chunk_id: ImplSpaceSource::Chunk(fixture_chunk_id()),
            target: ImplSpaceTarget::EncryptedBlob {
                blob_id: "blob/impl-space/chunk".to_owned(),
                key_capability_id: "cap/key/impl-space".to_owned(),
            },
            created_at,
            created_by: created_by.clone(),
            last_verified_at: None,
            integrity_state: IntegrityState::Unknown,
        },
        ImplSpaceBinding {
            binding_id: "ispb_01HXY8K2JPQ7N3M4R5S6T7V8W7".to_owned(),
            object_or_chunk_id: ImplSpaceSource::Version(fixture_version_id()),
            target: ImplSpaceTarget::AiosFsManaged {
                handle: "managed/version/ver_01HXY8K2JPQ7N3M4R5S6T7V8W9".to_owned(),
            },
            created_at,
            created_by,
            last_verified_at: None,
            integrity_state: IntegrityState::Stale,
        },
    ]
}

fn fixture_time() -> DateTime<Utc> {
    Utc::now()
}

fn fixture_object_id() -> ObjectId {
    ObjectId::parse("obj_01HXY8K2JPQ7N3M4R5S6T7V8W9").unwrap_or_else(|_| ObjectId::new())
}

fn fixture_version_id() -> VersionId {
    VersionId::parse("ver_01HXY8K2JPQ7N3M4R5S6T7V8W9").unwrap_or_else(|_| VersionId::new())
}

fn fixture_chunk_id() -> ChunkId {
    ChunkId::from_hash_bytes(b"impl-space chunk")
}
