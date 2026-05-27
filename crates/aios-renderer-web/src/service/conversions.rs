//! Rust ↔ proto translations for the gRPC `WebRendererService` surface (T-147).
//!
//! Owns bidirectional translation between domain types and tonic-generated proto
//! types, plus the `web_error_to_status` mapper.

#![allow(
    clippy::result_large_err,
    missing_docs,
    clippy::match_wildcard_for_single_variants,
    clippy::use_self,
    clippy::cast_possible_truncation,
    clippy::clone_on_copy,
    clippy::missing_errors_doc
)]

use chrono::{DateTime, TimeZone, Utc};
use prost_types::Timestamp;

use crate::error::WebRendererError;
use crate::exposure::{ExposureLevel, ExposureLevelLabel};
use crate::origin::{OriginScheme, ParsedOrigin};
use crate::renderer::{
    AllocateWebSurfaceRequest, RecoveryEntryReceipt, TokenApplicationReceipt, WebSurfaceFilter,
    WebSurfaceReleaseReceipt,
};
use crate::service::proto;
use crate::types::{RouteDescriptor, WebRendererMode, WebSurfaceDescriptor};
use aios_renderer_kde::{NodeKind, VisualToken, VisualTokenKind};

// ── Timestamp helpers ────────────────────────────────────────────────────

pub(crate) fn datetime_to_proto(dt: DateTime<Utc>) -> Timestamp {
    Timestamp {
        seconds: dt.timestamp(),
        nanos: i32::try_from(dt.timestamp_subsec_nanos()).unwrap_or(0),
    }
}

#[allow(dead_code)]
fn datetime_from_proto(ts: Timestamp) -> DateTime<Utc> {
    Utc.timestamp_opt(ts.seconds, u32::try_from(ts.nanos).unwrap_or(0))
        .single()
        .unwrap_or_else(|| Utc.timestamp_opt(0, 0).single().unwrap_or_default())
}

// ── WebRendererMode ↔ proto ─────────────────────────────────────────────

pub(crate) fn renderer_mode_to_proto(mode: &WebRendererMode) -> proto::WebRendererModeProto {
    match mode {
        WebRendererMode::Normal => proto::WebRendererModeProto {
            kind: proto::WebRendererModeKind::ModeNormal as i32,
            degraded_reason: String::new(),
        },
        WebRendererMode::Recovery => proto::WebRendererModeProto {
            kind: proto::WebRendererModeKind::ModeRecovery as i32,
            degraded_reason: String::new(),
        },
        WebRendererMode::Degraded(reason) => proto::WebRendererModeProto {
            kind: proto::WebRendererModeKind::ModeDegraded as i32,
            degraded_reason: reason.clone(),
        },
    }
}

fn renderer_mode_from_proto(
    p: &proto::WebRendererModeProto,
) -> Result<WebRendererMode, WebRendererError> {
    let kind = proto::WebRendererModeKind::try_from(p.kind).map_err(|_| {
        WebRendererError::Internal(format!("invalid renderer mode kind: {}", p.kind))
    })?;
    match kind {
        proto::WebRendererModeKind::ModeNormal => Ok(WebRendererMode::Normal),
        proto::WebRendererModeKind::ModeRecovery => Ok(WebRendererMode::Recovery),
        proto::WebRendererModeKind::ModeDegraded => {
            Ok(WebRendererMode::Degraded(p.degraded_reason.clone()))
        }
        proto::WebRendererModeKind::WebRendererModeUnspecified => Err(WebRendererError::Internal(
            "unspecified renderer mode".into(),
        )),
    }
}

// ── ExposureLevelLabel ↔ proto ──────────────────────────────────────────

#[allow(dead_code)]
const fn exposure_level_label_to_proto(l: ExposureLevelLabel) -> proto::ExposureLevelLabelProto {
    match l {
        ExposureLevelLabel::Localhost => proto::ExposureLevelLabelProto::LabelLocalhost,
        ExposureLevelLabel::LanPending => proto::ExposureLevelLabelProto::LabelLanPending,
        ExposureLevelLabel::LanApproved => proto::ExposureLevelLabelProto::LabelLanApproved,
        ExposureLevelLabel::LanActive => proto::ExposureLevelLabelProto::LabelLanActive,
        ExposureLevelLabel::Public => proto::ExposureLevelLabelProto::LabelPublic,
        ExposureLevelLabel::Revoked => proto::ExposureLevelLabelProto::LabelRevoked,
    }
}

