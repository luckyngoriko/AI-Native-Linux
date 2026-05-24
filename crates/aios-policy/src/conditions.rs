//! Conditions vocabulary — the typed AST for the §9 condition DSL (T-019).
//!
//! Per S2.3 §9.1 the DSL is **AND-only conjunction of predicates** — no `or`,
//! no `not`, no parentheses, closed namespaces. The EBNF is:
//!
//! ```ebnf
//! condition  = predicate ( "and" predicate )* ;
//! predicate  = field op value
//!            | field "in"       "[" value ( "," value )* "]"
//!            | field "contains" string_literal
//!            | field "exists"
//!            | "time" "." "recovery_mode"        ;          // boolean predicate
//! field      = namespace "." identifier ( "." identifier )* ;
//! namespace  = "subject" | "request" | "target" | "object" | "time" | "system" ;
//! op         = "=" | "!=" | "<" | "<=" | ">" | ">=" ;
//! ```
//!
//! ## Closed vocabulary
//!
//! Every field that appears in a bundle MUST resolve to one of the closed entries
//! enumerated in [`ClosedField`] — except for the `target` namespace, where adapter
//! manifests may declare their own typed target fields (e.g. `target.service`,
//! `target.url`). Per §9.2 those adapter-declared fields are "schema-validated by
//! adapter"; this crate represents them with the [`ClosedField::TargetAdapterDeclared`]
//! variant so the parser does not reject otherwise-valid bundles, while still keeping
//! the well-known target fields (`target.scope`, `target.device_class`, …) on their
//! own typed variants.
//!
//! Total closed field count = 32 (per §29.1 "12 base + 5 namespace + 6 Wave 5 + 3 Wave
//! 6 + 1 Wave 9 §26.6 + 5 Wave 17 = 32"), plus the adapter-declared escape hatch.
//!
//! ## Deviation from the brief
//!
//! The T-019 brief asks for `And`, `Or`, `Not` AST variants and parenthesized
//! grouping. The §9.1 EBNF explicitly excludes `or` and parens
//! ("same restrictions as S2.1 query DSL: `and` only (no `or`), no parentheses,
//! closed namespaces"). **TRUST THE SPEC**: this AST is conjunction-only, and the
//! parser refuses `or` / `not` / `(` / `)` with an explicit error. If the policy
//! authors ever need disjunction, the §9 EBNF needs amending first.

use serde::{Deserialize, Serialize};

/// A parsed `condition` — a conjunction of predicates per §9.1.
///
/// The AND-only restriction is baked into the type: there is no `Or` variant.
/// An empty condition (zero predicates) is allowed at the type level and evaluates
/// to `true`; the parser never produces one (every condition source string contains
/// at least one predicate), but evaluators that compose conditions programmatically
/// may construct `Condition::empty()`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Condition {
    /// Conjuncts — all must be true for the whole condition to be true.
    pub predicates: Vec<Predicate>,
}

impl Condition {
    /// Construct an empty condition that always evaluates to `true`.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            predicates: Vec::new(),
        }
    }

    /// Construct a condition with the supplied predicates joined by AND.
    #[must_use]
    pub const fn conjunction(predicates: Vec<Predicate>) -> Self {
        Self { predicates }
    }
}

impl Default for Condition {
    fn default() -> Self {
        Self::empty()
    }
}

/// One predicate in a §9 condition.
///
/// Models every shape the §9.1 EBNF allows:
///
/// - `field op value`               → [`Predicate::Compare`]
/// - `field "in"  "[" value, … "]"` → [`Predicate::In`]
/// - `field "contains" string`      → [`Predicate::Contains`]
/// - `field "exists"`               → [`Predicate::Exists`]
/// - `time.recovery_mode`           → modelled as
///   `Compare { field: Time(RecoveryMode), op: Eq, rhs: Value::Bool(true) }` by the
///   parser (the bare boolean sugar) and as the same shape when written explicitly
///   with `=`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Predicate {
    /// `field op value` per §9.1.
    Compare {
        /// Closed field reference.
        field: ClosedField,
        /// Comparison operator.
        op: CompareOp,
        /// Right-hand side literal.
        rhs: Value,
    },
    /// `field "in" "[" value (, value)* "]"` per §9.1.
    ///
    /// The set MUST contain at least one value; the parser rejects `IN []`.
    In {
        /// Closed field reference.
        field: ClosedField,
        /// Value set; all values must share the same `Value` variant as each other.
        values: Vec<Value>,
    },
    /// `field "contains" string_literal` per §9.1.
    ///
    /// The `contains` operator is defined on string-typed and list-of-string-typed
    /// fields: against a string it is substring containment, against a list it is
    /// element membership.
    Contains {
        /// Closed field reference.
        field: ClosedField,
        /// String literal needle.
        needle: String,
    },
    /// `field "exists"` per §9.1.
    ///
    /// True iff the enriched value is non-null (and, for collections, non-empty).
    Exists {
        /// Closed field reference.
        field: ClosedField,
    },
}

