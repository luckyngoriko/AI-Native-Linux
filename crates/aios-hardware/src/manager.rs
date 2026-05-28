#![allow(missing_docs)]

use std::collections::BTreeMap;

use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use tokio::sync::RwLock;

use crate::device_record::HardwareDeviceRecord;
use crate::error::HardwareError;
use crate::graph::{HardwareGraph, HardwareGraphBuilder};
use crate::ids::DeviceId;
use crate::lifecycle::DeviceLifecycleState;

// -- trait -----------------------------------------------------------------

/// Service interface for the hardware-manager subsystem (S8.3).
#[async_trait]
pub trait HardwareManager: Send + Sync {
    /// Return the most recently built graph snapshot, if any.
    async fn current_graph(&self) -> Option<HardwareGraph>;

    /// Capture a fresh graph snapshot from the current pending device set
    /// and store it as `current_graph`.
    async fn rebuild_graph(
        &self,
        host_canonical_id: &str,
        signer: &SigningKey,
        signer_fingerprint: &str,
    ) -> Result<HardwareGraph, HardwareError>;

    /// Register a device into the pending set.  Duplicate `device_id` is
    /// rejected with [`HardwareError::Internal`].
    async fn register_device(&self, record: HardwareDeviceRecord) -> Result<(), HardwareError>;

    /// Remove a device from the pending set.
    async fn deregister_device(&self, device_id: &DeviceId) -> Result<(), HardwareError>;

    /// List every device currently in the pending set.
    async fn list_pending_devices(&self) -> Vec<HardwareDeviceRecord>;

    /// Look up a single device by id in the pending set.
    async fn get_device(&self, device_id: &DeviceId)
        -> Result<HardwareDeviceRecord, HardwareError>;

    /// Transition the lifecycle state of a pending device.
    async fn set_device_lifecycle(
        &self,
        device_id: &DeviceId,
        state: DeviceLifecycleState,
    ) -> Result<(), HardwareError>;
}

// -- in-memory impl --------------------------------------------------------

struct HardwareManagerState {
    pending: BTreeMap<DeviceId, HardwareDeviceRecord>,
    current_graph: Option<HardwareGraph>,
}

/// In-memory [`HardwareManager`] backed by a `RwLock`.
pub struct InMemoryHardwareManager {
    state: RwLock<HardwareManagerState>,
}

impl InMemoryHardwareManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: RwLock::new(HardwareManagerState {
                pending: BTreeMap::new(),
                current_graph: None,
            }),
        }
    }

    #[must_use]
    pub fn with_graph(initial: HardwareGraph) -> Self {
        Self {
            state: RwLock::new(HardwareManagerState {
                pending: BTreeMap::new(),
                current_graph: Some(initial),
            }),
        }
    }
}

impl Default for InMemoryHardwareManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl HardwareManager for InMemoryHardwareManager {
    async fn current_graph(&self) -> Option<HardwareGraph> {
        self.state.read().await.current_graph.clone()
    }

    async fn rebuild_graph(
        &self,
        host_canonical_id: &str,
        signer: &SigningKey,
        signer_fingerprint: &str,
    ) -> Result<HardwareGraph, HardwareError> {
        let state = self.state.read().await;
        let mut builder = HardwareGraphBuilder::new(host_canonical_id);
        for record in state.pending.values() {
            builder.add_device(record.clone())?;
        }
        let graph = builder.build_and_sign(signer, signer_fingerprint)?;
        drop(state);

        let mut w = self.state.write().await;
        w.current_graph = Some(graph.clone());
        drop(w);
        Ok(graph)
    }

    async fn register_device(&self, record: HardwareDeviceRecord) -> Result<(), HardwareError> {
        let mut state = self.state.write().await;
        if state.pending.contains_key(&record.device_id) {
            return Err(HardwareError::Internal("duplicate device_id".into()));
        }
        state.pending.insert(record.device_id.clone(), record);
        drop(state);
        Ok(())
    }

    async fn deregister_device(&self, device_id: &DeviceId) -> Result<(), HardwareError> {
        let mut state = self.state.write().await;
        state
            .pending
            .remove(device_id)
            .map(|_| ())
            .ok_or_else(|| HardwareError::DeviceNotFound(device_id.clone()))
    }

    async fn list_pending_devices(&self) -> Vec<HardwareDeviceRecord> {
        self.state.read().await.pending.values().cloned().collect()
    }

    async fn get_device(
        &self,
        device_id: &DeviceId,
    ) -> Result<HardwareDeviceRecord, HardwareError> {
        self.state
            .read()
            .await
            .pending
            .get(device_id)
            .cloned()
            .ok_or_else(|| HardwareError::DeviceNotFound(device_id.clone()))
    }

    async fn set_device_lifecycle(
        &self,
        device_id: &DeviceId,
        lifecycle_state: DeviceLifecycleState,
    ) -> Result<(), HardwareError> {
        let mut w = self.state.write().await;
        let record = w
            .pending
            .get_mut(device_id)
            .ok_or_else(|| HardwareError::DeviceNotFound(device_id.clone()))?;
        record.lifecycle = lifecycle_state;
        drop(w);
        Ok(())
    }
}
