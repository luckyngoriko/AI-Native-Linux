//! Rust ↔ proto translations for the gRPC `KdeRendererService` surface (T-134).
//!
//! Owns bidirectional translation between domain types and tonic-generated proto
//! types, plus the `kde_error_to_status` mapper.

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

use crate::error::KdeRendererError;
use crate::kwin_script::KwinScript;
use crate::node_kind::NodeKind;
use crate::renderer::{
    AllocateSurfaceRequest, RecoveryEntryReceipt, SurfaceFilter, SurfaceReleaseReceipt,
    TokenApplicationReceipt,
};
use crate::service::proto;
use crate::types::{KdeSurfaceDescriptor, KdeSurfaceId, RendererMode};
use crate::visual_token::{VisualToken, VisualTokenKind};
use crate::wayland::{
    WaylandInteractivity, WaylandProtocol, WaylandSurfaceGrant, WaylandSurfaceLayer,
    WaylandSurfaceRequest,
};
use crate::zone::{CompositionZone, ZoneLayer};

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

// ── CompositionZone ↔ proto ──────────────────────────────────────────────

impl From<CompositionZone> for proto::CompositionZoneProto {
    fn from(z: CompositionZone) -> Self {
        match z {
            CompositionZone::Chrome => Self::Chrome,
            CompositionZone::Content => Self::Content,
            CompositionZone::Background => Self::Background,
            CompositionZone::Recovery => Self::Recovery,
        }
    }
}

impl TryFrom<proto::CompositionZoneProto> for CompositionZone {
    type Error = KdeRendererError;

    fn try_from(p: proto::CompositionZoneProto) -> Result<Self, Self::Error> {
        match p {
            proto::CompositionZoneProto::Chrome => Ok(CompositionZone::Chrome),
            proto::CompositionZoneProto::Content => Ok(CompositionZone::Content),
            proto::CompositionZoneProto::Background => Ok(CompositionZone::Background),
            proto::CompositionZoneProto::Recovery => Ok(CompositionZone::Recovery),
            proto::CompositionZoneProto::ZoneUnspecified => {
                Err(KdeRendererError::Internal("unspecified zone".into()))
            }
        }
    }
}

// ── ZoneLayer ↔ proto ────────────────────────────────────────────────────

impl From<ZoneLayer> for proto::ZoneLayerProto {
    fn from(l: ZoneLayer) -> Self {
        match l {
            ZoneLayer::Background => Self::ZlBackground,
            ZoneLayer::Bottom => Self::ZlBottom,
            ZoneLayer::Top => Self::ZlTop,
            ZoneLayer::Overlay => Self::ZlOverlay,
        }
    }
}

impl TryFrom<proto::ZoneLayerProto> for ZoneLayer {
    type Error = KdeRendererError;

    fn try_from(p: proto::ZoneLayerProto) -> Result<Self, Self::Error> {
        match p {
            proto::ZoneLayerProto::ZlBackground => Ok(ZoneLayer::Background),
            proto::ZoneLayerProto::ZlBottom => Ok(ZoneLayer::Bottom),
            proto::ZoneLayerProto::ZlTop => Ok(ZoneLayer::Top),
            proto::ZoneLayerProto::ZlOverlay => Ok(ZoneLayer::Overlay),
            proto::ZoneLayerProto::ZoneLayerUnspecified => {
                Err(KdeRendererError::Internal("unspecified zone layer".into()))
            }
        }
    }
}

// ── NodeKind ↔ proto ─────────────────────────────────────────────────────

impl TryFrom<proto::NodeKindProto> for NodeKind {
    type Error = KdeRendererError;

    fn try_from(p: proto::NodeKindProto) -> Result<Self, Self::Error> {
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
                Err(KdeRendererError::Internal("unspecified node kind".into()))
            }
        }
    }
}

