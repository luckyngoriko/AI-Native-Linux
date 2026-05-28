#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::missing_const_for_fn,
    clippy::missing_errors_doc,
    clippy::unused_async
)]

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::device_record::HardwareDeviceRecord;
use crate::error::HardwareError;
use crate::graph::HardwareGraph;
use crate::ids::{DeviceId, HardwareGraphId};

// -- DriftSignal ----------------------------------------------------------

/// The result of comparing a current-boot [`HardwareGraph`] against the
/// prior-boot graph snapshot stored in [`PriorGraphStore`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriftSignal {
    /// Graph id unchanged since last boot.
    NoDrift,
    /// No prior graph on record — this is the first boot (or store was cleared).
    FirstBoot { current: HardwareGraphId },
    /// Graph id differs from prior boot.  Carries the device-level diff so
    /// callers can decide whether to emit a candidate L0 constitutional
    /// invariant (`HARDWARE_GRAPH_DRIFT_FOREVER`).
    DriftDetected {
        prior: HardwareGraphId,
        current: HardwareGraphId,
        change: GraphDiff,
    },
}

// -- GraphDiff -------------------------------------------------------------

/// Device-level difference between two hardware graph snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphDiff {
    pub added: Vec<DeviceId>,
    pub removed: Vec<DeviceId>,
    pub modified: Vec<DeviceId>,
    pub kept: usize,
}

// -- PriorGraphStore -------------------------------------------------------

/// In-memory holder of the prior-boot [`HardwareGraph`] id and device map.
///
/// Thread-safe: all fields behind [`RwLock`].
pub struct PriorGraphStore {
    prior_id: RwLock<Option<HardwareGraphId>>,
    prior_devices: RwLock<Option<BTreeMap<DeviceId, HardwareDeviceRecord>>>,
}

impl PriorGraphStore {
    #[must_use]
    pub fn new() -> Self {
        Self {
            prior_id: RwLock::new(None),
            prior_devices: RwLock::new(None),
        }
    }

    #[must_use]
    pub fn with_prior(graph: HardwareGraph) -> Self {
        Self {
            prior_id: RwLock::new(Some(graph.id)),
            prior_devices: RwLock::new(Some(graph.devices)),
        }
    }

    /// Return the currently stored prior graph id, if any.
    ///
    /// # Panics
    /// Panics if the inner [`RwLock`] is poisoned.
    pub async fn current(&self) -> Option<HardwareGraphId> {
        self.prior_id
            .read()
            .expect("PriorGraphStore RwLock poisoned")
            .clone()
    }

    /// Replace the stored prior graph with the given snapshot's id and device map.
    ///
    /// # Panics
    /// Panics if the inner [`RwLock`] is poisoned.
    pub async fn store(&self, graph: &HardwareGraph) {
        *self
            .prior_id
            .write()
            .expect("PriorGraphStore RwLock poisoned") = Some(graph.id.clone());
        *self
            .prior_devices
            .write()
            .expect("PriorGraphStore RwLock poisoned") = Some(graph.devices.clone());
    }

    /// Clear the stored prior graph (both id and device map).
    ///
    /// # Panics
    /// Panics if the inner [`RwLock`] is poisoned.
    pub async fn clear(&self) {
        *self
            .prior_id
            .write()
            .expect("PriorGraphStore RwLock poisoned") = None;
        *self
            .prior_devices
            .write()
            .expect("PriorGraphStore RwLock poisoned") = None;
    }
}

impl Default for PriorGraphStore {
    fn default() -> Self {
        Self::new()
    }
}

// -- DriftDetector ---------------------------------------------------------

/// Compares a current-boot [`HardwareGraph`] against a [`PriorGraphStore`]
/// and produces a [`DriftSignal`].
pub struct DriftDetector {
    prior_store: Arc<PriorGraphStore>,
}

impl DriftDetector {
    #[must_use]
    pub fn new(prior_store: Arc<PriorGraphStore>) -> Self {
        Self { prior_store }
    }

