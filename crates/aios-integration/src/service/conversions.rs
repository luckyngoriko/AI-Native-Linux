//! Rust ↔ proto translations for gRPC `IntegrationService` (T-183).
//!
//! Owns bidirectional translation between domain types and tonic-generated proto
//! types, plus the `integration_error_to_status` mapper that translates each
//! `IntegrationError` variant into the appropriate `tonic::Status` gRPC code.

#![allow(
    clippy::result_large_err,
    missing_docs,
    clippy::match_wildcard_for_single_variants,
    clippy::use_self,
    clippy::cast_possible_truncation,
    clippy::clone_on_copy,
    clippy::missing_errors_doc,
    clippy::too_many_lines,
    clippy::wildcard_imports,
    clippy::cast_sign_loss
)]

use chrono::{DateTime, TimeZone, Utc};
use prost_types::Timestamp;
use tonic::Status;

use crate::bridges::{
    BridgeContract, BridgeKind, CapabilityExtractorRule, ManifestTranslationRules,
};
use crate::composition::{ComposedService, ServiceComposition, ServiceDependency};
use crate::control_map::{AiosInvariant, ComplianceBaseline, ControlFrameworkRef, ControlMapping};
use crate::cve::{CveId, CveSeverity, CveStatus};
use crate::cve_feed::{CveEnforcementLevel, CveRecord, PackageCveBinding};
use crate::error::IntegrationError;
use crate::ids::{ComposedSystemId, StandardSubscriptionId, VendorContractId};
use crate::lifecycle::{IntegrationLifecycleLabel, IntegrationLifecycleState};
use crate::orchestrator::{ServiceHealthSummary, ServiceScaffoldStatus};
use crate::service::proto;
use crate::standard::{StandardKind, StandardSubscription};
use crate::standard_registry::SubscriptionStatus;
use crate::vendor::{VendorIntegrationContract, VendorKind, VendorTrustClass};

// ── Timestamp helpers ──────────────────────────────────────────────────────