impl From<NodeKind> for proto::NodeKindProto {
    fn from(k: NodeKind) -> Self {
        match k {
            NodeKind::Container => Self::Container,
            NodeKind::Divider => Self::Divider,
            NodeKind::Spacer => Self::Spacer,
            NodeKind::Text => Self::Text,
            NodeKind::Heading => Self::Heading,
            NodeKind::InlineCode => Self::InlineCode,
            NodeKind::CodeBlock => Self::CodeBlock,
            NodeKind::Card => Self::Card,
            NodeKind::List => Self::List,
            NodeKind::Table => Self::Table,
            NodeKind::Form => Self::Form,
            NodeKind::ActionButton => Self::ActionButton,
            NodeKind::Visualization => Self::Visualization,
            NodeKind::Stream => Self::Stream,
            NodeKind::SurfaceEmbed => Self::SurfaceEmbed,
            NodeKind::SecurityIndicator => Self::SecurityIndicator,
            NodeKind::ApprovalPrompt => Self::ApprovalPrompt,
            NodeKind::EvidenceLink => Self::EvidenceLink,
            NodeKind::AgentMessage => Self::AgentMessage,
        }
    }
}

// ── VisualTokenKind ↔ proto ──────────────────────────────────────────────

impl From<VisualTokenKind> for proto::VisualTokenKindProto {
    fn from(k: VisualTokenKind) -> Self {
        match k {
            VisualTokenKind::Color => Self::Color,
            VisualTokenKind::Font => Self::Font,
            VisualTokenKind::Spacing => Self::Spacing,
            VisualTokenKind::Motion => Self::Motion,
            VisualTokenKind::Icon => Self::Icon,
            VisualTokenKind::Shape => Self::Shape,
            VisualTokenKind::Elevation => Self::Elevation,
        }
    }
}

impl TryFrom<proto::VisualTokenKindProto> for VisualTokenKind {
    type Error = KdeRendererError;

    fn try_from(p: proto::VisualTokenKindProto) -> Result<Self, Self::Error> {
        match p {
            proto::VisualTokenKindProto::Color => Ok(VisualTokenKind::Color),
            proto::VisualTokenKindProto::Font => Ok(VisualTokenKind::Font),
            proto::VisualTokenKindProto::Spacing => Ok(VisualTokenKind::Spacing),
            proto::VisualTokenKindProto::Motion => Ok(VisualTokenKind::Motion),
            proto::VisualTokenKindProto::Icon => Ok(VisualTokenKind::Icon),
            proto::VisualTokenKindProto::Shape => Ok(VisualTokenKind::Shape),
            proto::VisualTokenKindProto::Elevation => Ok(VisualTokenKind::Elevation),
            proto::VisualTokenKindProto::TokenKindUnspecified => {
                Err(KdeRendererError::Internal("unspecified token kind".into()))
            }
        }
    }
}

// ── WaylandProtocol ↔ proto ──────────────────────────────────────────────

impl From<WaylandProtocol> for proto::WaylandProtocolProto {
    fn from(p: WaylandProtocol) -> Self {
        match p {
            WaylandProtocol::WlCompositor => Self::WlCompositor,
            WaylandProtocol::XdgShell => Self::XdgShell,
            WaylandProtocol::WlrLayerShellV1 => Self::WlrLayerShellV1,
            WaylandProtocol::WpViewporter => Self::WpViewporter,
            WaylandProtocol::ZwpLinuxDmabufV1 => Self::ZwpLinuxDmabufV1,
            WaylandProtocol::XdgDecorationV1 => Self::XdgDecorationV1,
            WaylandProtocol::IdleInhibitV1 => Self::IdleInhibitV1,
        }
    }
}

impl TryFrom<proto::WaylandProtocolProto> for WaylandProtocol {
    type Error = KdeRendererError;

    fn try_from(p: proto::WaylandProtocolProto) -> Result<Self, Self::Error> {
        match p {
            proto::WaylandProtocolProto::WlCompositor => Ok(WaylandProtocol::WlCompositor),
            proto::WaylandProtocolProto::XdgShell => Ok(WaylandProtocol::XdgShell),
            proto::WaylandProtocolProto::WlrLayerShellV1 => Ok(WaylandProtocol::WlrLayerShellV1),
            proto::WaylandProtocolProto::WpViewporter => Ok(WaylandProtocol::WpViewporter),
            proto::WaylandProtocolProto::ZwpLinuxDmabufV1 => Ok(WaylandProtocol::ZwpLinuxDmabufV1),
            proto::WaylandProtocolProto::XdgDecorationV1 => Ok(WaylandProtocol::XdgDecorationV1),
            proto::WaylandProtocolProto::IdleInhibitV1 => Ok(WaylandProtocol::IdleInhibitV1),
            proto::WaylandProtocolProto::WaylandProtocolUnspecified => Err(
                KdeRendererError::Internal("unspecified wayland protocol".into()),
            ),
        }
    }
}

