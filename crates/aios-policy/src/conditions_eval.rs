//! Evaluator for the §9 conditions DSL (T-019).
//!
//! Given a parsed [`Condition`] AST and a borrowed [`EvalContext`], [`evaluate`]
//! returns `true` iff every predicate in the conjunction holds.
//!
//! ## Context shape
//!
//! The context borrows the hydrated subject, the action envelope, the enrichment
//! snapshot stub, and a clock snapshot. Nothing is owned; the evaluator does not
//! clone the heavy structures.
//!
//! ## Type-mismatch handling
//!
//! Operators are defined only on certain value types (e.g. `<` on integers,
//! `contains` on strings/lists, `in` on closed-enum/string fields). When a bundle
//! pairs a predicate with a value of the wrong type, the evaluator returns
//! [`ConditionEvalError::TypeMismatch`] instead of silently evaluating to `false` —
//! a silent `false` would hide bundle-author bugs that §17 explicitly requires to be
//! surfaced (`InvalidPolicyBundle` with `reason = "type_mismatch"`). The bundle
//! loader (T-022) will hoist this check to compile time once the type table is
//! wired into it; until then, the evaluator's runtime check is the only mechanical
//! floor.
//!
//! ## Short-circuit semantics
//!
//! Conjunction is left-to-right and short-circuits on the first `false` (no
//! subsequent predicate is evaluated). The `Exists` predicate is always safe to
//! evaluate first — it never raises a type-mismatch error, so bundle authors can
//! place `field exists and field = "X"` to guard the equality with an existence
//! check.

use thiserror::Error;

use aios_action::ActionEnvelope;

use crate::conditions::{ClosedField, CompareOp, Condition, Predicate, Value};
use crate::snapshot::EnrichmentSnapshot;
use crate::subject::{HydratedSubject, SubjectType};

/// Per-evaluation read-only context for the conditions evaluator.
///
/// Exposes every namespace the §9 predicates can reference. Constructed by the
/// pipeline driver (today: by the test harness; T-022 will hoist this into the
/// bundle-rule evaluation step).
///
/// The fields are borrowed (no ownership transfer) so the evaluator never clones
/// the heavy [`ActionEnvelope`] or [`HydratedSubject`]. The [`EnrichmentSnapshot`]
/// stub is included for forward compatibility — when T-022 fills it in with the
/// per-object metadata (`privacy_class`, `policy_tags`, …), the evaluator
/// signature does not need to change.
#[derive(Debug, Clone, Copy)]
pub struct EvalContext<'ctx> {
    /// Hydrated subject (§7).
    pub subject: &'ctx HydratedSubject,
    /// The submitted action envelope (§9.2 `request.*` source).
    pub envelope: &'ctx ActionEnvelope,
    /// Enrichment snapshot (§8). Today an opaque id; future tasks add typed fields.
    pub enrichment: &'ctx EnrichmentSnapshot,
    /// Wall-clock view for `time.*` predicates. Passed in (not read from the
    /// global clock) so determinism (§13.1) holds for replay.
    pub now: ClockSnapshot,
}

/// Wall-clock snapshot used by `time.*` predicates.
///
/// `recovery_mode` here mirrors `subject.recovery_mode`; per §9.2 the `time.*`
/// namespace exposes the same recovery posture but through the temporal lens so a
/// rule can read either form depending on what reads cleaner. `weekday` is 1..=7
/// (Mon=1, Sun=7) per ISO 8601; `hour_utc` is 0..=23.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClockSnapshot {
    /// `time.recovery_mode` — same value as `subject.recovery_mode` per §9.2.
    pub recovery_mode: bool,
    /// `time.weekday` — ISO 8601: Mon=1 .. Sun=7.
    pub weekday: u8,
    /// `time.hour_utc` — 0..=23.
    pub hour_utc: u8,
}

