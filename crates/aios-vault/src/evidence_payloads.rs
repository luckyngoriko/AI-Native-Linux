//! Typed vault evidence payloads for S5.2/S5.4 -> S3.1 emission.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S5.2/S5.4 evidence vocabulary"
)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use aios_action::ActionId;

use crate::capability::{CapabilityClass, CapabilityId};
use crate::identity::SubjectRef;
use crate::override_class::OverrideClass;

/// Payload for `VAULT_CAPABILITY_ISSUED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CapabilityIssuedPayload {
    /// Issued capability id.
    pub capability_id: CapabilityId,
    /// Issued capability class.
    pub class: CapabilityClass,
    /// Subject that received the capability.
    pub issued_to: SubjectRef,
    /// Broker issuance timestamp.
    pub issued_at: DateTime<Utc>,
    /// Optional hard expiry timestamp.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Payload for the redacted use-without-reveal operation event.
///
/// The current S3.1 vocabulary names this record `VAULT_OPERATION`, not
/// `CAPABILITY_USED`; this payload carries only the closed operation kind, never
/// input bytes, output bytes, ciphertext, signatures, or key material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CapabilityUsedPayload {
    /// Capability id used for the operation.
    pub capability_id: CapabilityId,
    /// String variant name only, e.g. `Encrypt` or `Sign`.
    pub operation_kind: String,
    /// Broker use timestamp.
    pub used_at: DateTime<Utc>,
    /// Evidence-emitting subject.
    pub subject: SubjectRef,
}

/// Payload for `VAULT_CAPABILITY_REVOKED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CapabilityRevokedPayload {
    /// Revoked capability id.
    pub capability_id: CapabilityId,
    /// Subject that revoked the capability.
    pub revoked_by: SubjectRef,
    /// Closed revocation reason token from S5.2.
    pub reason: String,
    /// Broker revocation timestamp.
    pub revoked_at: DateTime<Utc>,
}

/// Payload for an expiration transition.
///
/// S5.2 states normal expiration has no dedicated lifecycle record; T-053 emits
/// this as a redacted `VAULT_OPERATION` because S3.1 has no
/// `VAULT_CAPABILITY_EXPIRED` variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct CapabilityExpiredPayload {
    /// Expired capability id.
    pub capability_id: CapabilityId,
    /// Transition timestamp.
    pub expired_at: DateTime<Utc>,
}

/// Payload for `OVERRIDE_GRANTED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OverrideGrantedPayload {
    /// Override binding id.
    pub binding_id: String,
    /// Override class.
    pub class: OverrideClass,
    /// Subjects whose confirmation granted the binding.
    pub granted_by: Vec<SubjectRef>,
    /// Optional bound action id.
    pub target_action_id: Option<ActionId>,
    /// Grant timestamp.
    pub granted_at: DateTime<Utc>,
    /// Hard expiry timestamp.
    pub expires_at: DateTime<Utc>,
}

/// Payload for `OVERRIDE_CONSUMED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OverrideConsumedPayload {
    /// Override binding id.
    pub binding_id: String,
    /// Subject that consumed the binding.
    pub consumer: SubjectRef,
    /// Consumption timestamp.
    pub consumed_at: DateTime<Utc>,
}

/// Payload for `OVERRIDE_REVOKED`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct OverrideRevokedPayload {
    /// Override binding id.
    pub binding_id: String,
    /// Subject that revoked the binding.
    pub revoker: SubjectRef,
    /// Revocation timestamp.
    pub revoked_at: DateTime<Utc>,
}