pub(crate) fn datetime_to_proto(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: i32::try_from(dt.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

fn datetime_from_proto(ts: Option<Timestamp>) -> DateTime<Utc> {
    ts.map_or_else(
        || Utc.timestamp_opt(0, 0).single().unwrap_or_default(),
        |t| {
            Utc.timestamp_opt(t.seconds, u32::try_from(t.nanos).unwrap_or(0))
                .single()
                .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default())
        },
    )
}

// ── VendorKind ↔ proto ─────────────────────────────────────────────────────

const fn vendor_kind_to_proto(k: VendorKind) -> i32 {
    match k {
        VendorKind::PackageRepository => proto::VendorKindProto::PackageRepository as i32,
        VendorKind::ApplicationStore => proto::VendorKindProto::ApplicationStore as i32,
        VendorKind::OciRegistry => proto::VendorKindProto::OciRegistry as i32,
        VendorKind::CveFeed => proto::VendorKindProto::CveFeed as i32,
        VendorKind::ComplianceProvider => proto::VendorKindProto::ComplianceProvider as i32,
        VendorKind::MetricsExporter => proto::VendorKindProto::MetricsExporter as i32,
        VendorKind::IdentityProvider => proto::VendorKindProto::IdentityProvider as i32,
        VendorKind::OtherCertified => proto::VendorKindProto::OtherCertified as i32,
    }
}

fn vendor_kind_from_proto(v: i32) -> Result<VendorKind, IntegrationError> {
    let p = proto::VendorKindProto::try_from(v)
        .map_err(|_| IntegrationError::Internal(format!("invalid VendorKindProto: {v}")))?;
    match p {
        proto::VendorKindProto::PackageRepository => Ok(VendorKind::PackageRepository),
        proto::VendorKindProto::ApplicationStore => Ok(VendorKind::ApplicationStore),
        proto::VendorKindProto::OciRegistry => Ok(VendorKind::OciRegistry),
        proto::VendorKindProto::CveFeed => Ok(VendorKind::CveFeed),
        proto::VendorKindProto::ComplianceProvider => Ok(VendorKind::ComplianceProvider),
        proto::VendorKindProto::MetricsExporter => Ok(VendorKind::MetricsExporter),
        proto::VendorKindProto::IdentityProvider => Ok(VendorKind::IdentityProvider),
        proto::VendorKindProto::OtherCertified => Ok(VendorKind::OtherCertified),
        proto::VendorKindProto::VendorKindUnspecified => {
            Err(IntegrationError::Internal("unspecified vendor kind".into()))
        }
    }
}

// ── VendorTrustClass ↔ proto ───────────────────────────────────────────────

const fn trust_class_to_proto(tc: VendorTrustClass) -> i32 {
    match tc {
        VendorTrustClass::AiosCertifiedPartner => {
            proto::VendorTrustClassProto::AiosCertifiedPartner as i32
        }
        VendorTrustClass::CommunityVerified => {
            proto::VendorTrustClassProto::CommunityVerified as i32
        }
        VendorTrustClass::OperatorAuthorised => {
            proto::VendorTrustClassProto::OperatorAuthorised as i32
        }
        VendorTrustClass::BlacklistedDoNotAdmit => {
            proto::VendorTrustClassProto::BlacklistedDoNotAdmit as i32
        }
    }
}

fn trust_class_from_proto(v: i32) -> Result<VendorTrustClass, IntegrationError> {
    let p = proto::VendorTrustClassProto::try_from(v)
        .map_err(|_| IntegrationError::Internal(format!("invalid VendorTrustClassProto: {v}")))?;
    match p {
        proto::VendorTrustClassProto::AiosCertifiedPartner => {
            Ok(VendorTrustClass::AiosCertifiedPartner)
        }
        proto::VendorTrustClassProto::CommunityVerified => Ok(VendorTrustClass::CommunityVerified),
        proto::VendorTrustClassProto::OperatorAuthorised => {
            Ok(VendorTrustClass::OperatorAuthorised)
        }
        proto::VendorTrustClassProto::BlacklistedDoNotAdmit => {
            Ok(VendorTrustClass::BlacklistedDoNotAdmit)
        }
        proto::VendorTrustClassProto::TrustClassUnspecified => {
            Err(IntegrationError::Internal("unspecified trust class".into()))
        }
    }
}

// ── Lifecycle label ↔ proto ────────────────────────────────────────────────

#[allow(dead_code)]
const fn lifecycle_label_to_proto(l: IntegrationLifecycleLabel) -> i32 {
    match l {
        IntegrationLifecycleLabel::Proposed => {
            proto::IntegrationLifecycleLabelProto::Proposed as i32
        }
        IntegrationLifecycleLabel::Evaluated => {
            proto::IntegrationLifecycleLabelProto::Evaluated as i32
        }
        IntegrationLifecycleLabel::Piloted => proto::IntegrationLifecycleLabelProto::Piloted as i32,
        IntegrationLifecycleLabel::Production => {
            proto::IntegrationLifecycleLabelProto::Production as i32
        }
        IntegrationLifecycleLabel::Deprecated => {
            proto::IntegrationLifecycleLabelProto::Deprecated as i32
        }
        IntegrationLifecycleLabel::Retired => proto::IntegrationLifecycleLabelProto::Retired as i32,
    }
}

fn lifecycle_label_from_proto(v: i32) -> Result<IntegrationLifecycleLabel, IntegrationError> {
    let p = proto::IntegrationLifecycleLabelProto::try_from(v).map_err(|_| {
        IntegrationError::Internal(format!("invalid IntegrationLifecycleLabelProto: {v}"))
    })?;
    match p {
        proto::IntegrationLifecycleLabelProto::Proposed => Ok(IntegrationLifecycleLabel::Proposed),
        proto::IntegrationLifecycleLabelProto::Evaluated => {
            Ok(IntegrationLifecycleLabel::Evaluated)
        }
        proto::IntegrationLifecycleLabelProto::Piloted => Ok(IntegrationLifecycleLabel::Piloted),
        proto::IntegrationLifecycleLabelProto::Production => {
            Ok(IntegrationLifecycleLabel::Production)
        }
        proto::IntegrationLifecycleLabelProto::Deprecated => {
            Ok(IntegrationLifecycleLabel::Deprecated)
        }
        proto::IntegrationLifecycleLabelProto::Retired => Ok(IntegrationLifecycleLabel::Retired),
        proto::IntegrationLifecycleLabelProto::LifecycleLabelUnspecified => Err(
            IntegrationError::Internal("unspecified lifecycle label".into()),
        ),
    }
}

// ── Lifecycle state ↔ proto (JSON payload approach) ────────────────────────

#[allow(dead_code)]
fn lifecycle_state_to_proto(
    state: &IntegrationLifecycleState,
) -> proto::IntegrationLifecycleStateProto {
    let label = lifecycle_label_to_proto(state.label());
    let payload_json = match state {
        IntegrationLifecycleState::Proposed {
            proposer,
            proposed_at,
        } => serde_json::json!({
            "proposer": proposer,
            "proposed_at": proposed_at.to_rfc3339(),
        })
        .to_string(),
        IntegrationLifecycleState::Evaluated {
            evaluator,
            evaluated_at,
            security_audit_passed,
        } => serde_json::json!({
            "evaluator": evaluator,
            "evaluated_at": evaluated_at.to_rfc3339(),
            "security_audit_passed": security_audit_passed,
        })
        .to_string(),
        IntegrationLifecycleState::Piloted { since, profile } => serde_json::json!({
            "since": since.to_rfc3339(),
            "profile": profile,
        })
        .to_string(),
        IntegrationLifecycleState::Production { since } => serde_json::json!({
            "since": since.to_rfc3339(),
        })
        .to_string(),
        IntegrationLifecycleState::Deprecated { since, sunset_due } => serde_json::json!({
            "since": since.to_rfc3339(),
            "sunset_due": sunset_due.map(|d| d.to_rfc3339()),
        })
        .to_string(),
        IntegrationLifecycleState::Retired {
            since,
            reason,
            data_migration_completed,
        } => serde_json::json!({
            "since": since.to_rfc3339(),
            "reason": reason,
            "data_migration_completed": data_migration_completed,
        })
        .to_string(),
    };
    proto::IntegrationLifecycleStateProto {
        label,
        payload_json,
    }
}

pub(crate) fn lifecycle_state_from_proto(
    p: &proto::IntegrationLifecycleStateProto,
) -> Result<IntegrationLifecycleState, IntegrationError> {
    let label = lifecycle_label_from_proto(p.label)?;
    let v: serde_json::Value = serde_json::from_str(&p.payload_json).unwrap_or_default();
    let now = Utc::now();
    match label {
        IntegrationLifecycleLabel::Proposed => {
            let proposer = v["proposer"].as_str().unwrap_or("unknown").to_string();
            let proposed_at = v["proposed_at"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map_or(now, |dt| dt.with_timezone(&Utc));
            Ok(IntegrationLifecycleState::Proposed {
                proposer,
                proposed_at,
            })
        }
        IntegrationLifecycleLabel::Evaluated => {
            let evaluator = v["evaluator"].as_str().unwrap_or("unknown").to_string();
            let evaluated_at = v["evaluated_at"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map_or(now, |dt| dt.with_timezone(&Utc));
            let security_audit_passed = v["security_audit_passed"].as_bool().unwrap_or(false);
            Ok(IntegrationLifecycleState::Evaluated {
                evaluator,
                evaluated_at,
                security_audit_passed,
            })
        }
        IntegrationLifecycleLabel::Piloted => {
            let since = v["since"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map_or(now, |dt| dt.with_timezone(&Utc));
            let profile = v["profile"].as_str().unwrap_or("DEV_RELAXED").to_string();
            Ok(IntegrationLifecycleState::Piloted { since, profile })
        }
        IntegrationLifecycleLabel::Production => {
            let since = v["since"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map_or(now, |dt| dt.with_timezone(&Utc));
            Ok(IntegrationLifecycleState::Production { since })
        }
        IntegrationLifecycleLabel::Deprecated => {
            let since = v["since"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map_or(now, |dt| dt.with_timezone(&Utc));
            let sunset_due = v["sunset_due"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc));
            Ok(IntegrationLifecycleState::Deprecated { since, sunset_due })
        }
        IntegrationLifecycleLabel::Retired => {
            let since = v["since"]
                .as_str()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map_or(now, |dt| dt.with_timezone(&Utc));
            let reason = v["reason"].as_str().unwrap_or("").to_string();
            let data_migration_completed = v["data_migration_completed"].as_bool().unwrap_or(false);
            Ok(IntegrationLifecycleState::Retired {
                since,
                reason,
                data_migration_completed,
            })
        }
    }
}

// ── VendorIntegrationContract ↔ proto ──────────────────────────────────────

pub(crate) fn vendor_contract_to_proto(
    c: &VendorIntegrationContract,
) -> proto::VendorIntegrationContractProto {
    proto::VendorIntegrationContractProto {
        contract_id: c.contract_id.0.clone(),
        vendor_name: c.vendor_name.clone(),
        vendor_kind: vendor_kind_to_proto(c.vendor_kind),
        trust_class: trust_class_to_proto(c.trust_class),
        contact_canonical_id: c.contact_canonical_id.clone(),
        rotation_cadence_days: c.rotation_cadence_days,
        breach_playbook_url: c.breach_playbook_url.clone(),
        signer_fingerprint: c.signer_fingerprint.clone(),
        signature: c.signature.clone(),
        admitted_at: Some(datetime_to_proto(c.admitted_at)),
    }
}

pub(crate) fn vendor_contract_from_proto(
    p: &proto::VendorIntegrationContractProto,
) -> Result<VendorIntegrationContract, IntegrationError> {
    Ok(VendorIntegrationContract {
        contract_id: VendorContractId(p.contract_id.clone()),
        vendor_name: p.vendor_name.clone(),
        vendor_kind: vendor_kind_from_proto(p.vendor_kind)?,
        trust_class: trust_class_from_proto(p.trust_class)?,
        contact_canonical_id: p.contact_canonical_id.clone(),
        rotation_cadence_days: p.rotation_cadence_days,
        breach_playbook_url: p.breach_playbook_url.clone(),
        signer_fingerprint: p.signer_fingerprint.clone(),
        signature: p.signature.clone(),
        admitted_at: datetime_from_proto(p.admitted_at),
    })
}

// ── StandardKind ↔ proto ───────────────────────────────────────────────────

const fn standard_kind_to_proto(k: StandardKind) -> i32 {
    match k {
        StandardKind::Nist80053Rev5 => proto::StandardKindProto::Nist80053Rev5 as i32,
        StandardKind::NistSp800218Ssdf => proto::StandardKindProto::NistSp800218Ssdf as i32,
        StandardKind::NistSp800207ZeroTrust => {
            proto::StandardKindProto::NistSp800207ZeroTrust as i32
        }
        StandardKind::NistSp800193Firmware => proto::StandardKindProto::NistSp800193Firmware as i32,
        StandardKind::DisaStig => proto::StandardKindProto::DisaStig as i32,
        StandardKind::CisControlsV8 => proto::StandardKindProto::CisControlsV8 as i32,
        StandardKind::Fips1403 => proto::StandardKindProto::Fips1403 as i32,
        StandardKind::Gdpr => proto::StandardKindProto::Gdpr as i32,
        StandardKind::Hipaa => proto::StandardKindProto::Hipaa as i32,
        StandardKind::Iso27001 => proto::StandardKindProto::Iso27001 as i32,
        StandardKind::Soc2 => proto::StandardKindProto::Soc2 as i32,
    }
}

fn standard_kind_from_proto(v: i32) -> Result<StandardKind, IntegrationError> {
    let p = proto::StandardKindProto::try_from(v)
        .map_err(|_| IntegrationError::Internal(format!("invalid StandardKindProto: {v}")))?;
    match p {
        proto::StandardKindProto::Nist80053Rev5 => Ok(StandardKind::Nist80053Rev5),
        proto::StandardKindProto::NistSp800218Ssdf => Ok(StandardKind::NistSp800218Ssdf),
        proto::StandardKindProto::NistSp800207ZeroTrust => Ok(StandardKind::NistSp800207ZeroTrust),
        proto::StandardKindProto::NistSp800193Firmware => Ok(StandardKind::NistSp800193Firmware),
        proto::StandardKindProto::DisaStig => Ok(StandardKind::DisaStig),
        proto::StandardKindProto::CisControlsV8 => Ok(StandardKind::CisControlsV8),
        proto::StandardKindProto::Fips1403 => Ok(StandardKind::Fips1403),
        proto::StandardKindProto::Gdpr => Ok(StandardKind::Gdpr),
        proto::StandardKindProto::Hipaa => Ok(StandardKind::Hipaa),
        proto::StandardKindProto::Iso27001 => Ok(StandardKind::Iso27001),
        proto::StandardKindProto::Soc2 => Ok(StandardKind::Soc2),
        proto::StandardKindProto::StandardKindUnspecified => Err(IntegrationError::Internal(
            "unspecified standard kind".into(),
        )),
    }
}

// ── StandardSubscription ↔ proto ───────────────────────────────────────────

pub(crate) fn subscription_to_proto(s: &StandardSubscription) -> proto::StandardSubscriptionProto {
    proto::StandardSubscriptionProto {
        subscription_id: s.subscription_id.0.clone(),
        standard: standard_kind_to_proto(s.standard),
        catalog_url: s.catalog_url.clone(),
        current_revision: s.current_revision.clone(),
        last_reviewed_at: Some(datetime_to_proto(s.last_reviewed_at)),
        next_review_due_at: Some(datetime_to_proto(s.next_review_due_at)),
        responsible_canonical_id: s.responsible_canonical_id.clone(),
    }
}

pub(crate) fn subscription_from_proto(
    p: &proto::StandardSubscriptionProto,
) -> Result<StandardSubscription, IntegrationError> {
    Ok(StandardSubscription {
        subscription_id: StandardSubscriptionId(p.subscription_id.clone()),
        standard: standard_kind_from_proto(p.standard)?,
        catalog_url: p.catalog_url.clone(),
        current_revision: p.current_revision.clone(),
        last_reviewed_at: datetime_from_proto(p.last_reviewed_at),
        next_review_due_at: datetime_from_proto(p.next_review_due_at),
        responsible_canonical_id: p.responsible_canonical_id.clone(),
    })
}

// ── SubscriptionStatus → proto ─────────────────────────────────────────────

pub(crate) fn subscription_status_to_proto(
    sub_id: &str,
    s: &SubscriptionStatus,
) -> proto::GetSubscriptionStatusResponse {
    let (status, until, since, expired_at) = match s {
        SubscriptionStatus::Current { until: u } => (
            proto::SubscriptionStatusProto::Current as i32,
            Some(datetime_to_proto(*u)),
            None,
            None,
        ),
        SubscriptionStatus::ReviewDue { since: s } => (
            proto::SubscriptionStatusProto::ReviewDue as i32,
            None,
            Some(datetime_to_proto(*s)),
            None,
        ),
        SubscriptionStatus::Expired { expired_at: e } => (
            proto::SubscriptionStatusProto::Expired as i32,
            None,
            None,
            Some(datetime_to_proto(*e)),
        ),
    };
    proto::GetSubscriptionStatusResponse {
        subscription_id: sub_id.to_string(),
        status,
        until,
        since,
        expired_at,
    }
}

// ── CveSeverity ↔ proto ────────────────────────────────────────────────────

const fn cve_severity_to_proto(s: CveSeverity) -> i32 {
    match s {
        CveSeverity::Low => proto::CveSeverityProto::Low as i32,
        CveSeverity::Medium => proto::CveSeverityProto::Medium as i32,
        CveSeverity::High => proto::CveSeverityProto::High as i32,
        CveSeverity::Critical => proto::CveSeverityProto::Critical as i32,
    }
}

fn cve_severity_from_proto(v: i32) -> Result<CveSeverity, IntegrationError> {
    let p = proto::CveSeverityProto::try_from(v)
        .map_err(|_| IntegrationError::Internal(format!("invalid CveSeverityProto: {v}")))?;
    match p {
        proto::CveSeverityProto::Low => Ok(CveSeverity::Low),
        proto::CveSeverityProto::Medium => Ok(CveSeverity::Medium),
        proto::CveSeverityProto::High => Ok(CveSeverity::High),
        proto::CveSeverityProto::Critical => Ok(CveSeverity::Critical),
        proto::CveSeverityProto::None | proto::CveSeverityProto::CveSeverityUnspecified => Err(
            IntegrationError::Internal("unspecified CVE severity".into()),
        ),
    }
}

// ── CveStatus ↔ proto ──────────────────────────────────────────────────────

const fn cve_status_to_proto(s: CveStatus) -> i32 {
    match s {
        CveStatus::Open | CveStatus::UnderReview => proto::CveStatusProto::Unresolved as i32,
        CveStatus::Patched => proto::CveStatusProto::Resolved as i32,
        CveStatus::Quarantined => proto::CveStatusProto::Mitigated as i32,
        CveStatus::NotApplicable => proto::CveStatusProto::FalsePositive as i32,
    }
}

fn cve_status_from_proto(v: i32) -> Result<CveStatus, IntegrationError> {
    let p = proto::CveStatusProto::try_from(v)
        .map_err(|_| IntegrationError::Internal(format!("invalid CveStatusProto: {v}")))?;
    match p {
        proto::CveStatusProto::Unresolved => Ok(CveStatus::Open),
        proto::CveStatusProto::Resolved => Ok(CveStatus::Patched),
        proto::CveStatusProto::Mitigated => Ok(CveStatus::Quarantined),
        proto::CveStatusProto::FalsePositive => Ok(CveStatus::NotApplicable),
        proto::CveStatusProto::CveStatusUnspecified => {
            Err(IntegrationError::Internal("unspecified CVE status".into()))
        }
    }
}

// ── CveEnforcementLevel ↔ proto ────────────────────────────────────────────

pub(crate) const fn enforcement_level_to_proto(l: CveEnforcementLevel) -> i32 {
    match l {
        CveEnforcementLevel::MonitorOnly => proto::CveEnforcementLevelProto::MonitorOnly as i32,
        CveEnforcementLevel::OperatorNotify => {
            proto::CveEnforcementLevelProto::OperatorNotify as i32
        }
        CveEnforcementLevel::QuarantineCandidate => {
            proto::CveEnforcementLevelProto::QuarantineCandidate as i32
        }
        CveEnforcementLevel::AutoQuarantine => {
            proto::CveEnforcementLevelProto::AutoQuarantine as i32
        }
    }
}

// ── CveRecord ↔ proto ──────────────────────────────────────────────────────

pub(crate) fn cve_record_to_proto(r: &CveRecord) -> proto::CveRecordProto {
    proto::CveRecordProto {
        cve_id: r.cve_id.0.clone(),
        published_at: Some(datetime_to_proto(r.published_at)),
        last_modified_at: Some(datetime_to_proto(r.last_modified_at)),
        cvss_v3_score: r.cvss_v3_score,
        severity: cve_severity_to_proto(r.severity),
        summary: r.summary.clone(),
        affected_cpe_uris: r.affected_cpe_uris.clone(),
    }
}

pub(crate) fn cve_record_from_proto(
    p: &proto::CveRecordProto,
) -> Result<CveRecord, IntegrationError> {
    let score = p.cvss_v3_score;
    if !(0.0..=10.0).contains(&score) {
        return Err(IntegrationError::ConfigInvalid(
            "CVSS score out of range".into(),
        ));
    }
    Ok(CveRecord {
        cve_id: CveId(p.cve_id.clone()),
        published_at: datetime_from_proto(p.published_at),
        last_modified_at: datetime_from_proto(p.last_modified_at),
        cvss_v3_score: score,
        severity: cve_severity_from_proto(p.severity)?,
        summary: p.summary.clone(),
        affected_cpe_uris: p.affected_cpe_uris.clone(),
    })
}

// ── PackageCveBinding ↔ proto ──────────────────────────────────────────────

pub(crate) fn binding_to_proto(b: &PackageCveBinding) -> proto::PackageCveBindingProto {
    proto::PackageCveBindingProto {
        binding_id: b.binding_id.clone(),
        cve_id: b.cve_id.0.clone(),
        package_id: b.package_id.clone(),
        status: cve_status_to_proto(b.status),
        bound_at: Some(datetime_to_proto(b.bound_at)),
        matched_via_cpe: b.matched_via_cpe.clone().unwrap_or_default(),
        mitigated_by: b.mitigated_by.clone().unwrap_or_default(),
    }
}

pub(crate) fn binding_from_proto(
    p: &proto::PackageCveBindingProto,
) -> Result<PackageCveBinding, IntegrationError> {
    Ok(PackageCveBinding {
        binding_id: p.binding_id.clone(),
        cve_id: CveId(p.cve_id.clone()),
        package_id: p.package_id.clone(),
        status: cve_status_from_proto(p.status)?,
        bound_at: datetime_from_proto(p.bound_at),
        matched_via_cpe: if p.matched_via_cpe.is_empty() {
            None
        } else {
            Some(p.matched_via_cpe.clone())
        },
        mitigated_by: if p.mitigated_by.is_empty() {
            None
        } else {
            Some(p.mitigated_by.clone())
        },
    })
}

// ── BridgeKind ↔ proto ─────────────────────────────────────────────────────

const fn bridge_kind_to_proto(k: &BridgeKind) -> i32 {
    match k {
        BridgeKind::Flathub => proto::BridgeKindProto::Flathub as i32,
        BridgeKind::OciRegistry { .. } => proto::BridgeKindProto::OciRegistryBridge as i32,
        BridgeKind::Apt { .. } => proto::BridgeKindProto::Apt as i32,
        BridgeKind::Dnf { .. } => proto::BridgeKindProto::Dnf as i32,
        BridgeKind::Pacman { .. } => proto::BridgeKindProto::Pacman as i32,
    }
}

fn bridge_kind_from_proto(v: i32) -> Result<BridgeKind, IntegrationError> {
    let p = proto::BridgeKindProto::try_from(v)
        .map_err(|_| IntegrationError::Internal(format!("invalid BridgeKindProto: {v}")))?;
    match p {
        proto::BridgeKindProto::Flathub => Ok(BridgeKind::Flathub),
        proto::BridgeKindProto::OciRegistryBridge => Ok(BridgeKind::OciRegistry {
            registry_host: String::new(),
        }),
        proto::BridgeKindProto::Apt => Ok(BridgeKind::Apt {
            distro: String::new(),
        }),
        proto::BridgeKindProto::Dnf => Ok(BridgeKind::Dnf {
            distro: String::new(),
        }),
        proto::BridgeKindProto::Pacman => Ok(BridgeKind::Pacman {
            distro: String::new(),
        }),
        proto::BridgeKindProto::BridgeKindUnspecified => {
            Err(IntegrationError::Internal("unspecified bridge kind".into()))
        }
    }
}

// ── CapabilityExtractorRule ↔ proto ────────────────────────────────────────

const fn extractor_to_proto(e: &CapabilityExtractorRule) -> i32 {
    match e {
        CapabilityExtractorRule::FlatpakFinishesSection => {
            proto::CapabilityExtractorRuleProto::FlatpakFinishesSection as i32
        }
        CapabilityExtractorRule::OciAnnotations => {
            proto::CapabilityExtractorRuleProto::OciAnnotations as i32
        }
        CapabilityExtractorRule::DebianControl => {
            proto::CapabilityExtractorRuleProto::DebianControl as i32
        }
        CapabilityExtractorRule::RpmSpec => proto::CapabilityExtractorRuleProto::RpmSpec as i32,
        CapabilityExtractorRule::PkgbuildArray => {
            proto::CapabilityExtractorRuleProto::PkgbuildArray as i32
        }
        CapabilityExtractorRule::OperatorAuthored => {
            proto::CapabilityExtractorRuleProto::OperatorAuthored as i32
        }
    }
}

fn extractor_from_proto(v: i32) -> Result<CapabilityExtractorRule, IntegrationError> {
    let p = proto::CapabilityExtractorRuleProto::try_from(v).map_err(|_| {
        IntegrationError::Internal(format!("invalid CapabilityExtractorRuleProto: {v}"))
    })?;
    match p {
        proto::CapabilityExtractorRuleProto::FlatpakFinishesSection => {
            Ok(CapabilityExtractorRule::FlatpakFinishesSection)
        }
        proto::CapabilityExtractorRuleProto::OciAnnotations => {
            Ok(CapabilityExtractorRule::OciAnnotations)
        }
        proto::CapabilityExtractorRuleProto::DebianControl => {
            Ok(CapabilityExtractorRule::DebianControl)
        }
        proto::CapabilityExtractorRuleProto::RpmSpec => Ok(CapabilityExtractorRule::RpmSpec),
        proto::CapabilityExtractorRuleProto::PkgbuildArray => {
            Ok(CapabilityExtractorRule::PkgbuildArray)
        }
        proto::CapabilityExtractorRuleProto::OperatorAuthored => {
            Ok(CapabilityExtractorRule::OperatorAuthored)
        }
        proto::CapabilityExtractorRuleProto::ExtractorUnspecified => Err(
            IntegrationError::Internal("unspecified extractor rule".into()),
        ),
    }
}

// ── ManifestTranslationRules ↔ proto ───────────────────────────────────────

fn translation_rules_to_proto(
    r: &ManifestTranslationRules,
) -> proto::ManifestTranslationRulesProto {
    proto::ManifestTranslationRulesProto {
        source_manifest_format: r.source_manifest_format.clone(),
        capability_extractor: extractor_to_proto(&r.capability_extractor),
        trust_floor: trust_class_to_proto(r.trust_floor),
    }
}

fn translation_rules_from_proto(
    p: &proto::ManifestTranslationRulesProto,
) -> Result<ManifestTranslationRules, IntegrationError> {
    Ok(ManifestTranslationRules {
        source_manifest_format: p.source_manifest_format.clone(),
        capability_extractor: extractor_from_proto(p.capability_extractor)?,
        trust_floor: trust_class_from_proto(p.trust_floor)?,
    })
}

// ── BridgeContract ↔ proto ─────────────────────────────────────────────────

pub(crate) fn bridge_contract_to_proto(b: &BridgeContract) -> proto::BridgeContractProto {
    proto::BridgeContractProto {
        bridge_id: b.bridge_id.clone(),
        kind: bridge_kind_to_proto(&b.kind),
        vendor_contract: Some(vendor_contract_to_proto(&b.vendor_contract)),
        translation_rules: Some(translation_rules_to_proto(&b.translation_rules)),
        admitted_at: Some(datetime_to_proto(b.admitted_at)),
    }
}

pub(crate) fn bridge_contract_from_proto(
    p: &proto::BridgeContractProto,
) -> Result<BridgeContract, IntegrationError> {
    let vendor_contract = p
        .vendor_contract
        .as_ref()
        .map(vendor_contract_from_proto)
        .ok_or_else(|| IntegrationError::Internal("vendor_contract required".into()))??;
    let translation_rules = p
        .translation_rules
        .as_ref()
        .map(translation_rules_from_proto)
        .ok_or_else(|| IntegrationError::Internal("translation_rules required".into()))??;
    Ok(BridgeContract {
        bridge_id: p.bridge_id.clone(),
        kind: bridge_kind_from_proto(p.kind)?,
        vendor_contract,
        translation_rules,
        admitted_at: datetime_from_proto(p.admitted_at),
    })
}

// ── ComposedService ↔ proto ─────────────────────────────────────────────────

#[allow(dead_code)]
fn composed_service_to_proto(s: &ComposedService) -> proto::ComposedServiceProto {
    proto::ComposedServiceProto {
        service_id: s.service_id.clone(),
        crate_name: s.crate_name.clone(),
        binding_endpoint: s.binding_endpoint.clone(),
        depends_on: s.depends_on.clone(),
    }
}

fn composed_service_from_proto(p: &proto::ComposedServiceProto) -> ComposedService {
    ComposedService {
        service_id: p.service_id.clone(),
        crate_name: p.crate_name.clone(),
        binding_endpoint: p.binding_endpoint.clone(),
        depends_on: p.depends_on.clone(),
    }
}

// ── ServiceDependency ↔ proto ──────────────────────────────────────────────

#[allow(dead_code)]
fn dependency_to_proto(d: &ServiceDependency) -> proto::ServiceDependencyProto {
    proto::ServiceDependencyProto {
        from_service: d.from_service.clone(),
        to_service: d.to_service.clone(),
        required: d.required,
    }
}

fn dependency_from_proto(p: &proto::ServiceDependencyProto) -> ServiceDependency {
    ServiceDependency {
        from_service: p.from_service.clone(),
        to_service: p.to_service.clone(),
        required: p.required,
    }
}

// ── ServiceComposition ↔ proto ─────────────────────────────────────────────

#[allow(dead_code)]
fn composition_to_proto(c: &ServiceComposition) -> proto::ServiceCompositionProto {
    proto::ServiceCompositionProto {
        composition_id: c.composition_id.0.clone(),
        services: c.services.iter().map(composed_service_to_proto).collect(),
        dependencies: c.dependencies.iter().map(dependency_to_proto).collect(),
    }
}

pub(crate) fn composition_from_proto(p: &proto::ServiceCompositionProto) -> ServiceComposition {
    ServiceComposition {
        composition_id: ComposedSystemId(p.composition_id.clone()),
        services: p.services.iter().map(composed_service_from_proto).collect(),
        dependencies: p.dependencies.iter().map(dependency_from_proto).collect(),
        topological_order: vec![],
    }
}

// ── ServiceScaffoldStatus ↔ proto ──────────────────────────────────────────

const fn scaffold_status_to_proto(s: ServiceScaffoldStatus) -> i32 {
    match s {
        ServiceScaffoldStatus::ScaffoldReady => {
            proto::ServiceScaffoldStatusProto::ScaffoldReady as i32
        }
        ServiceScaffoldStatus::NotInComposition => {
            proto::ServiceScaffoldStatusProto::NotInComposition as i32
        }
        ServiceScaffoldStatus::ConfigMissing => {
            proto::ServiceScaffoldStatusProto::ConfigMissing as i32
        }
    }
}

// ── ServiceHealthSummary → proto ───────────────────────────────────────────

pub(crate) fn health_summary_to_proto(
    s: &ServiceHealthSummary,
) -> proto::ServiceHealthSummaryProto {
    proto::ServiceHealthSummaryProto {
        service_id: s.service_id.clone(),
        crate_name: s.crate_name.clone(),
        status: scaffold_status_to_proto(s.status),
        topological_index: s.topological_index as u32,
    }
}

// ── AiosInvariant ↔ proto ──────────────────────────────────────────────────

fn invariant_to_proto(i: &AiosInvariant) -> proto::AiosInvariantProto {
    proto::AiosInvariantProto {
        invariant_id: i.invariant_id.clone(),
        name: i.name.clone(),
        layer: i.layer.clone(),
    }
}

fn invariant_from_proto(p: &proto::AiosInvariantProto) -> AiosInvariant {
    AiosInvariant {
        invariant_id: p.invariant_id.clone(),
        name: p.name.clone(),
        layer: p.layer.clone(),
    }
}

// ── ControlFrameworkRef ↔ proto ────────────────────────────────────────────

fn control_ref_to_proto(r: &ControlFrameworkRef) -> proto::ControlFrameworkRefProto {
    proto::ControlFrameworkRefProto {
        framework: standard_kind_to_proto(r.framework),
        control_family: r.control_family.clone(),
        control_id: r.control_id.clone(),
    }
}

fn control_ref_from_proto(
    p: &proto::ControlFrameworkRefProto,
) -> Result<ControlFrameworkRef, IntegrationError> {
    Ok(ControlFrameworkRef {
        framework: standard_kind_from_proto(p.framework)?,
        control_family: p.control_family.clone(),
        control_id: p.control_id.clone(),
    })
}

// ── ControlMapping ↔ proto ─────────────────────────────────────────────────

pub(crate) fn control_mapping_to_proto(m: &ControlMapping) -> proto::ControlMappingProto {
    proto::ControlMappingProto {
        mapping_id: m.mapping_id.clone(),
        invariant: Some(invariant_to_proto(&m.invariant)),
        control_refs: m.control_refs.iter().map(control_ref_to_proto).collect(),
        mapping_rationale: m.mapping_rationale.clone(),
        mapped_at: Some(datetime_to_proto(m.mapped_at)),
    }
}

pub(crate) fn control_mapping_from_proto(
    p: &proto::ControlMappingProto,
) -> Result<ControlMapping, IntegrationError> {
    let invariant = p.invariant.as_ref().map_or_else(
        || AiosInvariant {
            invariant_id: String::new(),
            name: String::new(),
            layer: String::new(),
        },
        invariant_from_proto,
    );
    let control_refs: Result<Vec<ControlFrameworkRef>, IntegrationError> =
        p.control_refs.iter().map(control_ref_from_proto).collect();
    Ok(ControlMapping {
        mapping_id: p.mapping_id.clone(),
        invariant,
        control_refs: control_refs?,
        mapping_rationale: p.mapping_rationale.clone(),
        mapped_at: datetime_from_proto(p.mapped_at),
    })
}

// ── ComplianceBaseline → proto ─────────────────────────────────────────────

pub(crate) fn baseline_to_proto(b: &ComplianceBaseline) -> proto::ComplianceBaselineProto {
    proto::ComplianceBaselineProto {
        baseline_id: b.baseline_id.clone(),
        aios_version: b.aios_version.clone(),
        mappings: b.mappings.iter().map(control_mapping_to_proto).collect(),
        snapshot_at: Some(datetime_to_proto(b.snapshot_at)),
        validator_canonical_id: b.validator_canonical_id.clone(),
    }
}

// ── integration_error_to_status ────────────────────────────────────────────

/// Map an [`IntegrationError`] to a [`tonic::Status`] for gRPC responses.
#[must_use]
pub fn integration_error_to_status(err: &IntegrationError) -> Status {
    match err {
        IntegrationError::LifecycleInvalidTransition { from, to, reason } => {
            Status::failed_precondition(format!(
                "lifecycle invalid transition from {from:?} to {to:?}: {reason}"
            ))
        }
        IntegrationError::VendorContractSignatureInvalid {
            contract_id,
            reason,
        } => Status::permission_denied(format!(
            "vendor contract signature invalid for {contract_id:?}: {reason}"
        )),
        IntegrationError::VendorBlacklisted { contract_id } => {
            Status::permission_denied(format!("vendor {contract_id:?} is blacklisted"))
        }
        IntegrationError::StandardSubscriptionExpired {
            subscription_id,
            expired_at,
        } => Status::failed_precondition(format!(
            "standard subscription {subscription_id:?} expired at {expired_at}"
        )),
        IntegrationError::CveFeedUnreachable(msg) => {
            Status::unavailable(format!("CVE feed unreachable: {msg}"))
        }
        IntegrationError::CompositionCycleDetected { cycle } => {
            Status::failed_precondition(format!("composition cycle detected: {cycle:?}"))
        }
        IntegrationError::ComposedServiceMissing {
            service_id,
            required_by,
        } => Status::not_found(format!(
            "composed service {service_id} missing (required by {required_by})"
        )),
        IntegrationError::OrchestratorBootFailed { stage, reason } => Status::internal(format!(
            "orchestrator boot failed at stage {stage}: {reason}"
        )),
        IntegrationError::ConfigInvalid(msg) => {
            Status::invalid_argument(format!("config invalid: {msg}"))
        }
        IntegrationError::Internal(msg) => Status::internal(format!("internal error: {msg}")),
    }
}