/// Runtime evaluation failure.
///
/// Distinct from [`crate::error::PolicyError::ConditionEval`]: this enum is the
/// rich form the evaluator raises locally; [`PolicyError`] wraps the
/// human-readable rendering when the failure bubbles into the pipeline.
///
/// [`PolicyError`]: crate::error::PolicyError
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ConditionEvalError {
    /// Predicate operator does not accept the supplied value type, OR the field's
    /// own type does not match the value type.
    #[error("type mismatch evaluating {field}: operator `{op}` expects `{expected_value_type}`, got `{actual_value_type}`")]
    TypeMismatch {
        /// Dotted field, e.g. `subject.session_class`.
        field: String,
        /// Operator that surfaced the mismatch.
        op: String,
        /// What the predicate's field/op pairing required.
        expected_value_type: &'static str,
        /// What the bundle provided.
        actual_value_type: &'static str,
    },
    /// The bundle compares a field with operators that are not in its allowed set
    /// per §9.2 / §26 / §27 / §28 / §29 (e.g. `<` against a bool field).
    #[error("unsupported operator for {field}: operator `{op}` is not allowed on this field")]
    UnsupportedOperator {
        /// Dotted field, e.g. `subject.recovery_mode`.
        field: String,
        /// Operator that the bundle attempted.
        op: String,
    },
    /// The `in [...]` value list mixed value types (e.g. a string and an int) —
    /// rejected because the §9 vocabulary assigns one type per field.
    #[error("heterogeneous value list for {field}: `in` requires every value to share a type")]
    HeterogeneousValueList {
        /// Dotted field.
        field: String,
    },
}

