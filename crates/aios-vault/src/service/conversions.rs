//! Rust ↔ proto translations for the gRPC `VaultBroker` surface (T-052).
//!
//! The conversion layer is the only place that knows about prost-generated
//! message shapes. The core vault model remains tonic-free.

#![allow(
    missing_docs,
    clippy::clone_on_copy,
    clippy::missing_const_for_fn,
    clippy::missing_errors_doc,
    clippy::must_use_candidate,
    clippy::ref_option,
    clippy::result_large_err,
    reason = "conversion function names are intentionally literal and covered by the module docs"
)]

use chrono::{DateTime, TimeZone, Utc};
use prost_types::Timestamp;
use tonic::Status;

use aios_action::ActionId;

use crate::audit::CapabilityAuditEntry;
use crate::broker::{
    IssueCapabilityRequest, UseCapabilityResult, VaultOperation as RustVaultOperation,
};
use crate::capability::{
    CapabilityClass, CapabilityId, CapabilityState, KeyMaterialHandle, VaultCapability,
};
use crate::error::VaultError;
use crate::identity::{Session, SessionState, Subject, SubjectRef, SubjectType};
use crate::key_material::KeyAlgorithm;
use crate::lifecycle::ExpirationPassReport;
use crate::override_class::{OverrideBinding, OverrideBindingState, OverrideClass};
use crate::service::proto;

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

