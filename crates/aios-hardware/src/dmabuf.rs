#![allow(
    missing_docs,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::significant_drop_tightening,
    clippy::missing_panics_doc
)]

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::error::HardwareError;
use crate::ids::GpuId;

// ---------------------------------------------------------------------------
// S8.2 §7 — DmabufHandle: typed dmabuf handle carrying source attribution
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DmabufHandle {
    pub handle_id: String,
    pub source_gpu: GpuId,
    pub source_group: String,
    pub source_subject: String,
    pub size_bytes: u64,
    pub format_code: u32,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// S8.2 §7 — DmabufPeer: a single authorized import target
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DmabufPeer {
    pub target_gpu: GpuId,
    pub target_group: String,
    pub target_subject: String,
}

// ---------------------------------------------------------------------------
// S8.2 §7 — DmabufPeerSet: authorized peer set sealed by a policy decision
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct DmabufPeerSet {
    pub handle_id: String,
    pub authorized_peers: Vec<DmabufPeer>,
    pub policy_decision_id: String,
}

// ---------------------------------------------------------------------------
// S8.2 §7 — DmabufBroker: cross-GPU dmabuf import enforcement
// ---------------------------------------------------------------------------

pub struct DmabufBroker {
    handles: RwLock<HashMap<String, DmabufHandle>>,
    peer_sets: RwLock<HashMap<String, DmabufPeerSet>>,
}

impl DmabufBroker {
    pub fn new() -> Self {
        Self {
            handles: RwLock::new(HashMap::new()),
            peer_sets: RwLock::new(HashMap::new()),
        }
    }

    pub async fn create_handle(&self, handle: DmabufHandle) -> Result<(), HardwareError> {
        let mut handles = self.handles.write().await;
        if handles.contains_key(&handle.handle_id) {
            return Err(HardwareError::Internal("duplicate dmabuf handle".into()));
        }
        handles.insert(handle.handle_id.clone(), handle);
        Ok(())
    }

    pub async fn authorize_peer_set(&self, peer_set: DmabufPeerSet) -> Result<(), HardwareError> {
        let handles = self.handles.read().await;
        if !handles.contains_key(&peer_set.handle_id) {
            return Err(HardwareError::Internal("unknown dmabuf handle".into()));
        }
        drop(handles);

        let mut peer_sets = self.peer_sets.write().await;
        peer_sets.insert(peer_set.handle_id.clone(), peer_set);
        Ok(())
    }

    pub async fn check_import(
        &self,
        handle_id: &str,
        target_gpu: &GpuId,
        target_group: &str,
        target_subject: &str,
    ) -> Result<(), HardwareError> {
        let handles = self.handles.read().await;
        let handle = handles
            .get(handle_id)
            .cloned()
            .ok_or_else(|| HardwareError::Internal("unknown dmabuf handle".into()))?;
        let source_gpu = handle.source_gpu.clone();
        drop(handles);

        let peer_sets = self.peer_sets.read().await;
        let peer_set =
            peer_sets
                .get(handle_id)
                .ok_or_else(|| HardwareError::DmabufPeerUnauthorized {
                    src: source_gpu.clone(),
                    target: target_gpu.clone(),
                })?;

        let authorized = peer_set.authorized_peers.iter().any(|peer| {
            &peer.target_gpu == target_gpu
                && peer.target_group == target_group
                && peer.target_subject == target_subject
        });

        if authorized {
            Ok(())
        } else {
            Err(HardwareError::DmabufPeerUnauthorized {
                src: source_gpu,
                target: target_gpu.clone(),
            })
        }
    }

    pub async fn revoke_handle(&self, handle_id: &str) -> Result<(), HardwareError> {
        let mut handles = self.handles.write().await;
        if handles.remove(handle_id).is_none() {
            return Err(HardwareError::Internal("unknown dmabuf handle".into()));
        }
        drop(handles);

        let mut peer_sets = self.peer_sets.write().await;
        peer_sets.remove(handle_id);
        Ok(())
    }

    pub async fn list_handles(&self) -> Vec<DmabufHandle> {
        self.handles.read().await.values().cloned().collect()
    }

    pub async fn list_peer_sets(&self) -> Vec<DmabufPeerSet> {
        self.peer_sets.read().await.values().cloned().collect()
    }
}

impl Default for DmabufBroker {
    fn default() -> Self {
        Self::new()
    }
}
