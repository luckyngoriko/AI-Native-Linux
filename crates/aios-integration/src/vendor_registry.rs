use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use chrono::Utc;
use ed25519_dalek::{Signature, VerifyingKey};

use crate::error::IntegrationError;
use crate::ids::VendorContractId;
use crate::lifecycle::{IntegrationLifecycleLabel, IntegrationLifecycleState};
use crate::vendor::{VendorIntegrationContract, VendorKind, VendorTrustClass};

fn canonical_contract_bytes(contract: &VendorIntegrationContract) -> Vec<u8> {
    let mut s = String::new();
    s.push_str(&contract.contract_id.0);
    s.push('\n');
    s.push_str(&contract.vendor_name);
    s.push('\n');
    s.push_str(contract.vendor_kind.label());
    s.push('\n');
    s.push_str(contract.trust_class.label());
    s.push('\n');
    s.push_str(&contract.contact_canonical_id);
    s.push('\n');
    s.push_str(&contract.rotation_cadence_days.to_string());
    s.push('\n');
    s.push_str(&contract.breach_playbook_url);
    s.into_bytes()
}

fn lock_poisoned() -> IntegrationError {
    IntegrationError::Internal("lock poisoned".into())
}

/// Whether a lifecycle transition from `from` to `to` is valid, optionally
/// consulting additional guard state on `current`.
#[allow(clippy::unnested_or_patterns)]
const fn is_transition_allowed(
    from: IntegrationLifecycleLabel,
    to: IntegrationLifecycleLabel,
    current: &IntegrationLifecycleState,
) -> bool {
    match (from, to) {
        (IntegrationLifecycleLabel::Evaluated, IntegrationLifecycleLabel::Piloted) => matches!(
            current,
            IntegrationLifecycleState::Evaluated {
                security_audit_passed: true,
                ..
            }
        ),
        _ => matches!(
            (from, to),
            (
                IntegrationLifecycleLabel::Proposed,
                IntegrationLifecycleLabel::Evaluated | IntegrationLifecycleLabel::Retired
            ) | (
                IntegrationLifecycleLabel::Evaluated,
                IntegrationLifecycleLabel::Retired
            ) | (
                IntegrationLifecycleLabel::Piloted,
                IntegrationLifecycleLabel::Production
                    | IntegrationLifecycleLabel::Deprecated
                    | IntegrationLifecycleLabel::Retired,
            ) | (
                IntegrationLifecycleLabel::Production,
                IntegrationLifecycleLabel::Deprecated | IntegrationLifecycleLabel::Retired,
            ) | (
                IntegrationLifecycleLabel::Deprecated,
                IntegrationLifecycleLabel::Retired
            )
        ),
    }
}

/// Ed25519-signed vendor contract registry enforcing trust-class admission
/// discipline and the 6-state lifecycle FSM (S11.4 §2, invariant I2).
pub struct VendorIntegrationRegistry {
    contracts: RwLock<HashMap<VendorContractId, VendorIntegrationContract>>,
    lifecycle_states: RwLock<HashMap<VendorContractId, IntegrationLifecycleState>>,
    trusted_authorities: HashMap<String, VerifyingKey>,
    blacklist: RwLock<HashSet<String>>,
}

impl VendorIntegrationRegistry {
    /// Creates an empty registry with no trusted authorities.
    #[must_use]
    pub fn new() -> Self {
        Self {
            contracts: RwLock::new(HashMap::new()),
            lifecycle_states: RwLock::new(HashMap::new()),
            trusted_authorities: HashMap::new(),
            blacklist: RwLock::new(HashSet::new()),
        }
    }

    /// Registers a trusted signing authority keyed by fingerprint.
    pub fn register_authority(&mut self, fingerprint: &str, key: VerifyingKey) {
        self.trusted_authorities
            .insert(fingerprint.to_string(), key);
    }

    /// Admits a vendor contract after verifying the Ed25519 signature over
    /// canonical bytes and enforcing trust-class / blacklist guards.
    ///
    /// On success the contract is inserted and its lifecycle state is set to
    /// `Proposed`.
    ///
    /// # Errors
    ///
    /// Returns `VendorBlacklisted` if the trust class is
    /// `BlacklistedDoNotAdmit` or the vendor name appears in the blacklist.
    ///
    /// Returns `VendorContractSignatureInvalid` if the signing authority is
    /// unknown, the Ed25519 signature is invalid, or the `contract_id` is
    /// already admitted.
    #[allow(clippy::unused_async)]
    pub async fn admit_contract(
        &self,
        contract: VendorIntegrationContract,
    ) -> Result<(), IntegrationError> {
        let contract_id = contract.contract_id.clone();

        if contract.trust_class == VendorTrustClass::BlacklistedDoNotAdmit {
            return Err(IntegrationError::VendorBlacklisted { contract_id });
        }

        {
            let blacklist = self.blacklist.read().map_err(|_| lock_poisoned())?;
            if blacklist.contains(&contract.vendor_name) {
                return Err(IntegrationError::VendorBlacklisted { contract_id });
            }
        }

        let verifying_key = self
            .trusted_authorities
            .get(&contract.signer_fingerprint)
            .ok_or_else(|| IntegrationError::VendorContractSignatureInvalid {
                contract_id: contract_id.clone(),
                reason: "unknown authority".to_string(),
            })?;

        let canonical = canonical_contract_bytes(&contract);
        let signature = Signature::from_slice(&contract.signature).map_err(|_| {
            IntegrationError::VendorContractSignatureInvalid {
                contract_id: contract_id.clone(),
                reason: "ed25519 verify failed".to_string(),
            }
        })?;

        verifying_key
            .verify_strict(&canonical, &signature)
            .map_err(|_| IntegrationError::VendorContractSignatureInvalid {
                contract_id: contract_id.clone(),
                reason: "ed25519 verify failed".to_string(),
            })?;

        {
            let mut contracts = self.contracts.write().map_err(|_| lock_poisoned())?;
            if contracts.contains_key(&contract_id) {
                return Err(IntegrationError::VendorContractSignatureInvalid {
                    contract_id,
                    reason: "contract already admitted".to_string(),
                });
            }
            contracts.insert(contract_id.clone(), contract);
        }

        let lifecycle = IntegrationLifecycleState::Proposed {
            proposer: contract_id.0.clone(),
            proposed_at: Utc::now(),
        };
        self.lifecycle_states
            .write()
            .map_err(|_| lock_poisoned())?
            .insert(contract_id, lifecycle);

        Ok(())
    }

