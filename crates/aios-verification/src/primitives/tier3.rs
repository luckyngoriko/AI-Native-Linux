//! Tier-3 cross-layer verification primitives (M20).
//!
//! Tier-3 primitives verify state owned by other layers (evidence, policy,
//! AIOS-FS, renderers, GPU, network/DNS/VPN/mDNS, approvals). The actual
//! observed state is obtained from an injected [`StateProbe`] — never from the
//! caller's `expected` payload — preserving the verification guarantee, exactly
//! as the Tier-2 [`super::LocalProbe`] does. Without a configured probe
//! ([`super::StdStateProbe`]) every observation is `None` and the primitive
//! returns a `PROBE_ERROR` (kept distinct from a `FAILED` verdict) rather than
//! a misleading pass.

use serde_json::Value;

use crate::{PrimitiveResult, VerificationPrimitive};

use super::{primitive_result, required_str, ProbeVerdict, StateProbe};

/// Comparison contract for a Tier-3 primitive: how to derive the lookup key
/// from `expected`, and how to judge the probed observed state.
#[derive(Debug, Clone, Copy)]
enum Comparison {
    /// Passes iff the probe returns any observed state for the key.
    Exists { key: &'static str },
    /// Passes iff `observed[observed_field]` equals `expected[expected_field]`.
    StringEq {
        key: &'static str,
        expected_field: &'static str,
        observed_field: &'static str,
    },
    /// Passes iff `observed[observed_field]` is the boolean `true`.
    BoolTrue {
        key: &'static str,
        observed_field: &'static str,
    },
    /// Passes iff the array `observed[observed_field]` contains the string
    /// `expected[expected_field]`.
    ArrayContains {
        key: &'static str,
        expected_field: &'static str,
        observed_field: &'static str,
    },
}

impl Comparison {
    const fn key_field(self) -> &'static str {
        match self {
            Self::Exists { key }
            | Self::StringEq { key, .. }
            | Self::BoolTrue { key, .. }
            | Self::ArrayContains { key, .. } => key,
        }
    }
}

/// Return the comparison contract for a Tier-3 primitive, or `None` if the
/// primitive is not Tier-3.
#[allow(
    clippy::too_many_lines,
    reason = "one explicit arm per Tier-3 primitive is clearer than a lookup table"
)]
const fn comparison_for(primitive: VerificationPrimitive) -> Option<Comparison> {
    use VerificationPrimitive as P;
    let cmp = match primitive {
        // ── Evidence + policy (L9 / L4) ──
        P::EvidenceExists => Comparison::Exists { key: "receipt_id" },
        P::PolicyDecision => Comparison::StringEq {
            key: "policy_decision_id",
            expected_field: "expected_decision",
            observed_field: "observed_decision",
        },
        // ── AIOS-FS (L2) ──
        P::AiosfsPointer => Comparison::StringEq {
            key: "object_id",
            expected_field: "expected_version_id",
            observed_field: "observed_version_id",
        },
        P::AiosfsPathInNamespace => Comparison::StringEq {
            key: "path",
            expected_field: "expected_namespace_class",
            observed_field: "namespace_class",
        },
        P::AiosfsPathOwnerResolved => Comparison::StringEq {
            key: "path",
            expected_field: "expected_owner_subject_id",
            observed_field: "owner_subject_id",
        },
        P::AiosfsPathRecoveryTreatmentSet => Comparison::StringEq {
            key: "path",
            expected_field: "expected_recovery_treatment",
            observed_field: "recovery_treatment",
        },
        P::FilesystemRootIntact => Comparison::BoolTrue {
            key: "root",
            observed_field: "intact",
        },
        P::SpecConsumesTable => Comparison::ArrayContains {
            key: "spec_id",
            expected_field: "expected_consumes_entry",
            observed_field: "consumes",
        },
        // ── Renderers / theme / GPU (L7 / L8) ──
        P::SurfaceInZone => Comparison::StringEq {
            key: "surface_id",
            expected_field: "expected_zone",
            observed_field: "zone",
        },
        P::TreeContainsKind => Comparison::ArrayContains {
            key: "tree_id",
            expected_field: "expected_kind",
            observed_field: "kinds",
        },
        P::ThemeSatisfiesInvariants => Comparison::BoolTrue {
            key: "theme_id",
            observed_field: "satisfies_invariants",
        },
        P::ThemeConstitutionalIconsIntact => Comparison::BoolTrue {
            key: "theme_id",
            observed_field: "icons_intact",
        },
        P::StatusIndicatorVisible => Comparison::BoolTrue {
            key: "indicator_id",
            observed_field: "visible",
        },
        P::GpuBindingClass => Comparison::StringEq {
            key: "binding_id",
            expected_field: "expected_class",
            observed_field: "binding_class",
        },
        // ── Approval (L4) ──
        P::ApprovalBindingState => Comparison::StringEq {
            key: "approval_id",
            expected_field: "expected_state",
            observed_field: "observed_state",
        },
        // ── Network / DNS / VPN / mDNS (L8) ──
        P::NetworkSubjectOutboundClass => Comparison::StringEq {
            key: "subject_id",
            expected_field: "expected_class",
            observed_field: "outbound_class",
        },
        P::NetworkActiveExposureClass => Comparison::StringEq {
            key: "surface_id",
            expected_field: "expected_class",
            observed_field: "exposure_class",
        },
        P::NetworkExternalModelCallBrokeredOnly => Comparison::BoolTrue {
            key: "subject_id",
            observed_field: "brokered_only",
        },
        P::DnsResolverBackend => Comparison::StringEq {
            key: "host_id",
            expected_field: "expected_transport",
            observed_field: "transport",
        },
        P::VpnTunnelActive => Comparison::BoolTrue {
            key: "tunnel_id",
            observed_field: "active",
        },
        P::MdnsPosture => Comparison::StringEq {
            key: "host_id",
            expected_field: "expected_posture",
            observed_field: "posture",
        },
        // ── HTTP (L8; probed via StateProbe, never live I/O in the verdict) ──
        P::HttpOk => Comparison::BoolTrue {
            key: "url",
            observed_field: "ok",
        },
        // Not a Tier-3 primitive (Tier-1 / Tier-2).
        _ => return None,
    };
    Some(cmp)
}