/// Evaluate a parsed [`Condition`] against an [`EvalContext`].
///
/// Returns `Ok(true)` when every conjunct holds, `Ok(false)` when at least one
/// conjunct is falsified by the context. Returns `Err(_)` on a type-system
/// violation in the bundle (see [`ConditionEvalError`]). An empty condition (zero
/// predicates) evaluates to `true`.
///
/// # Errors
///
/// Returns [`ConditionEvalError`] if any predicate carries an operator/value/field
/// type combination the §9 vocabulary forbids. Evaluation short-circuits on the
/// first error.
pub fn evaluate(cond: &Condition, ctx: &EvalContext<'_>) -> Result<bool, ConditionEvalError> {
    for predicate in &cond.predicates {
        if !evaluate_predicate(predicate, ctx)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn evaluate_predicate(
    predicate: &Predicate,
    ctx: &EvalContext<'_>,
) -> Result<bool, ConditionEvalError> {
    match predicate {
        Predicate::Compare { field, op, rhs } => eval_compare(field, *op, rhs, ctx),
        Predicate::In { field, values } => eval_in(field, values, ctx),
        Predicate::Contains { field, needle } => eval_contains(field, needle, ctx),
        Predicate::Exists { field } => Ok(eval_exists(field, ctx)),
    }
}

/// Typed lens onto a closed field's runtime value.
#[derive(Debug, Clone)]
enum FieldValue {
    Str(String),
    StrList(Vec<String>),
    Bool(bool),
    Int(i64),
    /// The field is structurally present but unset (e.g. `request.environment` is
    /// optional). `Exists` is false; every other predicate raises an error.
    Absent,
}

impl FieldValue {
    const fn type_name(&self) -> &'static str {
        match self {
            Self::Str(_) => "string",
            Self::StrList(_) => "string-list",
            Self::Bool(_) => "bool",
            Self::Int(_) => "int",
            Self::Absent => "absent",
        }
    }
}

#[allow(
    clippy::match_same_arms,
    reason = "per-namespace Absent arms are kept separate so each carries its own \
              T-021/T-022 follow-up comment; merging them would erase the per-field \
              provenance that the next pipeline tasks rely on for grep-ability"
)]
fn resolve_field_value(field: &ClosedField, ctx: &EvalContext<'_>) -> FieldValue {
    match field {
        // ---- subject ----
        ClosedField::SubjectCanonicalSubjectId => {
            FieldValue::Str(ctx.subject.canonical_subject_id.clone())
        }
        ClosedField::SubjectSubjectType => {
            FieldValue::Str(subject_type_token(ctx.subject.subject_type).to_owned())
        }
        ClosedField::SubjectGroups => FieldValue::StrList(ctx.subject.groups.clone()),
        ClosedField::SubjectCapabilities => FieldValue::StrList(ctx.subject.capabilities.clone()),
        ClosedField::SubjectSessionClass => FieldValue::Str(ctx.subject.session_class.clone()),
        ClosedField::SubjectRecoveryMode => FieldValue::Bool(ctx.subject.recovery_mode),
        ClosedField::SubjectIsAi => FieldValue::Bool(ctx.subject.is_ai),
        // Wave 4+ subject fields not yet on HydratedSubject — surface as Absent so
        // `Exists` is false (matches §28.2.2 / §26.5 "field defaults to false for
        // subjects that never set it"). T-021 expands HydratedSubject with the
        // typed fields and this branch flips to real lookups.
        ClosedField::SubjectPrimaryGroupId
        | ClosedField::SubjectIsFirstBoot
        | ClosedField::SubjectNetworkOutboundDirective
        | ClosedField::SubjectAiExternalPosture => FieldValue::Absent,

        // ---- request ----
        ClosedField::RequestAction => FieldValue::Str(ctx.envelope.request.action.clone()),
        ClosedField::RequestDryRun => {
            FieldValue::Str(dry_run_token(ctx.envelope.request.dry_run).to_owned())
        }
        // `environment` / `sandbox_profile_id` are not yet typed on `Request`,
        // and the risk template (T-022) is not yet hoisted from the adapter
        // manifest. All five surface as Absent today; bundle authors can guard
        // with `exists`.
        ClosedField::RequestEnvironment
        | ClosedField::RequestSandboxProfileId
        | ClosedField::RequestRiskDestructive
        | ClosedField::RequestRiskPrivileged
        | ClosedField::RequestRiskNetworkExposure
        | ClosedField::RequestRiskSecretAccess
        | ClosedField::RequestRiskRecoveryPathAffected => FieldValue::Absent,

        // ---- target ----
        // The entire target namespace is enriched by the adapter manifest reader
        // (T-022). Until then everything surfaces as Absent.
        ClosedField::TargetScope
        | ClosedField::TargetGroupId
        | ClosedField::TargetUserId
        | ClosedField::TargetReservedName
        | ClosedField::TargetIsConstitutionalSubstrate
        | ClosedField::TargetSurfaceKind
        | ClosedField::TargetCompositionZone
        | ClosedField::TargetGpuCapabilityClass
        | ClosedField::TargetGpuDeviceKind
        | ClosedField::TargetThemeKind
        | ClosedField::TargetThemeId
        | ClosedField::TargetExposureClass
        | ClosedField::TargetDeviceClass
        | ClosedField::TargetDeviceTrustClass
        | ClosedField::TargetRemovable
        | ClosedField::TargetDriverProvenance
        | ClosedField::TargetFirmwareTrusted
        | ClosedField::TargetAdapterDeclared(_) => {
            resolve_target_from_request(field, &ctx.envelope.request.target)
        }

        // ---- object / enrichment ----
        ClosedField::ObjectPrivacyClass
        | ClosedField::ObjectPolicyTags
        | ClosedField::ObjectKind
        | ClosedField::ObjectLifecycleState
        | ClosedField::ObjectCreatedBy => {
            // EnrichmentSnapshot is still a stub (T-022 expands it). Always Absent
            // today. Note we touch the snapshot id so the borrow is realised — this
            // keeps the lifetime requirement on EvalContext visible.
            let _ = &ctx.enrichment.snapshot_id;
            FieldValue::Absent
        }

        // ---- time ----
        ClosedField::TimeRecoveryMode => FieldValue::Bool(ctx.now.recovery_mode),
        ClosedField::TimeWeekday => FieldValue::Int(i64::from(ctx.now.weekday)),
        ClosedField::TimeHourUtc => FieldValue::Int(i64::from(ctx.now.hour_utc)),

        // ---- system ----
        ClosedField::SystemHostId
        | ClosedField::SystemClusterId
        | ClosedField::SystemReleaseChannel => FieldValue::Absent,
    }
}