#[allow(dead_code)]
fn exposure_level_label_from_proto(
    p: proto::ExposureLevelLabelProto,
) -> Result<ExposureLevelLabel, WebRendererError> {
    match p {
        proto::ExposureLevelLabelProto::LabelLocalhost => Ok(ExposureLevelLabel::Localhost),
        proto::ExposureLevelLabelProto::LabelLanPending => Ok(ExposureLevelLabel::LanPending),
        proto::ExposureLevelLabelProto::LabelLanApproved => Ok(ExposureLevelLabel::LanApproved),
        proto::ExposureLevelLabelProto::LabelLanActive => Ok(ExposureLevelLabel::LanActive),
        proto::ExposureLevelLabelProto::LabelPublic => Ok(ExposureLevelLabel::Public),
        proto::ExposureLevelLabelProto::LabelRevoked => Ok(ExposureLevelLabel::Revoked),
        proto::ExposureLevelLabelProto::ExposureLevelLabelUnspecified => Err(
            WebRendererError::Internal("unspecified exposure level label".into()),
        ),
    }
}

// ── ExposureLevel → proto ───────────────────────────────────────────────

pub(crate) fn exposure_level_to_proto(level: &ExposureLevel) -> proto::ExposureLevelProto {
    match level {
        ExposureLevel::Localhost => proto::ExposureLevelProto {
            label: proto::ExposureLevelLabelProto::LabelLocalhost as i32,
            ..Default::default()
        },
        ExposureLevel::LanPending {
            since,
            approver_canonical_id,
        } => proto::ExposureLevelProto {
            label: proto::ExposureLevelLabelProto::LabelLanPending as i32,
            since: Some(datetime_to_proto(*since)),
            approver_canonical_id: Some(approver_canonical_id.clone()),
            ..Default::default()
        },
        ExposureLevel::LanApproved {
            granted_at,
            policy_decision_id,
        } => proto::ExposureLevelProto {
            label: proto::ExposureLevelLabelProto::LabelLanApproved as i32,
            granted_at: Some(datetime_to_proto(*granted_at)),
            policy_decision_id: Some(policy_decision_id.clone()),
            ..Default::default()
        },
        ExposureLevel::LanActive {
            activated_at,
            last_heartbeat_at,
        } => proto::ExposureLevelProto {
            label: proto::ExposureLevelLabelProto::LabelLanActive as i32,
            activated_at: Some(datetime_to_proto(*activated_at)),
            last_heartbeat_at: Some(datetime_to_proto(*last_heartbeat_at)),
            ..Default::default()
        },
        ExposureLevel::Public {
            granted_at,
            recovery_authorized_by,
            policy_decision_id,
        } => proto::ExposureLevelProto {
            label: proto::ExposureLevelLabelProto::LabelPublic as i32,
            granted_at: Some(datetime_to_proto(*granted_at)),
            recovery_authorized_by: Some(recovery_authorized_by.clone()),
            policy_decision_id: Some(policy_decision_id.clone()),
            ..Default::default()
        },
        ExposureLevel::Revoked { reason, revoked_at } => proto::ExposureLevelProto {
            label: proto::ExposureLevelLabelProto::LabelRevoked as i32,
            reason: Some(reason.clone()),
            revoked_at: Some(datetime_to_proto(*revoked_at)),
            ..Default::default()
        },
    }
}

// ── NodeKind ↔ proto ────────────────────────────────────────────────────

const fn node_kind_to_proto(k: NodeKind) -> proto::NodeKindProto {
    match k {
        NodeKind::Container => proto::NodeKindProto::Container,
        NodeKind::Divider => proto::NodeKindProto::Divider,
        NodeKind::Spacer => proto::NodeKindProto::Spacer,
        NodeKind::Text => proto::NodeKindProto::Text,
        NodeKind::Heading => proto::NodeKindProto::Heading,
        NodeKind::InlineCode => proto::NodeKindProto::InlineCode,
        NodeKind::CodeBlock => proto::NodeKindProto::CodeBlock,
        NodeKind::Card => proto::NodeKindProto::Card,
        NodeKind::List => proto::NodeKindProto::List,
        NodeKind::Table => proto::NodeKindProto::Table,
        NodeKind::Form => proto::NodeKindProto::Form,
        NodeKind::ActionButton => proto::NodeKindProto::ActionButton,
        NodeKind::Visualization => proto::NodeKindProto::Visualization,
        NodeKind::Stream => proto::NodeKindProto::Stream,
        NodeKind::SurfaceEmbed => proto::NodeKindProto::SurfaceEmbed,
        NodeKind::SecurityIndicator => proto::NodeKindProto::SecurityIndicator,
        NodeKind::ApprovalPrompt => proto::NodeKindProto::ApprovalPrompt,
        NodeKind::EvidenceLink => proto::NodeKindProto::EvidenceLink,
        NodeKind::AgentMessage => proto::NodeKindProto::AgentMessage,
    }
}