/// Execute a Tier-3 primitive against the injected [`StateProbe`].
pub async fn execute(
    primitive: VerificationPrimitive,
    expected: &Value,
    probe: &dyn StateProbe,
) -> PrimitiveResult {
    let verdict = run(primitive, expected, probe).await;
    primitive_result(primitive, expected, verdict)
}

async fn run(
    primitive: VerificationPrimitive,
    expected: &Value,
    probe: &dyn StateProbe,
) -> ProbeVerdict {
    let Some(cmp) = comparison_for(primitive) else {
        return ProbeVerdict::probe_error(format!("{primitive} is not a Tier-3 primitive"));
    };
    let key = match required_str(expected, cmp.key_field()) {
        Ok(key) => key,
        Err(verdict) => return verdict,
    };
    let Some(observed) = probe.observe(primitive, key).await else {
        return ProbeVerdict::probe_error(format!(
            "{primitive}: state source unavailable or `{key}` not found"
        ));
    };
    judge(primitive, expected, cmp, observed)
}

fn judge(
    primitive: VerificationPrimitive,
    expected: &Value,
    cmp: Comparison,
    observed: Value,
) -> ProbeVerdict {
    match cmp {
        Comparison::Exists { .. } => ProbeVerdict::passed(observed),
        Comparison::StringEq {
            expected_field,
            observed_field,
            ..
        } => {
            let want = match required_str(expected, expected_field) {
                Ok(want) => want.to_owned(),
                Err(verdict) => return verdict,
            };
            let decision = observed
                .get(observed_field)
                .and_then(Value::as_str)
                .map(|got| got == want);
            match decision {
                Some(true) => ProbeVerdict::passed(observed),
                Some(false) => ProbeVerdict::failed(observed),
                None => ProbeVerdict::probe_error(format!(
                    "{primitive}: observed state missing string `{observed_field}`"
                )),
            }
        }
        Comparison::BoolTrue { observed_field, .. } => {
            match observed.get(observed_field).and_then(Value::as_bool) {
                Some(true) => ProbeVerdict::passed(observed),
                Some(false) => ProbeVerdict::failed(observed),
                None => ProbeVerdict::probe_error(format!(
                    "{primitive}: observed state missing bool `{observed_field}`"
                )),
            }
        }
        Comparison::ArrayContains {
            expected_field,
            observed_field,
            ..
        } => {
            let want = match required_str(expected, expected_field) {
                Ok(want) => want.to_owned(),
                Err(verdict) => return verdict,
            };
            let decision = observed
                .get(observed_field)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .any(|item| item.as_str() == Some(want.as_str()))
                });
            match decision {
                Some(true) => ProbeVerdict::passed(observed),
                Some(false) => ProbeVerdict::failed(observed),
                None => ProbeVerdict::probe_error(format!(
                    "{primitive}: observed state missing array `{observed_field}`"
                )),
            }
        }
    }
}
