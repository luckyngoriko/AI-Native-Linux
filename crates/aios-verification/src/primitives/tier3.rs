//! Tier-3 network/control-plane/cross-crate primitive deferrals.

use serde_json::Value;

use crate::{PrimitiveResult, VerificationPrimitive};

use super::{primitive_result, ProbeVerdict};

const DEFERRED_PRIMITIVES: &[VerificationPrimitive] = &[
    VerificationPrimitive::HttpOk,
    VerificationPrimitive::AiosfsPointer,
    VerificationPrimitive::PolicyDecision,
    VerificationPrimitive::EvidenceExists,
    VerificationPrimitive::NetworkSubjectOutboundClass,
    VerificationPrimitive::NetworkActiveExposureClass,
    VerificationPrimitive::NetworkExternalModelCallBrokeredOnly,
    VerificationPrimitive::DnsResolverBackend,
    VerificationPrimitive::VpnTunnelActive,
    VerificationPrimitive::MdnsPosture,
    VerificationPrimitive::AiosfsPathInNamespace,
    VerificationPrimitive::SurfaceInZone,
    VerificationPrimitive::TreeContainsKind,
    VerificationPrimitive::ThemeSatisfiesInvariants,
    VerificationPrimitive::ThemeConstitutionalIconsIntact,
    VerificationPrimitive::GpuBindingClass,
    VerificationPrimitive::AiosfsPathOwnerResolved,
    VerificationPrimitive::AiosfsPathRecoveryTreatmentSet,
    VerificationPrimitive::StatusIndicatorVisible,
    VerificationPrimitive::FilesystemRootIntact,
    VerificationPrimitive::SpecConsumesTable,
    VerificationPrimitive::ApprovalBindingState,
];

/// Return all primitives deferred from M8 real execution.
#[must_use]
pub const fn deferred_primitives() -> &'static [VerificationPrimitive] {
    DEFERRED_PRIMITIVES
}

/// Return a deterministic `ProbeError` result for a deferred primitive.
#[must_use]
pub fn deferred_result(primitive: VerificationPrimitive, expected: &Value) -> PrimitiveResult {
    primitive_result(
        primitive,
        expected,
        ProbeVerdict::probe_error(format!(
            "{primitive} not yet implemented in M8; deferred to M16 aios-network/control-plane integration"
        )),
    )
}