fn node_kind_from_proto(p: proto::NodeKindProto) -> Result<NodeKind, WebRendererError> {
    match p {
        proto::NodeKindProto::Container => Ok(NodeKind::Container),
        proto::NodeKindProto::Divider => Ok(NodeKind::Divider),
        proto::NodeKindProto::Spacer => Ok(NodeKind::Spacer),
        proto::NodeKindProto::Text => Ok(NodeKind::Text),
        proto::NodeKindProto::Heading => Ok(NodeKind::Heading),
        proto::NodeKindProto::InlineCode => Ok(NodeKind::InlineCode),
        proto::NodeKindProto::CodeBlock => Ok(NodeKind::CodeBlock),
        proto::NodeKindProto::Card => Ok(NodeKind::Card),
        proto::NodeKindProto::List => Ok(NodeKind::List),
        proto::NodeKindProto::Table => Ok(NodeKind::Table),
        proto::NodeKindProto::Form => Ok(NodeKind::Form),
        proto::NodeKindProto::ActionButton => Ok(NodeKind::ActionButton),
        proto::NodeKindProto::Visualization => Ok(NodeKind::Visualization),
        proto::NodeKindProto::Stream => Ok(NodeKind::Stream),
        proto::NodeKindProto::SurfaceEmbed => Ok(NodeKind::SurfaceEmbed),
        proto::NodeKindProto::SecurityIndicator => Ok(NodeKind::SecurityIndicator),
        proto::NodeKindProto::ApprovalPrompt => Ok(NodeKind::ApprovalPrompt),
        proto::NodeKindProto::EvidenceLink => Ok(NodeKind::EvidenceLink),
        proto::NodeKindProto::AgentMessage => Ok(NodeKind::AgentMessage),
        proto::NodeKindProto::NodeKindUnspecified => {
            Err(WebRendererError::Internal("unspecified node kind".into()))
        }
    }
}

// ── VisualTokenKind ↔ proto ─────────────────────────────────────────────

const fn visual_token_kind_to_proto(k: VisualTokenKind) -> proto::VisualTokenKindProto {
    match k {
        VisualTokenKind::Color => proto::VisualTokenKindProto::Color,
        VisualTokenKind::Font => proto::VisualTokenKindProto::Font,
        VisualTokenKind::Spacing => proto::VisualTokenKindProto::Spacing,
        VisualTokenKind::Motion => proto::VisualTokenKindProto::Motion,
        VisualTokenKind::Icon => proto::VisualTokenKindProto::Icon,
        VisualTokenKind::Shape => proto::VisualTokenKindProto::Shape,
        VisualTokenKind::Elevation => proto::VisualTokenKindProto::Elevation,
    }
}

fn visual_token_kind_from_proto(
    p: proto::VisualTokenKindProto,
) -> Result<VisualTokenKind, WebRendererError> {
    match p {
        proto::VisualTokenKindProto::Color => Ok(VisualTokenKind::Color),
        proto::VisualTokenKindProto::Font => Ok(VisualTokenKind::Font),
        proto::VisualTokenKindProto::Spacing => Ok(VisualTokenKind::Spacing),
        proto::VisualTokenKindProto::Motion => Ok(VisualTokenKind::Motion),
        proto::VisualTokenKindProto::Icon => Ok(VisualTokenKind::Icon),
        proto::VisualTokenKindProto::Shape => Ok(VisualTokenKind::Shape),
        proto::VisualTokenKindProto::Elevation => Ok(VisualTokenKind::Elevation),
        proto::VisualTokenKindProto::TokenKindUnspecified => {
            Err(WebRendererError::Internal("unspecified token kind".into()))
        }
    }
}

// ── ParsedOrigin ↔ proto ────────────────────────────────────────────────