// ── WaylandSurfaceLayer ↔ proto ──────────────────────────────────────────

impl From<WaylandSurfaceLayer> for proto::WaylandSurfaceLayerProto {
    fn from(l: WaylandSurfaceLayer) -> Self {
        match l {
            WaylandSurfaceLayer::Background => Self::WslBackground,
            WaylandSurfaceLayer::Bottom => Self::WslBottom,
            WaylandSurfaceLayer::Top => Self::WslTop,
            WaylandSurfaceLayer::Overlay => Self::WslOverlay,
        }
    }
}

impl TryFrom<proto::WaylandSurfaceLayerProto> for WaylandSurfaceLayer {
    type Error = KdeRendererError;

    fn try_from(p: proto::WaylandSurfaceLayerProto) -> Result<Self, Self::Error> {
        match p {
            proto::WaylandSurfaceLayerProto::WslBackground => Ok(WaylandSurfaceLayer::Background),
            proto::WaylandSurfaceLayerProto::WslBottom => Ok(WaylandSurfaceLayer::Bottom),
            proto::WaylandSurfaceLayerProto::WslTop => Ok(WaylandSurfaceLayer::Top),
            proto::WaylandSurfaceLayerProto::WslOverlay => Ok(WaylandSurfaceLayer::Overlay),
            proto::WaylandSurfaceLayerProto::WaylandSurfaceLayerUnspecified => Err(
                KdeRendererError::Internal("unspecified wayland surface layer".into()),
            ),
        }
    }
}

// ── WaylandInteractivity ↔ proto ─────────────────────────────────────────

impl From<WaylandInteractivity> for proto::WaylandInteractivityProto {
    fn from(i: WaylandInteractivity) -> Self {
        match i {
            WaylandInteractivity::None => Self::None,
            WaylandInteractivity::OnDemand => Self::OnDemand,
            WaylandInteractivity::Exclusive => Self::Exclusive,
        }
    }
}

impl TryFrom<proto::WaylandInteractivityProto> for WaylandInteractivity {
    type Error = KdeRendererError;

    fn try_from(p: proto::WaylandInteractivityProto) -> Result<Self, Self::Error> {
        match p {
            proto::WaylandInteractivityProto::None => Ok(WaylandInteractivity::None),
            proto::WaylandInteractivityProto::OnDemand => Ok(WaylandInteractivity::OnDemand),
            proto::WaylandInteractivityProto::Exclusive => Ok(WaylandInteractivity::Exclusive),
            proto::WaylandInteractivityProto::InteractivityUnspecified => Err(
                KdeRendererError::Internal("unspecified interactivity".into()),
            ),
        }
    }
}

// ── RendererMode ↔ proto ─────────────────────────────────────────────────

pub(crate) fn renderer_mode_to_proto(mode: &RendererMode) -> proto::RendererModeProto {
    match mode {
        RendererMode::Normal => proto::RendererModeProto {
            kind: proto::KdeRendererModeKind::ModeNormal as i32,
            degraded_reason: String::new(),
        },
        RendererMode::Degraded(reason) => proto::RendererModeProto {
            kind: proto::KdeRendererModeKind::ModeDegraded as i32,
            degraded_reason: reason.clone(),
        },
        RendererMode::Recovery => proto::RendererModeProto {
            kind: proto::KdeRendererModeKind::ModeRecovery as i32,
            degraded_reason: String::new(),
        },
    }
}