/// `=`, `!=`, `<`, `<=`, `>`, `>=` — the six comparison operators from §9.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CompareOp {
    /// `=`
    Eq,
    /// `!=`
    Neq,
    /// `<`
    Lt,
    /// `<=`
    Lte,
    /// `>`
    Gt,
    /// `>=`
    Gte,
}

impl CompareOp {
    /// Canonical text symbol for the operator (matches §9.1 EBNF tokens).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "=",
            Self::Neq => "!=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Gt => ">",
            Self::Gte => ">=",
        }
    }
}

/// The six closed namespaces from §9.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Namespace {
    /// `subject` — hydrated subject fields (§9.2 + §26.1 + §28.1).
    Subject,
    /// `request` — caller-supplied action / risk / sandbox fields (§9.2).
    Request,
    /// `target` — adapter-declared OR Wave-touched typed target fields (§9.2 + §26.1
    /// + §26.6 + §27.1 + §28.1 + §29.1).
    Target,
    /// `object` — enrichment-fetched object metadata (§9.2).
    Object,
    /// `time` — clock + recovery-mode posture (§9.2).
    Time,
    /// `system` — host/cluster identity (§9.2).
    System,
}

impl Namespace {
    /// Canonical lowercase namespace token (matches §9.1).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Subject => "subject",
            Self::Request => "request",
            Self::Target => "target",
            Self::Object => "object",
            Self::Time => "time",
            Self::System => "system",
        }
    }

    /// Parse a namespace token; case-sensitive per §9.1.
    #[must_use]
    pub fn from_token(s: &str) -> Option<Self> {
        match s {
            "subject" => Some(Self::Subject),
            "request" => Some(Self::Request),
            "target" => Some(Self::Target),
            "object" => Some(Self::Object),
            "time" => Some(Self::Time),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

/// Closed field vocabulary.
///
/// Every variant is one of the 32 registered closed fields per §9.2 / §26.1 / §26.6 /
/// §27.1 / §28.1 / §29.1, plus the [`ClosedField::TargetAdapterDeclared`] escape hatch
/// for adapter-declared target fields (§9.2 — "adapter-declared target fields"). The
/// adapter-declared variant carries the raw sub-path string (e.g. `"service"` for
/// `target.service`) so adapter manifests can validate it downstream; the parser does
/// not introspect it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClosedField {
    // ---- subject namespace (§9.2 base + §26.1 namespace touch-up + §28.1 Wave 6) ----
    /// `subject.canonical_subject_id`
    SubjectCanonicalSubjectId,
    /// `subject.subject_type`
    SubjectSubjectType,
    /// `subject.groups`
    SubjectGroups,
    /// `subject.capabilities`
    SubjectCapabilities,
    /// `subject.session_class`
    SubjectSessionClass,
    /// `subject.recovery_mode`
    SubjectRecoveryMode,
    /// `subject.is_ai`
    SubjectIsAi,
    /// `subject.primary_group_id` (§26.1 Wave 4)
    SubjectPrimaryGroupId,
    /// `subject.is_first_boot` (§26.1 Wave 9)
    SubjectIsFirstBoot,
    /// `subject.network_outbound_directive` (§28.1 Wave 6)
    SubjectNetworkOutboundDirective,
    /// `subject.ai_external_posture` (§28.1 Wave 6)
    SubjectAiExternalPosture,

    // ---- request namespace (§9.2 base) ----
    /// `request.action`
    RequestAction,
    /// `request.environment`
    RequestEnvironment,
    /// `request.risk.destructive`
    RequestRiskDestructive,
    /// `request.risk.privileged`
    RequestRiskPrivileged,
    /// `request.risk.network_exposure`
    RequestRiskNetworkExposure,
    /// `request.risk.secret_access`
    RequestRiskSecretAccess,
    /// `request.risk.recovery_path_affected`
    RequestRiskRecoveryPathAffected,
    /// `request.dry_run`
    RequestDryRun,
    /// `request.sandbox_profile_id`
    RequestSandboxProfileId,

    // ---- target namespace (§26.1 namespace + §26.6 substrate + §27.1 Wave 5 +
    // §28.1 Wave 6 + §29.1 Wave 17) ----
    /// `target.scope` (§26.1 Wave 4)
    TargetScope,
    /// `target.group_id` (§26.1 Wave 4)
    TargetGroupId,
    /// `target.user_id` (§26.1 Wave 4)
    TargetUserId,
    /// `target.reserved_name` (§26.1 Wave 4)
    TargetReservedName,
    /// `target.is_constitutional_substrate` (§26.6 Wave 9)
    TargetIsConstitutionalSubstrate,
    /// `target.surface_kind` (§27.1 Wave 5)
    TargetSurfaceKind,
    /// `target.composition_zone` (§27.1 Wave 5)
    TargetCompositionZone,
    /// `target.gpu_capability_class` (§27.1 Wave 5)
    TargetGpuCapabilityClass,
    /// `target.gpu_device_kind` (§27.1 Wave 5)
    TargetGpuDeviceKind,
    /// `target.theme_kind` (§27.1 Wave 5)
    TargetThemeKind,
    /// `target.theme_id` (§27.1 Wave 5)
    TargetThemeId,
    /// `target.exposure_class` (§28.1 Wave 6)
    TargetExposureClass,
    /// `target.device_class` (§29.1 Wave 17)
    TargetDeviceClass,
    /// `target.device_trust_class` (§29.1 Wave 17)
    TargetDeviceTrustClass,
    /// `target.removable` (§29.1 Wave 17)
    TargetRemovable,
    /// `target.driver_provenance` (§29.1 Wave 17)
    TargetDriverProvenance,
    /// `target.firmware_trusted` (§29.1 Wave 17)
    TargetFirmwareTrusted,
    /// `target.<adapter-declared>` — the §9.2 escape hatch. Sub-path verbatim.
    TargetAdapterDeclared(String),

    // ---- object namespace (§9.2 base) ----
    /// `object.privacy_class`
    ObjectPrivacyClass,
    /// `object.policy_tags`
    ObjectPolicyTags,
    /// `object.kind`
    ObjectKind,
    /// `object.lifecycle_state`
    ObjectLifecycleState,
    /// `object.created_by`
    ObjectCreatedBy,

    // ---- time namespace (§9.2 base) ----
    /// `time.recovery_mode`
    TimeRecoveryMode,
    /// `time.weekday`
    TimeWeekday,
    /// `time.hour_utc`
    TimeHourUtc,

    // ---- system namespace (§9.2 base) ----
    /// `system.host_id`
    SystemHostId,
    /// `system.cluster_id`
    SystemClusterId,
    /// `system.release_channel`
    SystemReleaseChannel,
}

impl ClosedField {
    /// Owning namespace for this field.
    #[must_use]
    pub const fn namespace(&self) -> Namespace {
        match self {
            Self::SubjectCanonicalSubjectId
            | Self::SubjectSubjectType
            | Self::SubjectGroups
            | Self::SubjectCapabilities
            | Self::SubjectSessionClass
            | Self::SubjectRecoveryMode
            | Self::SubjectIsAi
            | Self::SubjectPrimaryGroupId
            | Self::SubjectIsFirstBoot
            | Self::SubjectNetworkOutboundDirective
            | Self::SubjectAiExternalPosture => Namespace::Subject,

            Self::RequestAction
            | Self::RequestEnvironment
            | Self::RequestRiskDestructive
            | Self::RequestRiskPrivileged
            | Self::RequestRiskNetworkExposure
            | Self::RequestRiskSecretAccess
            | Self::RequestRiskRecoveryPathAffected
            | Self::RequestDryRun
            | Self::RequestSandboxProfileId => Namespace::Request,

            Self::TargetScope
            | Self::TargetGroupId
            | Self::TargetUserId
            | Self::TargetReservedName
            | Self::TargetIsConstitutionalSubstrate
            | Self::TargetSurfaceKind
            | Self::TargetCompositionZone
            | Self::TargetGpuCapabilityClass
            | Self::TargetGpuDeviceKind
            | Self::TargetThemeKind
            | Self::TargetThemeId
            | Self::TargetExposureClass
            | Self::TargetDeviceClass
            | Self::TargetDeviceTrustClass
            | Self::TargetRemovable
            | Self::TargetDriverProvenance
            | Self::TargetFirmwareTrusted
            | Self::TargetAdapterDeclared(_) => Namespace::Target,

            Self::ObjectPrivacyClass
            | Self::ObjectPolicyTags
            | Self::ObjectKind
            | Self::ObjectLifecycleState
            | Self::ObjectCreatedBy => Namespace::Object,

            Self::TimeRecoveryMode | Self::TimeWeekday | Self::TimeHourUtc => Namespace::Time,

            Self::SystemHostId | Self::SystemClusterId | Self::SystemReleaseChannel => {
                Namespace::System
            }
        }
    }

    /// Resolve a `namespace.subpath` pair to a [`ClosedField`].
    ///
    /// Returns `None` for an unregistered (namespace, subpath) pair in every namespace
    /// **except** `target`: any unknown `target.<subpath>` is admitted as
    /// [`ClosedField::TargetAdapterDeclared`] because §9.2 explicitly leaves the
    /// target sub-vocabulary to adapter manifests. Adapter-side validation happens
    /// downstream (in the policy bundle loader, T-022, against the loaded adapter
    /// manifest registry).
    #[must_use]
    pub fn resolve(namespace: Namespace, subpath: &str) -> Option<Self> {
        match (namespace, subpath) {
            // subject
            (Namespace::Subject, "canonical_subject_id") => Some(Self::SubjectCanonicalSubjectId),
            (Namespace::Subject, "subject_type") => Some(Self::SubjectSubjectType),
            (Namespace::Subject, "groups") => Some(Self::SubjectGroups),
            (Namespace::Subject, "capabilities") => Some(Self::SubjectCapabilities),
            (Namespace::Subject, "session_class") => Some(Self::SubjectSessionClass),
            (Namespace::Subject, "recovery_mode") => Some(Self::SubjectRecoveryMode),
            (Namespace::Subject, "is_ai") => Some(Self::SubjectIsAi),
            (Namespace::Subject, "primary_group_id") => Some(Self::SubjectPrimaryGroupId),
            (Namespace::Subject, "is_first_boot") => Some(Self::SubjectIsFirstBoot),
            (Namespace::Subject, "network_outbound_directive") => {
                Some(Self::SubjectNetworkOutboundDirective)
            }
            (Namespace::Subject, "ai_external_posture") => Some(Self::SubjectAiExternalPosture),

            // request
            (Namespace::Request, "action") => Some(Self::RequestAction),
            (Namespace::Request, "environment") => Some(Self::RequestEnvironment),
            (Namespace::Request, "risk.destructive") => Some(Self::RequestRiskDestructive),
            (Namespace::Request, "risk.privileged") => Some(Self::RequestRiskPrivileged),
            (Namespace::Request, "risk.network_exposure") => Some(Self::RequestRiskNetworkExposure),
            (Namespace::Request, "risk.secret_access") => Some(Self::RequestRiskSecretAccess),
            (Namespace::Request, "risk.recovery_path_affected") => {
                Some(Self::RequestRiskRecoveryPathAffected)
            }
            (Namespace::Request, "dry_run") => Some(Self::RequestDryRun),
            (Namespace::Request, "sandbox_profile_id") => Some(Self::RequestSandboxProfileId),

            // target — typed entries first, then adapter-declared escape hatch.
            (Namespace::Target, "scope") => Some(Self::TargetScope),
            (Namespace::Target, "group_id") => Some(Self::TargetGroupId),
            (Namespace::Target, "user_id") => Some(Self::TargetUserId),
            (Namespace::Target, "reserved_name") => Some(Self::TargetReservedName),
            (Namespace::Target, "is_constitutional_substrate") => {
                Some(Self::TargetIsConstitutionalSubstrate)
            }
            (Namespace::Target, "surface_kind") => Some(Self::TargetSurfaceKind),
            (Namespace::Target, "composition_zone") => Some(Self::TargetCompositionZone),
            (Namespace::Target, "gpu_capability_class") => Some(Self::TargetGpuCapabilityClass),
            (Namespace::Target, "gpu_device_kind") => Some(Self::TargetGpuDeviceKind),
            (Namespace::Target, "theme_kind") => Some(Self::TargetThemeKind),
            (Namespace::Target, "theme_id") => Some(Self::TargetThemeId),
            (Namespace::Target, "exposure_class") => Some(Self::TargetExposureClass),
            (Namespace::Target, "device_class") => Some(Self::TargetDeviceClass),
            (Namespace::Target, "device_trust_class") => Some(Self::TargetDeviceTrustClass),
            (Namespace::Target, "removable") => Some(Self::TargetRemovable),
            (Namespace::Target, "driver_provenance") => Some(Self::TargetDriverProvenance),
            (Namespace::Target, "firmware_trusted") => Some(Self::TargetFirmwareTrusted),
            (Namespace::Target, other) => Some(Self::TargetAdapterDeclared(other.to_owned())),

            // object
            (Namespace::Object, "privacy_class") => Some(Self::ObjectPrivacyClass),
            (Namespace::Object, "policy_tags") => Some(Self::ObjectPolicyTags),
            (Namespace::Object, "kind") => Some(Self::ObjectKind),
            (Namespace::Object, "lifecycle_state") => Some(Self::ObjectLifecycleState),
            (Namespace::Object, "created_by") => Some(Self::ObjectCreatedBy),

            // time
            (Namespace::Time, "recovery_mode") => Some(Self::TimeRecoveryMode),
            (Namespace::Time, "weekday") => Some(Self::TimeWeekday),
            (Namespace::Time, "hour_utc") => Some(Self::TimeHourUtc),

            // system
            (Namespace::System, "host_id") => Some(Self::SystemHostId),
            (Namespace::System, "cluster_id") => Some(Self::SystemClusterId),
            (Namespace::System, "release_channel") => Some(Self::SystemReleaseChannel),

            // every other (namespace, subpath) pair outside `target` is rejected at
            // parse-time per §9 "Fields outside the vocabulary cause bundle-load
            // failure with InvalidPolicyBundle".
            _ => None,
        }
    }

    /// Canonical dotted form (e.g. `"subject.recovery_mode"`).
    #[must_use]
    pub fn as_dotted(&self) -> String {
        match self {
            Self::SubjectCanonicalSubjectId => "subject.canonical_subject_id".to_owned(),
            Self::SubjectSubjectType => "subject.subject_type".to_owned(),
            Self::SubjectGroups => "subject.groups".to_owned(),
            Self::SubjectCapabilities => "subject.capabilities".to_owned(),
            Self::SubjectSessionClass => "subject.session_class".to_owned(),
            Self::SubjectRecoveryMode => "subject.recovery_mode".to_owned(),
            Self::SubjectIsAi => "subject.is_ai".to_owned(),
            Self::SubjectPrimaryGroupId => "subject.primary_group_id".to_owned(),
            Self::SubjectIsFirstBoot => "subject.is_first_boot".to_owned(),
            Self::SubjectNetworkOutboundDirective => {
                "subject.network_outbound_directive".to_owned()
            }
            Self::SubjectAiExternalPosture => "subject.ai_external_posture".to_owned(),

            Self::RequestAction => "request.action".to_owned(),
            Self::RequestEnvironment => "request.environment".to_owned(),
            Self::RequestRiskDestructive => "request.risk.destructive".to_owned(),
            Self::RequestRiskPrivileged => "request.risk.privileged".to_owned(),
            Self::RequestRiskNetworkExposure => "request.risk.network_exposure".to_owned(),
            Self::RequestRiskSecretAccess => "request.risk.secret_access".to_owned(),
            Self::RequestRiskRecoveryPathAffected => {
                "request.risk.recovery_path_affected".to_owned()
            }
            Self::RequestDryRun => "request.dry_run".to_owned(),
            Self::RequestSandboxProfileId => "request.sandbox_profile_id".to_owned(),

            Self::TargetScope => "target.scope".to_owned(),
            Self::TargetGroupId => "target.group_id".to_owned(),
            Self::TargetUserId => "target.user_id".to_owned(),
            Self::TargetReservedName => "target.reserved_name".to_owned(),
            Self::TargetIsConstitutionalSubstrate => {
                "target.is_constitutional_substrate".to_owned()
            }
            Self::TargetSurfaceKind => "target.surface_kind".to_owned(),
            Self::TargetCompositionZone => "target.composition_zone".to_owned(),
            Self::TargetGpuCapabilityClass => "target.gpu_capability_class".to_owned(),
            Self::TargetGpuDeviceKind => "target.gpu_device_kind".to_owned(),
            Self::TargetThemeKind => "target.theme_kind".to_owned(),
            Self::TargetThemeId => "target.theme_id".to_owned(),
            Self::TargetExposureClass => "target.exposure_class".to_owned(),
            Self::TargetDeviceClass => "target.device_class".to_owned(),
            Self::TargetDeviceTrustClass => "target.device_trust_class".to_owned(),
            Self::TargetRemovable => "target.removable".to_owned(),
            Self::TargetDriverProvenance => "target.driver_provenance".to_owned(),
            Self::TargetFirmwareTrusted => "target.firmware_trusted".to_owned(),
            Self::TargetAdapterDeclared(sub) => format!("target.{sub}"),

            Self::ObjectPrivacyClass => "object.privacy_class".to_owned(),
            Self::ObjectPolicyTags => "object.policy_tags".to_owned(),
            Self::ObjectKind => "object.kind".to_owned(),
            Self::ObjectLifecycleState => "object.lifecycle_state".to_owned(),
            Self::ObjectCreatedBy => "object.created_by".to_owned(),

            Self::TimeRecoveryMode => "time.recovery_mode".to_owned(),
            Self::TimeWeekday => "time.weekday".to_owned(),
            Self::TimeHourUtc => "time.hour_utc".to_owned(),

            Self::SystemHostId => "system.host_id".to_owned(),
            Self::SystemClusterId => "system.cluster_id".to_owned(),
            Self::SystemReleaseChannel => "system.release_channel".to_owned(),
        }
    }
}

/// Typed literal value on the RHS of a predicate.
///
/// Per §9.1 `value = string_literal | number_literal | boolean_literal |
/// timestamp_literal | identifier_literal`. We collapse `number_literal` to `Int`
/// (the policy vocabulary never compares fractional numbers — `hour_utc` is integer,
/// risk scalars are booleans, group counts are unsigned). Floats are intentionally
/// not supported; the parser rejects literals that contain a `.` followed by digits.
///
/// `Identifier` carries an unquoted enum name like `LOCALHOST_ONLY` (used in §27.1 /
/// §28.1 / §29.1 closed-enum RHS literals). It and `String` are distinguished at the
/// lexer level so a misquoted enum (`subject.session_class = INTERNAL`) is treated
/// differently from a quoted string (`subject.session_class = "INTERNAL"`) by the
/// type checker downstream — see [`crate::conditions_eval`]. Both currently evaluate
/// identically against string-typed fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    /// `"foo"`-quoted string literal.
    String(String),
    /// Unquoted identifier literal — typically a closed-enum value.
    Identifier(String),
    /// Decimal integer literal. Negative values supported via leading `-`.
    Int(i64),
    /// `true` / `false` literal.
    Bool(bool),
    /// RFC 3339 timestamp literal — preserved as the original source string so the
    /// evaluator can parse with `chrono` without round-tripping through `DateTime`
    /// (which would erase nanosecond precision in some cases).
    Timestamp(String),
}