pub(crate) fn parsed_origin_to_proto(o: &ParsedOrigin) -> proto::ParsedOriginProto {
    let (scheme_str, token) = match &o.scheme {
        OriginScheme::AiosLocalhost(t) => ("aios_localhost", Some(t.0.clone())),
        OriginScheme::Recovery => ("recovery", None),
        OriginScheme::AppOrigin(t) => ("app_origin", Some(t.0.clone())),
    };
    proto::ParsedOriginProto {
        full_origin: o.full_origin.clone(),
        host: o.host.clone(),
        port: u32::from(o.port),
        origin_scheme: scheme_str.to_string(),
        origin_token: token,
    }
}

fn parsed_origin_from_proto(
    p: &proto::ParsedOriginProto,
) -> Result<ParsedOrigin, WebRendererError> {
    OriginScheme::parse(&p.full_origin)
}

// ── WebSurfaceDescriptor ↔ proto ───────────────────────────────────────

pub(crate) fn surface_descriptor_to_proto(
    d: &WebSurfaceDescriptor,
) -> proto::WebSurfaceDescriptorProto {
    proto::WebSurfaceDescriptorProto {
        id: d.id.0.clone(),
        origin: Some(parsed_origin_to_proto(&d.origin)),
        node_kind: node_kind_to_proto(d.node_kind) as i32,
        claimed_by: d.claimed_by.clone(),
        mode: Some(renderer_mode_to_proto(&d.mode)),
        created_at: Some(datetime_to_proto(d.created_at)),
    }
}

// ── AllocateWebSurfaceRequest ↔ proto ──────────────────────────────────

pub fn allocate_request_from_proto(
    p: &proto::AllocateWebSurfaceRequestProto,
) -> Result<AllocateWebSurfaceRequest, WebRendererError> {
    let origin = p.origin.as_ref().map_or_else(
        || {
            Err(WebRendererError::Internal(
                "origin field is required".into(),
            ))
        },
        parsed_origin_from_proto,
    )?;
    let nk_proto = proto::NodeKindProto::try_from(p.node_kind)
        .map_err(|_| WebRendererError::Internal(format!("invalid node kind: {}", p.node_kind)))?;
    let node_kind = node_kind_from_proto(nk_proto)?;

    Ok(AllocateWebSurfaceRequest {
        origin,
        node_kind,
        claimed_by: p.claimed_by.clone(),
        expected_group_id: p.expected_group_id.clone(),
    })
}

// ── WebSurfaceReleaseReceipt → proto ────────────────────────────────────

pub(crate) fn release_receipt_to_proto(
    r: &WebSurfaceReleaseReceipt,
) -> proto::WebSurfaceReleaseReceiptProto {
    proto::WebSurfaceReleaseReceiptProto {
        id: r.id.0.clone(),
        released_at: Some(datetime_to_proto(r.released_at)),
        final_mode: Some(renderer_mode_to_proto(&r.final_mode)),
    }
}

// ── RecoveryEntryReceipt → proto ────────────────────────────────────────

pub(crate) fn recovery_receipt_to_proto(
    r: &RecoveryEntryReceipt,
) -> proto::RecoveryEntryReceiptProto {
    proto::RecoveryEntryReceiptProto {
        entered_at: Some(datetime_to_proto(r.entered_at)),
        recovery_origin: r.recovery_origin.clone(),
        service_worker_disabled: r.service_worker_disabled,
    }
}

// ── WebSurfaceFilter ↔ proto ────────────────────────────────────────────

pub(crate) fn surface_filter_from_proto(
    p: &proto::WebSurfaceFilterProto,
) -> Result<WebSurfaceFilter, WebRendererError> {
    match &p.filter {
        Some(proto::web_surface_filter_proto::Filter::ByOrigin(o)) => {
            Ok(WebSurfaceFilter::ByOrigin(o.clone()))
        }
        Some(proto::web_surface_filter_proto::Filter::ByClaimant(c)) => {
            Ok(WebSurfaceFilter::ByClaimant(c.clone()))
        }
        Some(proto::web_surface_filter_proto::Filter::ByNodeKind(k)) => {
            let np = proto::NodeKindProto::try_from(*k)
                .map_err(|_| WebRendererError::Internal(format!("invalid node kind: {k}")))?;
            let kind = node_kind_from_proto(np)?;
            Ok(WebSurfaceFilter::ByNodeKind(kind))
        }
        Some(proto::web_surface_filter_proto::Filter::InModeOnly(m)) => {
            let mode = renderer_mode_from_proto(m)?;
            Ok(WebSurfaceFilter::InModeOnly(mode))
        }
        Some(proto::web_surface_filter_proto::Filter::All(())) | None => Ok(WebSurfaceFilter::All),
    }
}