/// Look up an adapter-declared target sub-field on the `serde_json::Value` target
/// payload. The conditions DSL only models scalar and string-list typed fields, so
/// we coerce a top-level JSON `bool`/`string`/`number`/`array<string>` into the
/// matching [`FieldValue`] variant; everything else is `Absent` (effectively
/// failing every comparison predicate but passing `exists` as false).
fn resolve_target_from_request(field: &ClosedField, target: &serde_json::Value) -> FieldValue {
    let key = match field {
        ClosedField::TargetScope => "scope",
        ClosedField::TargetGroupId => "group_id",
        ClosedField::TargetUserId => "user_id",
        ClosedField::TargetReservedName => "reserved_name",
        ClosedField::TargetIsConstitutionalSubstrate => "is_constitutional_substrate",
        ClosedField::TargetSurfaceKind => "surface_kind",
        ClosedField::TargetCompositionZone => "composition_zone",
        ClosedField::TargetGpuCapabilityClass => "gpu_capability_class",
        ClosedField::TargetGpuDeviceKind => "gpu_device_kind",
        ClosedField::TargetThemeKind => "theme_kind",
        ClosedField::TargetThemeId => "theme_id",
        ClosedField::TargetExposureClass => "exposure_class",
        ClosedField::TargetDeviceClass => "device_class",
        ClosedField::TargetDeviceTrustClass => "device_trust_class",
        ClosedField::TargetRemovable => "removable",
        ClosedField::TargetDriverProvenance => "driver_provenance",
        ClosedField::TargetFirmwareTrusted => "firmware_trusted",
        ClosedField::TargetAdapterDeclared(sub) => sub.as_str(),
        _ => return FieldValue::Absent,
    };
    match target.get(key) {
        Some(serde_json::Value::String(s)) => FieldValue::Str(s.clone()),
        Some(serde_json::Value::Bool(b)) => FieldValue::Bool(*b),
        Some(serde_json::Value::Number(n)) => {
            n.as_i64().map_or(FieldValue::Absent, FieldValue::Int)
        }
        Some(serde_json::Value::Array(items)) => {
            let mut strs = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    serde_json::Value::String(s) => strs.push(s.clone()),
                    _ => return FieldValue::Absent,
                }
            }
            FieldValue::StrList(strs)
        }
        Some(_) | None => FieldValue::Absent,
    }
}

const fn subject_type_token(st: SubjectType) -> &'static str {
    match st {
        SubjectType::Human => "human",
        SubjectType::Agent => "agent",
        SubjectType::Application => "application",
        SubjectType::Service => "service",
        SubjectType::Device => "device",
        SubjectType::Workflow => "workflow",
        SubjectType::RemoteOperator => "remote_operator",
    }
}

const fn dry_run_token(d: aios_action::DryRunMode) -> &'static str {
    match d {
        aios_action::DryRunMode::Live => "LIVE",
        aios_action::DryRunMode::Validate => "VALIDATE",
        aios_action::DryRunMode::Simulate => "SIMULATE",
    }
}