/// Convert a `chrono::DateTime<Utc>` into the prost well-known `Timestamp`.
#[must_use]
pub fn datetime_to_proto(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: i32::try_from(dt.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

/// Convert the prost well-known `Timestamp` back into `chrono::DateTime<Utc>`.
#[must_use]
pub fn datetime_from_proto(ts: Timestamp) -> DateTime<Utc> {
    Utc.timestamp_opt(ts.seconds, u32::try_from(ts.nanos).unwrap_or(0))
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default())
}

pub(crate) fn required_datetime_from_proto(
    ts: Option<Timestamp>,
    field: &'static str,
) -> Result<DateTime<Utc>, Status> {
    ts.map(datetime_from_proto)
        .ok_or_else(|| Status::invalid_argument(format!("{field} is required")))
}

fn optional_datetime_to_proto(dt: Option<DateTime<Utc>>) -> Option<Timestamp> {
    dt.map(datetime_to_proto)
}

// ---------------------------------------------------------------------------
// Error -> tonic::Status
// ---------------------------------------------------------------------------

/// Map typed [`VaultError`] values onto canonical gRPC status codes.
#[must_use]
pub fn vault_error_to_status(err: &VaultError) -> Status {
    match err {
        VaultError::CapabilityNotFound(_)
        | VaultError::SubjectNotFound(_)
        | VaultError::OverrideBindingNotFound(_)
        | VaultError::SessionExpired(_) => Status::not_found(err.to_string()),
        VaultError::CapabilityRevoked(_)
        | VaultError::CapabilityExpired(_)
        | VaultError::OverrideAlreadyConsumed
        | VaultError::OverrideExpired(_)
        | VaultError::InvalidTransition { .. } => Status::failed_precondition(err.to_string()),
        VaultError::OperationClassMismatch { .. }
        | VaultError::KeyAlgorithmMismatch { .. }
        | VaultError::OverrideClassApproverCountMismatch { .. }
        | VaultError::OverrideRequiresHumanApprovers { .. } => {
            Status::invalid_argument(err.to_string())
        }
        VaultError::AiCannotGrantOverride(_) | VaultError::KeyMaterialLeak => {
            Status::permission_denied(err.to_string())
        }
        VaultError::SubjectAlreadyRegistered(_)
        | VaultError::SessionAlreadyActive(_)
        | VaultError::GroupMembershipUnchanged => Status::already_exists(err.to_string()),
        VaultError::CryptoError(_)
        | VaultError::OperationUnsupportedInT047(_)
        | VaultError::OperationUnsupportedInT049(_)
        | VaultError::Internal(_) => Status::internal(err.to_string()),
    }
}

// ---------------------------------------------------------------------------
// ID helpers
// ---------------------------------------------------------------------------

pub(crate) fn parse_capability_id(input: &str) -> Result<CapabilityId, Status> {
    CapabilityId::parse(input)
        .map_err(|err| Status::invalid_argument(format!("invalid capability_id `{input}`: {err}")))
}

fn action_id_to_proto(action_id: &Option<ActionId>) -> Option<Vec<u8>> {
    action_id.as_ref().map(|id| id.as_str().as_bytes().to_vec())
}

fn action_id_from_proto(bytes: Option<Vec<u8>>) -> Result<Option<ActionId>, Status> {
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    if bytes.is_empty() {
        return Ok(None);
    }
    let raw = std::str::from_utf8(&bytes).map_err(|err| {
        Status::invalid_argument(format!("target_action_id_proto is not UTF-8: {err}"))
    })?;
    ActionId::parse(raw)
        .map(Some)
        .map_err(|err| Status::invalid_argument(format!("invalid action id `{raw}`: {err}")))
}

// ---------------------------------------------------------------------------
// Enum conversions
// ---------------------------------------------------------------------------

#[must_use]
pub const fn capability_class_to_proto(class: CapabilityClass) -> proto::VaultCapabilityClass {
    match class {
        CapabilityClass::KeySign => proto::VaultCapabilityClass::KeySign,
        CapabilityClass::KeyVerify => proto::VaultCapabilityClass::KeyVerify,
        CapabilityClass::KeyEncrypt => proto::VaultCapabilityClass::KeyEncrypt,
        CapabilityClass::KeyDecrypt => proto::VaultCapabilityClass::KeyDecrypt,
        CapabilityClass::MacGenerate => proto::VaultCapabilityClass::MacGenerate,
        CapabilityClass::MacVerify => proto::VaultCapabilityClass::MacVerify,
        CapabilityClass::RandomGenerate => proto::VaultCapabilityClass::RandomGenerate,
        CapabilityClass::SecretGet => proto::VaultCapabilityClass::SecretGet,
        CapabilityClass::BootstrapKeySign => proto::VaultCapabilityClass::BootstrapKeySign,
    }
}

pub fn capability_class_from_proto(
    class: proto::VaultCapabilityClass,
) -> Result<CapabilityClass, Status> {
    match class {
        proto::VaultCapabilityClass::KeySign => Ok(CapabilityClass::KeySign),
        proto::VaultCapabilityClass::KeyVerify => Ok(CapabilityClass::KeyVerify),
        proto::VaultCapabilityClass::KeyEncrypt => Ok(CapabilityClass::KeyEncrypt),
        proto::VaultCapabilityClass::KeyDecrypt => Ok(CapabilityClass::KeyDecrypt),
        proto::VaultCapabilityClass::MacGenerate => Ok(CapabilityClass::MacGenerate),
        proto::VaultCapabilityClass::MacVerify => Ok(CapabilityClass::MacVerify),
        proto::VaultCapabilityClass::RandomGenerate => Ok(CapabilityClass::RandomGenerate),
        proto::VaultCapabilityClass::SecretGet => Ok(CapabilityClass::SecretGet),
        proto::VaultCapabilityClass::BootstrapKeySign => Ok(CapabilityClass::BootstrapKeySign),
        proto::VaultCapabilityClass::Unspecified => {
            Err(Status::invalid_argument("capability class is unspecified"))
        }
    }
}

#[must_use]
pub const fn capability_state_to_proto(state: CapabilityState) -> proto::CapabilityState {
    match state {
        CapabilityState::Draft => proto::CapabilityState::Draft,
        CapabilityState::Active => proto::CapabilityState::Active,
        CapabilityState::Expired => proto::CapabilityState::Expired,
        CapabilityState::Revoked => proto::CapabilityState::Revoked,
        CapabilityState::Rotated => proto::CapabilityState::Rotated,
        CapabilityState::Discarded => proto::CapabilityState::Discarded,
    }
}

pub fn key_algorithm_from_proto(algorithm: proto::KeyAlgorithm) -> Result<KeyAlgorithm, Status> {
    match algorithm {
        proto::KeyAlgorithm::Aes256Gcm => Ok(KeyAlgorithm::Aes256Gcm),
        proto::KeyAlgorithm::HmacSha256 => Ok(KeyAlgorithm::HmacSha256),
        proto::KeyAlgorithm::HkdfSha256 => Ok(KeyAlgorithm::HkdfSha256),
        proto::KeyAlgorithm::Ed25519 => Ok(KeyAlgorithm::Ed25519),
        proto::KeyAlgorithm::X25519 => Ok(KeyAlgorithm::X25519),
        proto::KeyAlgorithm::Unspecified => {
            Err(Status::invalid_argument("key algorithm is unspecified"))
        }
    }
}

#[must_use]
pub const fn override_class_to_proto(class: OverrideClass) -> proto::OverrideClass {
    match class {
        OverrideClass::StrongSolo => proto::OverrideClass::StrongSolo,
        OverrideClass::DualHuman => proto::OverrideClass::DualHuman,
        OverrideClass::TripleHuman => proto::OverrideClass::TripleHuman,
    }
}

pub fn override_class_from_proto(class: proto::OverrideClass) -> Result<OverrideClass, Status> {
    match class {
        proto::OverrideClass::StrongSolo => Ok(OverrideClass::StrongSolo),
        proto::OverrideClass::DualHuman => Ok(OverrideClass::DualHuman),
        proto::OverrideClass::TripleHuman => Ok(OverrideClass::TripleHuman),
        proto::OverrideClass::Unspecified => {
            Err(Status::invalid_argument("override class is unspecified"))
        }
    }
}

#[must_use]
pub const fn override_state_to_proto(state: OverrideBindingState) -> proto::OverrideBindingState {
    match state {
        OverrideBindingState::Granted => proto::OverrideBindingState::Granted,
        OverrideBindingState::Consumed => proto::OverrideBindingState::Consumed,
        OverrideBindingState::Revoked => proto::OverrideBindingState::Revoked,
        OverrideBindingState::Expired => proto::OverrideBindingState::Expired,
    }
}

#[must_use]
pub const fn subject_type_to_proto(subject_type: SubjectType) -> proto::SubjectType {
    match subject_type {
        SubjectType::Human => proto::SubjectType::HumanUser,
        SubjectType::Agent => proto::SubjectType::AiAgent,
        SubjectType::Application => proto::SubjectType::Application,
        SubjectType::Service => proto::SubjectType::Service,
        SubjectType::Device => proto::SubjectType::Device,
        SubjectType::Workflow => proto::SubjectType::Workflow,
        SubjectType::RemoteOperator => proto::SubjectType::RemoteOperator,
        SubjectType::LocalOperator => proto::SubjectType::LocalOperator,
    }
}

pub fn subject_type_from_proto(subject_type: proto::SubjectType) -> Result<SubjectType, Status> {
    match subject_type {
        proto::SubjectType::HumanUser => Ok(SubjectType::Human),
        proto::SubjectType::AiAgent => Ok(SubjectType::Agent),
        proto::SubjectType::Application => Ok(SubjectType::Application),
        proto::SubjectType::Service => Ok(SubjectType::Service),
        proto::SubjectType::Device => Ok(SubjectType::Device),
        proto::SubjectType::Workflow => Ok(SubjectType::Workflow),
        proto::SubjectType::RemoteOperator => Ok(SubjectType::RemoteOperator),
        proto::SubjectType::LocalOperator => Ok(SubjectType::LocalOperator),
        proto::SubjectType::Unspecified => {
            Err(Status::invalid_argument("subject type is unspecified"))
        }
    }
}

#[must_use]
pub const fn session_state_to_proto(state: SessionState) -> proto::SessionState {
    match state {
        SessionState::Active => proto::SessionState::SessionActive,
        SessionState::Suspended => proto::SessionState::SessionSuspended,
        SessionState::Revoked => proto::SessionState::SessionRevoked,
        SessionState::Expired => proto::SessionState::SessionExpired,
    }
}

// ---------------------------------------------------------------------------
// Struct conversions
// ---------------------------------------------------------------------------

#[must_use]
pub fn vault_capability_to_proto(capability: &VaultCapability) -> proto::VaultCapabilityProto {
    proto::VaultCapabilityProto {
        capability_id: capability.capability_id.to_string(),
        class: i32::from(capability_class_to_proto(capability.class)),
        issued_to: capability.issued_to.0.clone(),
        issued_at: Some(datetime_to_proto(capability.issued_at)),
        expires_at: optional_datetime_to_proto(capability.expires_at),
        state: i32::from(capability_state_to_proto(capability.state)),
        key_material_handle: capability.key_material_handle.0.clone(),
    }
}

pub fn issue_capability_request_from_proto(
    request: proto::IssueCapabilityRequest,
) -> Result<IssueCapabilityRequest, Status> {
    let class = proto::VaultCapabilityClass::try_from(request.class)
        .map_err(|_| {
            Status::invalid_argument(format!("unknown capability class {}", request.class))
        })
        .and_then(capability_class_from_proto)?;
    let key_algorithm = proto::KeyAlgorithm::try_from(request.key_algorithm)
        .map_err(|_| {
            Status::invalid_argument(format!("unknown key algorithm {}", request.key_algorithm))
        })
        .and_then(key_algorithm_from_proto)?;

    Ok(IssueCapabilityRequest {
        class,
        issued_to: SubjectRef(request.issued_to),
        expires_at: request.expires_at.map(datetime_from_proto),
        key_algorithm,
        key_material_bytes: request.key_material_bytes,
    })
}

pub fn vault_operation_from_proto(
    operation: Option<proto::VaultOperation>,
) -> Result<RustVaultOperation, Status> {
    let operation = operation
        .and_then(|operation| operation.operation)
        .ok_or_else(|| Status::invalid_argument("operation is required"))?;

    match operation {
        proto::vault_operation::Operation::Enc(request) => Ok(RustVaultOperation::Encrypt {
            plaintext: request.plaintext,
            aad: request.aad,
        }),
        proto::vault_operation::Operation::Dec(request) => Ok(RustVaultOperation::Decrypt {
            ciphertext: request.ciphertext,
            aad: request.aad,
        }),
        proto::vault_operation::Operation::MacGen(request) => Ok(RustVaultOperation::MacGenerate {
            message: request.message,
        }),
        proto::vault_operation::Operation::MacVerify(request) => {
            Ok(RustVaultOperation::MacVerify {
                message: request.message,
                tag: request.tag,
            })
        }
        proto::vault_operation::Operation::KdfDerive(request) => {
            Ok(RustVaultOperation::KdfDerive {
                info: request.info,
                length: request.length,
            })
        }
        proto::vault_operation::Operation::Sign(request) => Ok(RustVaultOperation::Sign {
            message: request.message,
        }),
        proto::vault_operation::Operation::Verify(request) => Ok(RustVaultOperation::Verify {
            message: request.message,
            signature: request.signature,
        }),
        proto::vault_operation::Operation::RandomGenerate(request) => {
            Ok(RustVaultOperation::RandomGenerate {
                byte_count: request.byte_count,
            })
        }
        proto::vault_operation::Operation::SecretGet(request) => {
            Ok(RustVaultOperation::SecretGet {
                co_signer_approval_id: request.co_signer_approval_id,
            })
        }
    }
}

#[must_use]
pub fn use_capability_result_to_proto(result: &UseCapabilityResult) -> proto::UseCapabilityResult {
    let result = match result {
        UseCapabilityResult::Encrypted {
            ciphertext,
            nonce,
            aad,
        } => proto::use_capability_result::Result::Encrypted(proto::EncryptedResult {
            ciphertext: ciphertext.clone(),
            nonce: nonce.clone(),
            aad: aad.clone(),
        }),
        UseCapabilityResult::Decrypted { plaintext } => {
            proto::use_capability_result::Result::Decrypted(proto::DecryptedResult {
                plaintext: plaintext.clone(),
            })
        }
        UseCapabilityResult::MacGenerated { tag } => {
            proto::use_capability_result::Result::MacGenerated(proto::MacGeneratedResult {
                tag: tag.clone(),
            })
        }
        UseCapabilityResult::MacVerified { valid } => {
            proto::use_capability_result::Result::MacVerified(proto::MacVerifiedResult {
                valid: *valid,
            })
        }
        UseCapabilityResult::KdfDerived { derived_key_handle } => {
            proto::use_capability_result::Result::KdfDerived(proto::KdfDerivedResult {
                derived_key_handle: derived_key_handle.0.clone(),
            })
        }
        UseCapabilityResult::Signed { signature } => {
            proto::use_capability_result::Result::Signed(proto::SignedResult {
                signature: signature.clone(),
            })
        }
        UseCapabilityResult::Verified { valid } => {
            proto::use_capability_result::Result::Verified(proto::VerifiedResult { valid: *valid })
        }
        UseCapabilityResult::RandomGenerated { random_bytes } => {
            proto::use_capability_result::Result::RandomGenerated(proto::RandomGeneratedResult {
                random_bytes: random_bytes.clone(),
            })
        }
    };

    proto::UseCapabilityResult {
        result: Some(result),
    }
}

#[must_use]
pub fn override_binding_to_proto(binding: &OverrideBinding) -> proto::OverrideBindingProto {
    proto::OverrideBindingProto {
        binding_id: binding.binding_id.clone(),
        class: i32::from(override_class_to_proto(binding.class)),
        granted_by: binding
            .granted_by
            .iter()
            .map(|subject| subject.0.clone())
            .collect(),
        granted_at: Some(datetime_to_proto(binding.granted_at)),
        expires_at: Some(datetime_to_proto(binding.expires_at)),
        target_action_id_proto: action_id_to_proto(&binding.target_action_id),
        state: i32::from(override_state_to_proto(binding.state)),
    }
}

pub fn target_action_id_from_proto(bytes: Option<Vec<u8>>) -> Result<Option<ActionId>, Status> {
    action_id_from_proto(bytes)
}

#[must_use]
pub fn subject_to_proto(subject: &Subject) -> proto::SubjectProto {
    proto::SubjectProto {
        canonical_subject_id: subject.canonical_subject_id.clone(),
        subject_type: i32::from(subject_type_to_proto(subject.subject_type)),
        provisional_name: subject.provisional_name.clone(),
        groups: subject.groups.clone(),
        is_ai: subject.is_ai,
        created_at: Some(datetime_to_proto(subject.created_at)),
    }
}

pub fn subject_from_proto(subject: proto::SubjectProto) -> Result<Subject, Status> {
    let subject_type = proto::SubjectType::try_from(subject.subject_type)
        .map_err(|_| {
            Status::invalid_argument(format!("unknown subject type {}", subject.subject_type))
        })
        .and_then(subject_type_from_proto)?;

    Ok(Subject {
        canonical_subject_id: subject.canonical_subject_id,
        subject_type,
        provisional_name: subject.provisional_name,
        groups: subject.groups,
        is_ai: subject.is_ai,
        created_at: required_datetime_from_proto(subject.created_at, "subject.created_at")?,
    })
}

#[must_use]
pub fn session_to_proto(session: &Session) -> proto::SessionProto {
    proto::SessionProto {
        session_id: session.session_id.clone(),
        subject_id: session.subject_id.clone(),
        started_at: Some(datetime_to_proto(session.started_at)),
        expires_at: Some(datetime_to_proto(session.expires_at)),
        state: i32::from(session_state_to_proto(session.state)),
    }
}

#[must_use]
pub fn audit_entry_to_proto(entry: &CapabilityAuditEntry) -> proto::CapabilityAuditEntryProto {
    proto::CapabilityAuditEntryProto {
        capability_id: entry.capability_id.to_string(),
        use_count: entry.use_count,
        last_used_at: optional_datetime_to_proto(entry.last_used_at),
        last_used_op_kind: entry.last_used_op_kind.clone(),
        issued_by: entry.issued_by.0.clone(),
        revoked_by: entry.revoked_by.as_ref().map(|subject| subject.0.clone()),
        revoked_at: optional_datetime_to_proto(entry.revoked_at),
        expired_at: optional_datetime_to_proto(entry.expired_at),
    }
}

#[must_use]
pub fn expiration_report_to_proto(
    report: &ExpirationPassReport,
) -> proto::ExpirationPassReportProto {
    proto::ExpirationPassReportProto {
        pass_id: report.pass_id.clone(),
        started_at: Some(datetime_to_proto(report.started_at)),
        completed_at: Some(datetime_to_proto(report.completed_at)),
        capabilities_inspected: report.capabilities_inspected,
        capabilities_expired: report.capabilities_expired,
    }
}

#[must_use]
pub fn derived_handle_to_proto(handle: &KeyMaterialHandle) -> String {
    handle.0.clone()
}