fn renderer_mode_from_proto(
    p: &proto::RendererModeProto,
) -> Result<RendererMode, KdeRendererError> {
    let kind = proto::KdeRendererModeKind::try_from(p.kind).map_err(|_| {
        KdeRendererError::Internal(format!("invalid renderer mode kind: {}", p.kind))
    })?;
    match kind {
        proto::KdeRendererModeKind::ModeNormal => Ok(RendererMode::Normal),
        proto::KdeRendererModeKind::ModeDegraded => {
            Ok(RendererMode::Degraded(p.degraded_reason.clone()))
        }
        proto::KdeRendererModeKind::ModeRecovery => Ok(RendererMode::Recovery),
        proto::KdeRendererModeKind::KdeRendererModeUnspecified => Err(KdeRendererError::Internal(
            "unspecified renderer mode".into(),
        )),
    }
}

// ── KdeSurfaceDescriptor ↔ proto ────────────────────────────────────────

pub(crate) fn surface_descriptor_to_proto(
    d: &KdeSurfaceDescriptor,
) -> proto::KdeSurfaceDescriptorProto {
    proto::KdeSurfaceDescriptorProto {
        id: d.id.0.clone(),
        zone: proto::CompositionZoneProto::from(d.zone) as i32,
        layer: proto::ZoneLayerProto::from(d.layer) as i32,
        mode: Some(renderer_mode_to_proto(&d.mode)),
        created_at: Some(datetime_to_proto(d.created_at)),
        claimed_by: d.claimed_by.clone(),
    }
}

#[allow(dead_code)]
fn surface_descriptor_from_proto(
    p: &proto::KdeSurfaceDescriptorProto,
) -> Result<KdeSurfaceDescriptor, KdeRendererError> {
    let zone_proto = proto::CompositionZoneProto::try_from(p.zone)
        .map_err(|_| KdeRendererError::Internal(format!("invalid zone: {}", p.zone)))?;
    let zone = CompositionZone::try_from(zone_proto)?;
    let layer_proto = proto::ZoneLayerProto::try_from(p.layer)
        .map_err(|_| KdeRendererError::Internal(format!("invalid layer: {}", p.layer)))?;
    let layer = ZoneLayer::try_from(layer_proto)?;
    let mode = p
        .mode
        .as_ref()
        .map_or(Ok(RendererMode::Normal), renderer_mode_from_proto)?;
    let created_at = p
        .created_at
        .clone()
        .map_or_else(Utc::now, datetime_from_proto);

    Ok(KdeSurfaceDescriptor {
        id: KdeSurfaceId(p.id.clone()),
        zone,
        layer,
        mode,
        created_at,
        claimed_by: p.claimed_by.clone(),
    })
}

// ── AllocateSurfaceRequest ↔ proto ───────────────────────────────────────

pub fn allocate_request_from_proto(
    p: &proto::AllocateSurfaceRequestProto,
) -> Result<AllocateSurfaceRequest, KdeRendererError> {
    let zone_proto = proto::CompositionZoneProto::try_from(p.zone)
        .map_err(|_| KdeRendererError::Internal(format!("invalid zone: {}", p.zone)))?;
    let zone = CompositionZone::try_from(zone_proto)?;
    let nk_proto = proto::NodeKindProto::try_from(p.node_kind)
        .map_err(|_| KdeRendererError::Internal(format!("invalid node kind: {}", p.node_kind)))?;
    let node_kind = NodeKind::try_from(nk_proto)?;
    let requested_layer = if let Some(l) = p.requested_layer {
        let zl_proto = proto::ZoneLayerProto::try_from(l)
            .map_err(|_| KdeRendererError::Internal(format!("invalid layer: {l}")))?;
        Some(ZoneLayer::try_from(zl_proto)?)
    } else {
        None
    };

    Ok(AllocateSurfaceRequest {
        zone,
        claimed_by: p.claimed_by.clone(),
        node_kind,
        requested_layer,
    })
}

// ── SurfaceReleaseReceipt → proto ────────────────────────────────────────

pub(crate) fn release_receipt_to_proto(
    r: &SurfaceReleaseReceipt,
) -> proto::SurfaceReleaseReceiptProto {
    proto::SurfaceReleaseReceiptProto {
        id: r.id.0.clone(),
        released_at: Some(datetime_to_proto(r.released_at)),
        final_mode: Some(renderer_mode_to_proto(&r.final_mode)),
    }
}

