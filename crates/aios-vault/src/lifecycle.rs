//! Capability expiration driver for the in-memory vault broker.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S5.2 vault broker vocabulary"
)]

use std::sync::Arc;

use chrono::{DateTime, Utc};
use ulid::Ulid;

use crate::audit::CapabilityAuditLog;
use crate::capability::{CapabilityId, CapabilityState};
use crate::error::VaultError;
use crate::in_memory_broker::InMemoryVaultBroker;

/// Drives capability lifecycle transitions that are not direct use/revoke calls.
#[derive(Debug, Clone)]
pub struct CapabilityLifecycleDriver {
    broker: Arc<InMemoryVaultBroker>,
    audit: Arc<CapabilityAuditLog>,
}

impl CapabilityLifecycleDriver {
    /// Construct a lifecycle driver over the in-memory broker.
    #[must_use]
    pub const fn new(broker: Arc<InMemoryVaultBroker>, audit: Arc<CapabilityAuditLog>) -> Self {
        Self { broker, audit }
    }

    /// Expire one active capability.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError::CapabilityNotFound`] for unknown ids and
    /// [`VaultError::InvalidTransition`] when the capability is not active.
    pub async fn expire_capability(&self, capability_id: &CapabilityId) -> Result<(), VaultError> {
        let mut store = self.broker.capabilities.write().await;
        let (capability, _key_material) = store
            .get_mut(capability_id)
            .ok_or_else(|| VaultError::CapabilityNotFound(capability_id.clone()))?;

        if capability.state != CapabilityState::Active {
            return Err(VaultError::InvalidTransition {
                from: capability.state,
                to: CapabilityState::Expired,
            });
        }

        capability.state = CapabilityState::Expired;
        drop(store);

        self.audit.record_expire(capability_id);
        Ok(())
    }

    /// Expire all active capabilities whose hard expiry is before `now`.
    ///
    /// # Errors
    ///
    /// Returns [`VaultError`] if the backing broker cannot be inspected.
    pub async fn run_expiration_pass(
        &self,
        now: DateTime<Utc>,
    ) -> Result<ExpirationPassReport, VaultError> {
        let started_at = Utc::now();
        let mut expired_capability_ids = Vec::new();

        let mut store = self.broker.capabilities.write().await;
        let capabilities_inspected = u64::try_from(store.len()).map_err(|_| {
            VaultError::Internal("capability store length is not representable as u64".to_owned())
        })?;

        for (capability_id, (capability, _key_material)) in &mut *store {
            if capability.state == CapabilityState::Active
                && capability
                    .expires_at
                    .is_some_and(|expires_at| expires_at < now)
            {
                capability.state = CapabilityState::Expired;
                expired_capability_ids.push(capability_id.clone());
            }
        }
        drop(store);

        for capability_id in &expired_capability_ids {
            self.audit.record_expire(capability_id);
        }

        Ok(ExpirationPassReport {
            pass_id: format!("expp_{}", Ulid::new()),
            started_at,
            completed_at: Utc::now(),
            capabilities_inspected,
            capabilities_expired: u64::try_from(expired_capability_ids.len()).map_err(|_| {
                VaultError::Internal(
                    "expired capability count is not representable as u64".to_owned(),
                )
            })?,
        })
    }
}

/// Summary of one expiration pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpirationPassReport {
    /// Fresh pass identifier.
    pub pass_id: String,
    /// Wall-clock pass start timestamp.
    pub started_at: DateTime<Utc>,
    /// Wall-clock pass completion timestamp.
    pub completed_at: DateTime<Utc>,
    /// Number of capability records inspected.
    pub capabilities_inspected: u64,
    /// Number of active capabilities moved to expired.
    pub capabilities_expired: u64,
}