    /// Check the current graph against the prior-boot store.
    ///
    /// - No prior stored → [`DriftSignal::FirstBoot`].
    /// - Same graph id → [`DriftSignal::NoDrift`].
    /// - Different id → [`DriftSignal::DriftDetected`] with a per-device [`GraphDiff`].
    ///
    /// # Panics
    /// Panics if the inner [`RwLock`] is poisoned.
    pub async fn check(&self, current: &HardwareGraph) -> Result<DriftSignal, HardwareError> {
        let prior_id = self.prior_store.current().await;
        match prior_id {
            None => Ok(DriftSignal::FirstBoot {
                current: current.id.clone(),
            }),
            Some(ref prior) if *prior == current.id => Ok(DriftSignal::NoDrift),
            Some(prior) => {
                let change = self
                    .prior_store
                    .prior_devices
                    .read()
                    .expect("PriorGraphStore RwLock poisoned")
                    .as_ref()
                    .map_or_else(
                        || {
                            let added: Vec<DeviceId> = current.devices.keys().cloned().collect();
                            GraphDiff {
                                added,
                                removed: Vec::new(),
                                modified: Vec::new(),
                                kept: 0,
                            }
                        },
                        |prior_devs| compute_graph_diff(prior_devs, &current.devices),
                    );
                Ok(DriftSignal::DriftDetected {
                    prior,
                    current: current.id.clone(),
                    change,
                })
            }
        }
    }
}

// -- compute_graph_diff ----------------------------------------------------

/// Partition device sets into added / removed / modified / kept.
fn compute_graph_diff(
    prior_devices: &BTreeMap<DeviceId, HardwareDeviceRecord>,
    current_devices: &BTreeMap<DeviceId, HardwareDeviceRecord>,
) -> GraphDiff {
    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();
    let mut kept = 0usize;

    for id in current_devices.keys() {
        if !prior_devices.contains_key(id) {
            added.push(id.clone());
        }
    }

    for id in prior_devices.keys() {
        if !current_devices.contains_key(id) {
            removed.push(id.clone());
        }
    }

    for id in current_devices.keys() {
        if let (Some(prior), Some(current)) = (prior_devices.get(id), current_devices.get(id)) {
            if prior.canonical_bytes() == current.canonical_bytes() {
                kept += 1;
            } else {
                modified.push(id.clone());
            }
        }
    }

    GraphDiff {
        added,
        removed,
        modified,
        kept,
    }
}

// -- EvilMaidEvidenceMarker ------------------------------------------------

/// Recommended operator or system action when hardware drift is detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvilMaidRecommendedAction {
    /// A non-removable device disappeared — possible physical tampering.
    EnterRecoveryMode,
    /// Only new devices appeared (no removals) — isolate them until approved.
    AutoQuarantineNewDevices,
    /// Only firmware/driver-level changes detected — operator should investigate.
    OperatorInvestigation,
}

/// Type-level marker for evil-maid attack evidence.
///
/// Evidence **emission** proper lands in T-173.  This struct carries the
/// forensic data needed to emit a candidate L0 invariant violation
/// (`HARDWARE_GRAPH_DRIFT_FOREVER` per S8.3 §6).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvilMaidEvidenceMarker {
    pub prior: HardwareGraphId,
    pub current: HardwareGraphId,
    pub diff: GraphDiff,
    pub detected_at: DateTime<Utc>,
    pub recommended_action: EvilMaidRecommendedAction,
}

impl EvilMaidEvidenceMarker {
    /// Build a marker from a drift signal.
    ///
    /// Returns `None` for [`DriftSignal::NoDrift`] and [`DriftSignal::FirstBoot`].
    ///
    /// Recommendation logic:
    /// - Any removed device that was non-removable → `EnterRecoveryMode`.
    /// - Any added device (no non-removable removals) → `AutoQuarantineNewDevices`.
    /// - Only modified devices → `OperatorInvestigation`.
    #[must_use]
    pub fn from_drift(
        signal: &DriftSignal,
        detected_at: DateTime<Utc>,
        prior_devices: Option<&BTreeMap<DeviceId, HardwareDeviceRecord>>,
    ) -> Option<Self> {
        match signal {
            DriftSignal::DriftDetected {
                prior,
                current,
                change,
            } => {
                let recommended_action = if !change.removed.is_empty() {
                    let has_non_removable_removed = prior_devices.is_none_or(|devs| {
                        change
                            .removed
                            .iter()
                            .any(|id| devs.get(id).is_none_or(|d| !d.removable))
                    });
                    if has_non_removable_removed {
                        EvilMaidRecommendedAction::EnterRecoveryMode
                    } else {
                        EvilMaidRecommendedAction::OperatorInvestigation
                    }
                } else if !change.added.is_empty() {
                    EvilMaidRecommendedAction::AutoQuarantineNewDevices
                } else {
                    EvilMaidRecommendedAction::OperatorInvestigation
                };

                Some(Self {
                    prior: prior.clone(),
                    current: current.clone(),
                    diff: change.clone(),
                    detected_at,
                    recommended_action,
                })
            }
            _ => None,
        }
    }
}