    /// Transitions a contract's lifecycle state after validating the move
    /// against the allowed FSM transition table.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the contract is unknown.
    ///
    /// Returns `LifecycleInvalidTransition` if the requested transition is not
    /// in the allowed set or the `Evaluated` → `Piloted` path fails the
    /// `security_audit_passed` guard.
    #[allow(clippy::unused_async)]
    pub async fn transition_lifecycle(
        &self,
        contract_id: &VendorContractId,
        new_state: IntegrationLifecycleState,
    ) -> Result<(), IntegrationError> {
        let mut states = self.lifecycle_states.write().map_err(|_| lock_poisoned())?;

        let current = states
            .get(contract_id)
            .ok_or_else(|| IntegrationError::Internal("unknown contract".to_string()))?;

        let from = current.label();
        let to = new_state.label();

        if !is_transition_allowed(from, to, current) {
            return Err(IntegrationError::LifecycleInvalidTransition {
                from,
                to,
                reason: format!(
                    "transition from {from:?} to {to:?} is not allowed by the lifecycle FSM"
                ),
            });
        }

        states.insert(contract_id.clone(), new_state);
        drop(states);
        Ok(())
    }

    /// Returns the admitted contract for the given id, if any.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn get_contract(
        &self,
        contract_id: &VendorContractId,
    ) -> Option<VendorIntegrationContract> {
        let contracts = self.contracts.read().ok()?;
        contracts.get(contract_id).cloned()
    }

    /// Returns the current lifecycle state for the given contract id, if any.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn current_lifecycle(
        &self,
        contract_id: &VendorContractId,
    ) -> Option<IntegrationLifecycleState> {
        let states = self.lifecycle_states.read().ok()?;
        states.get(contract_id).cloned()
    }

    /// Lists all admitted contracts.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn list_contracts(&self) -> Vec<VendorIntegrationContract> {
        let contracts = self.contracts.read().ok();
        contracts
            .map(|c| c.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Lists admitted contracts filtered by vendor kind.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn list_by_kind(&self, kind: VendorKind) -> Vec<VendorIntegrationContract> {
        let contracts = self.contracts.read().ok();
        contracts
            .map(|c| {
                c.values()
                    .filter(|ct| ct.vendor_kind == kind)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns contract ids whose current lifecycle matches the given label.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn list_in_state(&self, label: IntegrationLifecycleLabel) -> Vec<VendorContractId> {
        let states = self.lifecycle_states.read().ok();
        states
            .map(|s| {
                s.iter()
                    .filter(|(_, st)| st.label() == label)
                    .map(|(id, _)| id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Adds a vendor name to the blacklist.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the blacklist lock is poisoned.
    #[allow(clippy::unused_async)]
    pub async fn add_to_blacklist(&self, vendor_name: &str) -> Result<(), IntegrationError> {
        self.blacklist
            .write()
            .map_err(|_| lock_poisoned())?
            .insert(vendor_name.to_string());
        Ok(())
    }

    /// Returns `true` if the vendor name is blacklisted.
    #[must_use]
    #[allow(clippy::unused_async)]
    pub async fn is_blacklisted(&self, vendor_name: &str) -> bool {
        self.blacklist
            .read()
            .ok()
            .is_some_and(|b| b.contains(vendor_name))
    }

    /// Forces a contract's lifecycle to `Retired`, bypassing the normal
    /// transition table.
    ///
    /// # Errors
    ///
    /// Returns `Internal` if the contract is unknown or a lock is poisoned.
    #[allow(clippy::unused_async)]
    pub async fn revoke_contract(
        &self,
        contract_id: &VendorContractId,
        reason: &str,
    ) -> Result<(), IntegrationError> {
        {
            let contracts = self.contracts.read().map_err(|_| lock_poisoned())?;
            if !contracts.contains_key(contract_id) {
                return Err(IntegrationError::Internal("unknown contract".to_string()));
            }
        }

        self.lifecycle_states
            .write()
            .map_err(|_| lock_poisoned())?
            .insert(
                contract_id.clone(),
                IntegrationLifecycleState::Retired {
                    since: Utc::now(),
                    reason: reason.to_string(),
                    data_migration_completed: false,
                },
            );
        Ok(())
    }
}

impl Default for VendorIntegrationRegistry {
    fn default() -> Self {
        Self::new()
    }
}