// ── RecoveryEntryReceipt → proto ─────────────────────────────────────────

pub(crate) fn recovery_receipt_to_proto(
    r: &RecoveryEntryReceipt,
) -> proto::RecoveryEntryReceiptProto {
    proto::RecoveryEntryReceiptProto {
        entered_at: Some(datetime_to_proto(r.entered_at)),
        aios_surfaces_only: r.aios_surfaces_only,
        display_separation: r.display_separation.clone(),
    }
}

// ── SurfaceFilter ↔ proto ────────────────────────────────────────────────

pub(crate) fn surface_filter_from_proto(
    p: &proto::SurfaceFilterProto,
) -> Result<SurfaceFilter, KdeRendererError> {
    match &p.filter {
        Some(proto::surface_filter_proto::Filter::ByZone(z)) => {
            let zp = proto::CompositionZoneProto::try_from(*z)
                .map_err(|_| KdeRendererError::Internal(format!("invalid zone: {z}")))?;
            let zone = CompositionZone::try_from(zp)?;
            Ok(SurfaceFilter::ByZone(zone))
        }
        Some(proto::surface_filter_proto::Filter::ByClaimant(c)) => {
            Ok(SurfaceFilter::ByClaimant(c.clone()))
        }
        Some(proto::surface_filter_proto::Filter::ByNodeKind(k)) => {
            let np = proto::NodeKindProto::try_from(*k)
                .map_err(|_| KdeRendererError::Internal(format!("invalid node kind: {k}")))?;
            let kind = NodeKind::try_from(np)?;
            Ok(SurfaceFilter::ByNodeKind(kind))
        }
        Some(proto::surface_filter_proto::Filter::InModeOnly(m)) => {
            let mode = renderer_mode_from_proto(m)?;
            Ok(SurfaceFilter::InModeOnly(mode))
        }
        Some(proto::surface_filter_proto::Filter::All(())) | None => Ok(SurfaceFilter::All),
    }
}

#[allow(dead_code)]
pub(crate) fn surface_filter_to_proto(f: &SurfaceFilter) -> proto::SurfaceFilterProto {
    let filter = match f {
        SurfaceFilter::All => proto::surface_filter_proto::Filter::All(()),
        SurfaceFilter::ByZone(z) => {
            proto::surface_filter_proto::Filter::ByZone(proto::CompositionZoneProto::from(*z) as i32)
        }
        SurfaceFilter::ByClaimant(c) => proto::surface_filter_proto::Filter::ByClaimant(c.clone()),
        SurfaceFilter::ByNodeKind(k) => {
            proto::surface_filter_proto::Filter::ByNodeKind(proto::NodeKindProto::from(*k) as i32)
        }
        SurfaceFilter::InModeOnly(m) => {
            proto::surface_filter_proto::Filter::InModeOnly(renderer_mode_to_proto(m))
        }
    };
    proto::SurfaceFilterProto {
        filter: Some(filter),
    }
}

// ── VisualToken ↔ proto ──────────────────────────────────────────────────

pub(crate) fn visual_token_to_proto(t: &VisualToken) -> proto::VisualTokenProto {
    proto::VisualTokenProto {
        id: t.id.clone(),
        kind: proto::VisualTokenKindProto::from(t.kind) as i32,
        canonical_value: t.canonical_value.clone(),
    }
}

pub(crate) fn visual_token_from_proto(
    p: &proto::VisualTokenProto,
) -> Result<VisualToken, KdeRendererError> {
    let tk_proto = proto::VisualTokenKindProto::try_from(p.kind)
        .map_err(|_| KdeRendererError::Internal(format!("invalid token kind: {}", p.kind)))?;
    let kind = VisualTokenKind::try_from(tk_proto)?;
    Ok(VisualToken {
        id: p.id.clone(),
        kind,
        canonical_value: p.canonical_value.clone(),
    })
}

// ── TokenApplicationReceipt → proto ──────────────────────────────────────

pub(crate) fn token_receipt_to_proto(
    r: &TokenApplicationReceipt,
) -> proto::TokenApplicationReceiptProto {
    proto::TokenApplicationReceiptProto {
        applied_count: r.applied_count as u32,
        timestamp: Some(datetime_to_proto(r.timestamp)),
    }
}