// ── RouteDescriptor ↔ proto ─────────────────────────────────────────────

pub(crate) fn route_descriptor_to_proto(d: &RouteDescriptor) -> proto::RouteDescriptorProto {
    proto::RouteDescriptorProto {
        path: d.path.clone(),
        requires_auth: d.requires_auth,
        served_in_recovery: d.served_in_recovery,
    }
}

pub(crate) fn route_descriptor_from_proto(p: &proto::RouteDescriptorProto) -> RouteDescriptor {
    RouteDescriptor {
        path: p.path.clone(),
        requires_auth: p.requires_auth,
        served_in_recovery: p.served_in_recovery,
    }
}

// ── VisualToken ↔ proto ─────────────────────────────────────────────────

pub(crate) fn visual_token_to_proto(t: &VisualToken) -> proto::VisualTokenProto {
    proto::VisualTokenProto {
        id: t.id.clone(),
        kind: visual_token_kind_to_proto(t.kind) as i32,
        canonical_value: t.canonical_value.clone(),
    }
}

pub(crate) fn visual_token_from_proto(
    p: &proto::VisualTokenProto,
) -> Result<VisualToken, WebRendererError> {
    let tk_proto = proto::VisualTokenKindProto::try_from(p.kind)
        .map_err(|_| WebRendererError::Internal(format!("invalid token kind: {}", p.kind)))?;
    let kind = visual_token_kind_from_proto(tk_proto)?;
    Ok(VisualToken {
        id: p.id.clone(),
        kind,
        canonical_value: p.canonical_value.clone(),
    })
}

// ── TokenApplicationReceipt → proto ─────────────────────────────────────

pub(crate) fn token_receipt_to_proto(
    r: &TokenApplicationReceipt,
) -> proto::TokenApplicationReceiptProto {
    proto::TokenApplicationReceiptProto {
        applied_count: r.applied_count as u32,
        timestamp: Some(datetime_to_proto(r.timestamp)),
    }
}

// ── web_error_to_status ─────────────────────────────────────────────────

/// Map a [`WebRendererError`] to a [`tonic::Status`] for gRPC responses.
#[must_use]
pub fn web_error_to_status(err: &WebRendererError) -> tonic::Status {
    match err {
        WebRendererError::SurfaceNotFound(id) => {
            tonic::Status::not_found(format!("surface not found: {id}"))
        }
        WebRendererError::OriginVerificationFailed {
            expected_group_id,
            presented_origin,
        } => tonic::Status::permission_denied(format!(
            "origin verification failed: expected group_id '{expected_group_id}', presented '{presented_origin}'"
        )),
        WebRendererError::ExposureEscalationDenied { from, to, reason } => {
            tonic::Status::failed_precondition(format!(
                "exposure escalation denied from {from} to {to}: {reason}"
            ))
        }
        WebRendererError::LanExposureWithoutEvidence => {
            tonic::Status::failed_precondition(
                "LAN exposure attempted without WEB_EXPOSURE_GRANTED evidence",
            )
        }
        WebRendererError::ChromeShadowRootIntegrityFailed { reason } => {
            tonic::Status::failed_precondition(format!(
                "chrome shadow root integrity failed: {reason}"
            ))
        }
        WebRendererError::CertificateVerificationFailed(msg) => {
            tonic::Status::unavailable(format!("certificate verification failed: {msg}"))
        }
        WebRendererError::PlainHttpRejected(msg) => {
            tonic::Status::failed_precondition(format!("plain HTTP rejected: {msg}"))
        }
        WebRendererError::IconBundleVerificationFailed { theme_id, reason } => {
            tonic::Status::failed_precondition(format!(
                "icon bundle verification failed for theme '{theme_id}': {reason}"
            ))
        }
        WebRendererError::WebgpuAdapterUnavailable(msg) => {
            tonic::Status::resource_exhausted(format!("webgpu adapter unavailable: {msg}"))
        }
        WebRendererError::ExtensionInterferenceDetected(msg) => {
            tonic::Status::permission_denied(format!(
                "extension interference detected: {msg}"
            ))
        }
        WebRendererError::Internal(msg) => {
            tonic::Status::internal(format!("internal renderer error: {msg}"))
        }
    }
}