fn eval_compare(
    field: &ClosedField,
    op: CompareOp,
    rhs: &Value,
    ctx: &EvalContext<'_>,
) -> Result<bool, ConditionEvalError> {
    let actual = resolve_field_value(field, ctx);

    match (&actual, rhs) {
        (FieldValue::Bool(a), Value::Bool(b)) => match op {
            CompareOp::Eq => Ok(a == b),
            CompareOp::Neq => Ok(a != b),
            CompareOp::Lt | CompareOp::Lte | CompareOp::Gt | CompareOp::Gte => {
                Err(ConditionEvalError::UnsupportedOperator {
                    field: field.as_dotted(),
                    op: op.as_str().to_owned(),
                })
            }
        },
        (FieldValue::Int(a), Value::Int(b)) => Ok(compare_ord(*a, *b, op)),
        (FieldValue::Str(a), Value::String(b) | Value::Identifier(b)) => match op {
            CompareOp::Eq => Ok(a == b),
            CompareOp::Neq => Ok(a != b),
            CompareOp::Lt => Ok(a.as_str() < b.as_str()),
            CompareOp::Lte => Ok(a.as_str() <= b.as_str()),
            CompareOp::Gt => Ok(a.as_str() > b.as_str()),
            CompareOp::Gte => Ok(a.as_str() >= b.as_str()),
        },
        (FieldValue::Str(a), Value::Timestamp(b)) => match op {
            CompareOp::Eq => Ok(a == b),
            CompareOp::Neq => Ok(a != b),
            _ => Err(ConditionEvalError::TypeMismatch {
                field: field.as_dotted(),
                op: op.as_str().to_owned(),
                expected_value_type: "string",
                actual_value_type: rhs.type_name(),
            }),
        },
        (FieldValue::Absent, _) => Ok(false),
        (actual, rhs) => Err(ConditionEvalError::TypeMismatch {
            field: field.as_dotted(),
            op: op.as_str().to_owned(),
            expected_value_type: actual.type_name(),
            actual_value_type: rhs.type_name(),
        }),
    }
}

const fn compare_ord(a: i64, b: i64, op: CompareOp) -> bool {
    match op {
        CompareOp::Eq => a == b,
        CompareOp::Neq => a != b,
        CompareOp::Lt => a < b,
        CompareOp::Lte => a <= b,
        CompareOp::Gt => a > b,
        CompareOp::Gte => a >= b,
    }
}

fn eval_in(
    field: &ClosedField,
    values: &[Value],
    ctx: &EvalContext<'_>,
) -> Result<bool, ConditionEvalError> {
    // Ensure homogeneity of the value list — required by §9 closed-type discipline.
    if let Some(first) = values.first() {
        for v in &values[1..] {
            if std::mem::discriminant(first) != std::mem::discriminant(v) {
                return Err(ConditionEvalError::HeterogeneousValueList {
                    field: field.as_dotted(),
                });
            }
        }
    }

    let actual = resolve_field_value(field, ctx);
    match (&actual, values.first()) {
        // Absent field or defensive empty list — both yield false. The parser
        // already rejects empty `in []`, but the explicit guard keeps the
        // evaluator safe against AST handles built programmatically.
        (FieldValue::Absent, _) | (_, None) => Ok(false),
        (FieldValue::Bool(a), Some(Value::Bool(_))) => {
            Ok(values.iter().any(|v| matches!(v, Value::Bool(b) if b == a)))
        }
        (FieldValue::Int(a), Some(Value::Int(_))) => {
            Ok(values.iter().any(|v| matches!(v, Value::Int(b) if b == a)))
        }
        (FieldValue::Str(a), Some(Value::String(_) | Value::Identifier(_))) => Ok(values
            .iter()
            .any(|v| matches!(v, Value::String(b) | Value::Identifier(b) if b == a))),
        (FieldValue::StrList(list), Some(Value::String(_) | Value::Identifier(_))) => {
            // For list-typed fields `in [a, b]` means: does the field contain any
            // of the listed values? This mirrors the §9.1 `contains` semantics
            // extended to many needles.
            Ok(values.iter().any(|v| match v {
                Value::String(b) | Value::Identifier(b) => list.iter().any(|item| item == b),
                _ => false,
            }))
        }
        (actual, Some(rhs)) => Err(ConditionEvalError::TypeMismatch {
            field: field.as_dotted(),
            op: "in".to_owned(),
            expected_value_type: actual.type_name(),
            actual_value_type: rhs.type_name(),
        }),
    }
}

