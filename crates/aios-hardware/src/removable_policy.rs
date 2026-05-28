#![allow(missing_docs, clippy::missing_errors_doc)]

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use tokio::sync::RwLock;

use crate::error::HardwareError;
use crate::ids::DeviceId;
use crate::removable::RemovableDevicePolicy;

// ---------------------------------------------------------------------------
// Inline AiSubjectClassifier (duplicate of aios-network shape — no dep)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct AiSubjectClassifier {
    prefixes: Vec<String>,
}

impl AiSubjectClassifier {
    #[must_use]
    pub fn new() -> Self {
        Self {
            prefixes: vec!["agent:".into(), "ai:".into()],
        }
    }

    #[must_use]
    pub fn is_ai(&self, subject: &str) -> bool {
        self.prefixes.iter().any(|p| subject.starts_with(p))
    }
}

impl Default for AiSubjectClassifier {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// RemovableDevicePolicyTable
// ---------------------------------------------------------------------------

pub struct RemovableDevicePolicyTable {
    policies: RwLock<HashMap<DeviceId, RemovableDevicePolicy>>,
    ai_subject_classifier: AiSubjectClassifier,
    recovery_mode_active: AtomicBool,
}

impl RemovableDevicePolicyTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            policies: RwLock::new(HashMap::new()),
            ai_subject_classifier: AiSubjectClassifier::new(),
            recovery_mode_active: AtomicBool::new(false),
        }
    }

    #[must_use]
    pub fn with_recovery_mode(active: bool) -> Self {
        Self {
            policies: RwLock::new(HashMap::new()),
            ai_subject_classifier: AiSubjectClassifier::new(),
            recovery_mode_active: AtomicBool::new(active),
        }
    }

    pub async fn set_policy(
        &self,
        device: DeviceId,
        policy: RemovableDevicePolicy,
        setter: &str,
    ) -> Result<(), HardwareError> {
        if self.ai_subject_classifier.is_ai(setter) {
            return Err(HardwareError::Internal(
                "AI cannot set removable policy".into(),
            ));
        }

        let effective = if self.recovery_mode_active.load(Ordering::Acquire) {
            RemovableDevicePolicy::RecoveryDenied
        } else {
            policy
        };

        self.policies.write().await.insert(device, effective);
        Ok(())
    }

    pub async fn get_policy(&self, device: &DeviceId) -> RemovableDevicePolicy {
        if self.recovery_mode_active.load(Ordering::Acquire) {
            return RemovableDevicePolicy::RecoveryDenied;
        }
        self.policies
            .read()
            .await
            .get(device)
            .copied()
            .unwrap_or(RemovableDevicePolicy::DenyDefault)
    }

    pub async fn check_mount(
        &self,
        device: &DeviceId,
        requester: &str,
    ) -> Result<(), HardwareError> {
        // INV-013: AI subjects never mount removable directly
        if self.ai_subject_classifier.is_ai(requester) {
            let policy = self.get_policy(device).await;
            return Err(HardwareError::RemovableDenied {
                device: device.clone(),
                policy,
            });
        }

        let policy = self.get_policy(device).await;
        match policy {
            RemovableDevicePolicy::DenyDefault | RemovableDevicePolicy::RecoveryDenied => {
                Err(HardwareError::RemovableDenied {
                    device: device.clone(),
                    policy,
                })
            }
            RemovableDevicePolicy::AllowReadOnly
            | RemovableDevicePolicy::AllowMount
            | RemovableDevicePolicy::AllowReadWrite => Ok(()),
        }
    }

    pub fn set_recovery_mode(&self, active: bool) {
        self.recovery_mode_active.store(active, Ordering::Release);
    }

    pub async fn list_policies(&self) -> Vec<(DeviceId, RemovableDevicePolicy)> {
        let guard = self.policies.read().await;
        guard.iter().map(|(id, pol)| (id.clone(), *pol)).collect()
    }
}

impl Default for RemovableDevicePolicyTable {
    fn default() -> Self {
        Self::new()
    }
}