impl Value {
    /// Short identifier of this value's variant, for error messages.
    #[must_use]
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::String(_) => "string",
            Self::Identifier(_) => "identifier",
            Self::Int(_) => "int",
            Self::Bool(_) => "bool",
            Self::Timestamp(_) => "timestamp",
        }
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

    #[test]
    fn namespace_round_trip_through_token() {
        for ns in [
            Namespace::Subject,
            Namespace::Request,
            Namespace::Target,
            Namespace::Object,
            Namespace::Time,
            Namespace::System,
        ] {
            let token = ns.as_str();
            let parsed = Namespace::from_token(token);
            assert_eq!(parsed, Some(ns), "round-trip failed for namespace {ns:?}");
        }
    }

    #[test]
    fn namespace_from_token_rejects_unknown() {
        assert_eq!(Namespace::from_token("Subject"), None, "case-sensitive");
        assert_eq!(Namespace::from_token("env"), None);
        assert_eq!(Namespace::from_token(""), None);
    }

    #[test]
    fn closed_field_resolve_known_subject_field() {
        let f = ClosedField::resolve(Namespace::Subject, "recovery_mode");
        assert_eq!(f, Some(ClosedField::SubjectRecoveryMode));
    }

    #[test]
    fn closed_field_resolve_unknown_subject_field_returns_none() {
        // every namespace except `target` is fully closed.
        assert!(ClosedField::resolve(Namespace::Subject, "spoofed_field").is_none());
        assert!(ClosedField::resolve(Namespace::Object, "spoofed_field").is_none());
        assert!(ClosedField::resolve(Namespace::Time, "spoofed_field").is_none());
        assert!(ClosedField::resolve(Namespace::System, "spoofed_field").is_none());
        assert!(ClosedField::resolve(Namespace::Request, "spoofed_field").is_none());
    }

    #[test]
    fn closed_field_resolve_target_adapter_declared_passes_through() {
        let f = ClosedField::resolve(Namespace::Target, "service");
        assert_eq!(
            f,
            Some(ClosedField::TargetAdapterDeclared("service".to_owned()))
        );

        let f2 = ClosedField::resolve(Namespace::Target, "url");
        assert_eq!(
            f2,
            Some(ClosedField::TargetAdapterDeclared("url".to_owned()))
        );
    }

    #[test]
    fn closed_field_dotted_round_trip_via_resolve() {
        // Round-trip every typed variant: from dotted form, split on first '.',
        // resolve, then re-emit the dotted form and compare.
        let typed = [
            ClosedField::SubjectRecoveryMode,
            ClosedField::SubjectIsFirstBoot,
            ClosedField::SubjectNetworkOutboundDirective,
            ClosedField::RequestAction,
            ClosedField::RequestRiskDestructive,
            ClosedField::RequestRiskRecoveryPathAffected,
            ClosedField::TargetScope,
            ClosedField::TargetIsConstitutionalSubstrate,
            ClosedField::TargetDeviceClass,
            ClosedField::ObjectPrivacyClass,
            ClosedField::TimeRecoveryMode,
            ClosedField::SystemHostId,
        ];

        for f in typed {
            let dotted = f.as_dotted();
            let (ns_str, sub) = dotted
                .split_once('.')
                .expect("typed field has at least one dot");
            let ns = Namespace::from_token(ns_str).expect("typed field carries a real namespace");
            let resolved = ClosedField::resolve(ns, sub)
                .expect("typed field must resolve from its dotted form");
            assert_eq!(resolved, f, "round-trip mismatch for {dotted}");
        }
    }

    #[test]
    fn namespace_matches_field_owner() {
        assert_eq!(
            ClosedField::SubjectRecoveryMode.namespace(),
            Namespace::Subject
        );
        assert_eq!(
            ClosedField::TargetDeviceClass.namespace(),
            Namespace::Target
        );
        assert_eq!(
            ClosedField::TargetAdapterDeclared("service".to_owned()).namespace(),
            Namespace::Target
        );
        assert_eq!(ClosedField::TimeRecoveryMode.namespace(), Namespace::Time);
    }

    #[test]
    fn condition_default_is_empty_conjunction_true() {
        let c = Condition::default();
        assert!(c.predicates.is_empty());
    }

    #[test]
    fn compare_op_as_str_matches_ebnf_tokens() {
        assert_eq!(CompareOp::Eq.as_str(), "=");
        assert_eq!(CompareOp::Neq.as_str(), "!=");
        assert_eq!(CompareOp::Lt.as_str(), "<");
        assert_eq!(CompareOp::Lte.as_str(), "<=");
        assert_eq!(CompareOp::Gt.as_str(), ">");
        assert_eq!(CompareOp::Gte.as_str(), ">=");
    }
}