fn eval_contains(
    field: &ClosedField,
    needle: &str,
    ctx: &EvalContext<'_>,
) -> Result<bool, ConditionEvalError> {
    let actual = resolve_field_value(field, ctx);
    match actual {
        FieldValue::Absent => Ok(false),
        FieldValue::Str(haystack) => Ok(haystack.contains(needle)),
        FieldValue::StrList(list) => Ok(list.iter().any(|item| item == needle)),
        other => Err(ConditionEvalError::TypeMismatch {
            field: field.as_dotted(),
            op: "contains".to_owned(),
            expected_value_type: "string-or-string-list",
            actual_value_type: other.type_name(),
        }),
    }
}

fn eval_exists(field: &ClosedField, ctx: &EvalContext<'_>) -> bool {
    match resolve_field_value(field, ctx) {
        FieldValue::Absent => false,
        FieldValue::Str(s) => !s.is_empty(),
        FieldValue::StrList(l) => !l.is_empty(),
        FieldValue::Bool(_) | FieldValue::Int(_) => true,
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::conditions::Predicate;
    use aios_action::{Identity, Request, Trace};

    fn make_subject(is_ai: bool, recovery_mode: bool) -> HydratedSubject {
        HydratedSubject {
            canonical_subject_id: "human:lucky".to_owned(),
            subject_type: if is_ai {
                SubjectType::Agent
            } else {
                SubjectType::Human
            },
            groups: vec!["operators".to_owned()],
            capabilities: vec!["service.restart".to_owned()],
            session_class: "INTERNAL".to_owned(),
            recovery_mode,
            is_ai,
        }
    }

    fn make_envelope(action: &str, target: serde_json::Value) -> ActionEnvelope {
        ActionEnvelope::new(
            Identity::new("human:lucky", false),
            Request::new(action, target),
            Trace::new("00000000000000000000000000000001", "0000000000000001", None),
        )
    }

    fn make_enrichment() -> EnrichmentSnapshot {
        EnrichmentSnapshot {
            snapshot_id: "polb_snap_test".to_owned(),
            object: crate::snapshot::ObjectEnrichment::default(),
            adapter: crate::snapshot::AdapterEnrichment::default(),
        }
    }

    #[test]
    fn evaluate_empty_condition_is_true() {
        let subj = make_subject(false, false);
        let env = make_envelope("service.restart", serde_json::json!({}));
        let enr = make_enrichment();
        let ctx = EvalContext {
            subject: &subj,
            envelope: &env,
            enrichment: &enr,
            now: ClockSnapshot::default(),
        };
        let result = evaluate(&Condition::empty(), &ctx).unwrap_or(false);
        assert!(result, "empty conjunction is the true identity");
    }

    #[test]
    fn evaluate_subject_recovery_mode_bool_eq() {
        let subj = make_subject(false, true);
        let env = make_envelope("service.restart", serde_json::json!({}));
        let enr = make_enrichment();
        let ctx = EvalContext {
            subject: &subj,
            envelope: &env,
            enrichment: &enr,
            now: ClockSnapshot::default(),
        };
        let cond = Condition::conjunction(vec![Predicate::Compare {
            field: ClosedField::SubjectRecoveryMode,
            op: CompareOp::Eq,
            rhs: Value::Bool(true),
        }]);
        let result = evaluate(&cond, &ctx).unwrap_or(false);
        assert!(result);
    }

    #[test]
    fn evaluate_lt_on_bool_returns_unsupported_operator_error() {
        let subj = make_subject(false, true);
        let env = make_envelope("service.restart", serde_json::json!({}));
        let enr = make_enrichment();
        let ctx = EvalContext {
            subject: &subj,
            envelope: &env,
            enrichment: &enr,
            now: ClockSnapshot::default(),
        };
        let cond = Condition::conjunction(vec![Predicate::Compare {
            field: ClosedField::SubjectRecoveryMode,
            op: CompareOp::Lt,
            rhs: Value::Bool(true),
        }]);
        let result = evaluate(&cond, &ctx);
        assert!(matches!(
            result,
            Err(ConditionEvalError::UnsupportedOperator { .. })
        ));
    }
}