// ── WaylandSurfaceRequest ↔ proto ────────────────────────────────────────

pub(crate) fn wayland_request_from_proto(
    p: &proto::WaylandSurfaceRequestProto,
) -> Result<WaylandSurfaceRequest, KdeRendererError> {
    let wp_proto = proto::WaylandProtocolProto::try_from(p.protocol)
        .map_err(|_| KdeRendererError::Internal(format!("invalid protocol: {}", p.protocol)))?;
    let protocol = WaylandProtocol::try_from(wp_proto)?;
    let zone_proto = proto::CompositionZoneProto::try_from(p.zone)
        .map_err(|_| KdeRendererError::Internal(format!("invalid zone: {}", p.zone)))?;
    let zone = CompositionZone::try_from(zone_proto)?;
    let nk_proto = proto::NodeKindProto::try_from(p.node_kind)
        .map_err(|_| KdeRendererError::Internal(format!("invalid node kind: {}", p.node_kind)))?;
    let node_kind = NodeKind::try_from(nk_proto)?;

    Ok(WaylandSurfaceRequest {
        protocol,
        layer_namespace: p.layer_namespace.clone(),
        claimed_by: p.claimed_by.clone(),
        zone,
        node_kind,
    })
}

// ── WaylandSurfaceGrant → proto ──────────────────────────────────────────

pub(crate) fn wayland_grant_to_proto(g: &WaylandSurfaceGrant) -> proto::WaylandSurfaceGrantProto {
    proto::WaylandSurfaceGrantProto {
        assigned_layer: proto::WaylandSurfaceLayerProto::from(g.assigned_layer) as i32,
        interactivity: proto::WaylandInteractivityProto::from(g.interactivity) as i32,
        exclusive_zone: g.exclusive_zone,
    }
}

// ── KwinScript ↔ proto ───────────────────────────────────────────────────

pub(crate) fn kwin_script_from_proto(p: &proto::KwinScriptProto) -> KwinScript {
    KwinScript {
        id: p.id.clone(),
        canonical_path: p.canonical_path.clone(),
        source: p.source.clone(),
        blake3_hash: p.blake3_hash.clone(),
        signature: p.signature.clone(),
        signer_key_fingerprint: p.signer_key_fingerprint.clone(),
    }
}

// ── kde_error_to_status ──────────────────────────────────────────────────

/// Map a [`KdeRendererError`] to a [`tonic::Status`] for gRPC responses.
#[must_use]
pub fn kde_error_to_status(err: &KdeRendererError) -> tonic::Status {
    match err {
        KdeRendererError::SurfaceNotFound(id) => {
            tonic::Status::not_found(format!("surface not found: {id}"))
        }
        KdeRendererError::OverlayLayerForbidden { client_id } => tonic::Status::permission_denied(
            format!("overlay layer forbidden for client '{client_id}'"),
        ),
        KdeRendererError::WaylandConnectError(msg) => {
            tonic::Status::unavailable(format!("wayland connect error: {msg}"))
        }
        KdeRendererError::KwinScriptVerificationFailed { script_id, reason } => {
            tonic::Status::permission_denied(format!(
                "kwin script verification failed for '{script_id}': {reason}"
            ))
        }
        KdeRendererError::IconBundleVerificationFailed { theme_id, reason } => {
            tonic::Status::failed_precondition(format!(
                "icon bundle verification failed for theme '{theme_id}': {reason}"
            ))
        }
        KdeRendererError::GpuBindingUnavailable(msg) => {
            tonic::Status::resource_exhausted(format!("gpu binding unavailable: {msg}"))
        }
        KdeRendererError::Degraded(msg) => {
            tonic::Status::failed_precondition(format!("renderer degraded: {msg}"))
        }
        KdeRendererError::Internal(msg) => {
            tonic::Status::internal(format!("internal renderer error: {msg}"))
        }
        KdeRendererError::InvalidZoneTransition { from, to } => tonic::Status::invalid_argument(
            format!("invalid zone transition from {from:?} to {to:?}"),
        ),
    }
}
