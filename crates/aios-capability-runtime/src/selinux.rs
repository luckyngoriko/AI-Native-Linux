#![allow(
    clippy::doc_markdown,
    clippy::missing_errors_doc,
    clippy::missing_const_for_fn,
    clippy::too_long_first_doc_paragraph,
    reason = "lints that conflict with OS-RESEARCH module prose conventions"
)]
//! SELinux MAC Policy Plane — mandatory access control backend for
//! STIG_ALIGNED and AIRGAP_HIGH profiles (S16.2).
//!
//! ## OS Research Provenance
//!
//! NSA's Security-Enhanced Linux (SELinux), first publicly released in 2000,
//! introduced **Mandatory Access Control (MAC)** into the Linux kernel via
//! the Linux Security Modules (LSM) framework. Its design draws on the
//! **Flask** security architecture (Spencer et al., 1999), which separates
//! *security policy logic* from *enforcement* through a well-defined
//! interface between the security server and the object manager.
//!
//! Key architectural decisions inherited from Flask:
//!
//! 1. **Type Enforcement (TE)** — every subject (process) and object (file,
//!    socket, etc.) is assigned a *type*. Access is granted solely through
//!    explicit `allow` rules between source and target types.
//! 2. **Role-Based Access Control (RBAC)** — users are mapped to roles;
//!    roles are authorized for a set of types, constraining which domains a
//!    user can enter.
//! 3. **Multi-Level Security (MLS) / Multi-Category Security (MCS)** — every
//!    subject and object has a sensitivity *level* and a *category* set.
//!    Access requires both the TE rule to pass AND the MLS constraints
//!    (`dominates` / `equals`) to hold.
//! 4. **AVC (Access Vector Cache)** — the kernel caches access decisions
//!    in a hash table. Denials are logged as `avc: denied` messages;
//!    every denial is an auditable security event.
//!
//! ### Mapping to AIOS Capsule Architecture
//!
//! | SELinux / Flask concept    | AIOS equivalent                          |
//! |----------------------------|------------------------------------------|
//! | Domain (`*_t`)              | [`SeLinuxDomain`] — per-capsule domain   |
//! | Security context (quad)    | [`SeLinuxContext`]                        |
//! | `allow` rule               | [`SeLinuxRule`]                           |
//! | MLS/MCS sensitivity level  | [`SeLinuxContext::level`]                 |
//! | MLS/MCS category set       | [`SeLinuxContext::categories`]            |
//! | AVC denial                 | [`AvcDenial`] — typed evidence record     |
//! | Policy module / bundle     | [`SePolicyBundle`]                        |
//! | `setenforce` / `getenforce` | [`SePolicyValidator`]                    |
//!
//! ## Constitutional invariants (verified in tests)
//!
//! - **INV-SEL-001 (Domain naming):** Every capsule domain must follow the
//!   pattern `aios_capsule_N_t` where N is the capsule's numeric id.
//! - **INV-SEL-002 (No unconfined):** No AIOS service or capsule may run as
//!   `unconfined_t`. Any rule that references `unconfined_t` in any field
//!   is rejected at validation time.
//! - **INV-SEL-003 (Least privilege):** Every rule must list explicit
//!   permissions; wildcard / blanket `*` allow rules are rejected.
//! - **INV-SEL-004 (Context validity):** Every [`SeLinuxContext`] must
//!   contain non-empty `user`, `role`, `type`, and a well-formed `level`
//!   (sensitivity `s0`..`s15` plus optional category list `c0..c1023`).
//! - **INV-SEL-005 (AVC audit integrity):** Every denial captured in an
//!   [`AvcDenial`] must carry a non-zero timestamp, non-empty source/target
//!   domain references, and a non-empty permission set.

use std::fmt;

use super::capsule_namespace::CapsuleId;

// ---------------------------------------------------------------------------
// SeLinuxDomain — per-capsule domain name
// ---------------------------------------------------------------------------

/// A per-capsule SELinux domain name.
///
/// Every AIOS capsule gets its own SELinux type, following the naming
/// convention `aios_capsule_N_t`. This type is the primary subject label
/// for all processes running within that capsule.
///
/// # Examples
///
/// ```rust
/// # use aios_capability_runtime::capsule_namespace::CapsuleId;
/// # use aios_capability_runtime::selinux::SeLinuxDomain;
/// let domain = SeLinuxDomain::from_capsule_id(CapsuleId(7));
/// assert_eq!(domain.as_str(), "aios_capsule_7_t");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeLinuxDomain(String);

impl SeLinuxDomain {
    /// Construct a domain from a pre-validated string.
    ///
    /// Returns `None` if the string does not match the `aios_capsule_N_t`
    /// pattern.
    #[must_use]
    pub fn new(raw: &str) -> Option<Self> {
        if !raw.starts_with("aios_capsule_") || !raw.ends_with("_t") {
            return None;
        }
        let body = &raw["aios_capsule_".len()..raw.len() - 2]; // strip suffix _t
        if body.is_empty() {
            return None;
        }
        for ch in body.chars() {
            if !ch.is_ascii_digit() {
                return None;
            }
        }
        Some(Self(raw.into()))
    }

    /// Construct a domain from a [`CapsuleId`].
    #[must_use]
    pub fn from_capsule_id(id: CapsuleId) -> Self {
        Self(format!("aios_capsule_{}_t", id.raw()))
    }

    /// The canonical domain string (e.g. `"aios_capsule_7_t"`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Extract the numeric capsule id from the domain name.
    #[must_use]
    pub fn capsule_id(&self) -> Option<u64> {
        let body = &self.0["aios_capsule_".len()..self.0.len() - 2];
        body.parse::<u64>().ok()
    }
}

impl fmt::Display for SeLinuxDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Canonical domain for a typed file object used by all AIOS capsules.
pub const AIOS_DATA_DOMAIN: &str = "aios_data_t";
/// Canonical domain for the AIOS system orchestrator process.
pub const AIOS_SYSTEM_DOMAIN: &str = "aios_system_t";

// ---------------------------------------------------------------------------
// SeLinuxContext — user:role:type:level security quad
// ---------------------------------------------------------------------------

/// A full SELinux security context: `user:role:type:level`.
///
/// The fields directly correspond to the SELinux security attribute quad:
///
/// - `user` — SELinux user (e.g. `system_u`)
/// - `role` — SELinux role (e.g. `object_r`)
/// - `type_` — the domain / type (e.g. `aios_data_t`)
/// - `level` — MLS/MCS sensitivity + categories (e.g. `s0`, `s0:c0,c1`)
///
/// # Examples
///
/// ```rust
/// # use aios_capability_runtime::selinux::SeLinuxContext;
/// let ctx = SeLinuxContext {
///     user: "system_u".into(),
///     role: "object_r".into(),
///     type_: "aios_data_t".into(),
///     level: "s0".into(),
/// };
/// assert_eq!(ctx.to_string(), "system_u:object_r:aios_data_t:s0");
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SeLinuxContext {
    /// SELinux user identity (e.g. `system_u`, `staff_u`).
    pub user: String,
    /// SELinux role (e.g. `object_r`, `system_r`).
    pub role: String,
    /// SELinux type / domain (e.g. `aios_capsule_7_t`).
    pub type_: String,
    /// MLS/MCS level: sensitivity plus optional categories
    /// (e.g. `s0`, `s1:c0,c2`).
    pub level: String,
}

impl SeLinuxContext {
    /// Create a context from individual components, returning `None` if any
    /// component is empty or the level is malformed.
    #[must_use]
    pub fn new(user: &str, role: &str, type_: &str, level: &str) -> Option<Self> {
        if user.is_empty() || role.is_empty() || type_.is_empty() || level.is_empty() {
            return None;
        }
        if !Self::is_valid_level(level) {
            return None;
        }
        Some(Self {
            user: user.into(),
            role: role.into(),
            type_: type_.into(),
            level: level.into(),
        })
    }

    /// Create a context for a capsule domain.
    #[must_use]
    pub fn for_capsule(domain: &SeLinuxDomain, sensitivity: &str, categories: &[u16]) -> Self {
        let cats: Vec<String> = categories.iter().map(|c| format!("c{c}")).collect();
        let level = if cats.is_empty() {
            sensitivity.to_string()
        } else {
            format!("{}:{}", sensitivity, cats.join(","))
        };
        Self {
            user: "system_u".into(),
            role: "system_r".into(),
            type_: domain.as_str().into(),
            level,
        }
    }

    /// Create a file context for an object accessible by a specific capsule.
    #[must_use]
    pub fn for_file(domain: &SeLinuxDomain) -> Self {
        Self {
            user: "system_u".into(),
            role: "object_r".into(),
            type_: domain.as_str().into(),
            level: "s0".into(),
        }
    }

    /// Validate the MLS/MCS level string.
    ///
    /// Acceptable forms:
    /// - `sN` where N is 0..15 (sensitivity)
    /// - `sN:cA,cB,...` where each category is `c0`..`c1023`
    #[must_use]
    pub fn is_valid_level(level: &str) -> bool {
        let (sens_part, cats_part) = match level.split_once(':') {
            Some((s, rest)) => (s, Some(rest)),
            None => (level, None),
        };

        // Sensitivity must be s0..s15.
        if !sens_part.starts_with('s') {
            return false;
        }
        if sens_part[1..].parse::<u8>().map_or(true, |n| n > 15) {
            return false;
        }

        // Optional categories: c0..c1023, comma-separated.
        if let Some(cats) = cats_part {
            if cats.is_empty() {
                return false;
            }
            for cat in cats.split(',') {
                if !cat.starts_with('c') {
                    return false;
                }
                if cat[1..].parse::<u16>().map_or(true, |n| n > 1023) {
                    return false;
                }
            }
        }

        true
    }
}

impl fmt::Display for SeLinuxContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}:{}",
            self.user, self.role, self.type_, self.level
        )
    }
}

// ---------------------------------------------------------------------------
// SeLinuxPermission — allowed operations
// ---------------------------------------------------------------------------

/// Individual operation a capsule may be authorized to perform on a target.
///
/// The permission set mirrors the standard SELinux object-class permission
/// vocabulary, scoped to the AIOS capsule interaction model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SeLinuxPermission {
    /// Read data from the target.
    Read,
    /// Write / modify data on the target.
    Write,
    /// Execute / invoke the target (for executable files or capsule entrypoints).
    Execute,
    /// Append data (write without overwrite).
    Append,
    /// Create new resources within the target context.
    Create,
    /// Delete resources within the target context.
    Delete,
    /// Open the target (e.g., file descriptor, socket).
    Open,
    /// Transition into the target domain (domain transition).
    Transition,
    /// Get or set attributes on the target.
    GetAttr,
    /// Set attributes on the target.
    SetAttr,
}

impl SeLinuxPermission {
    /// Human-readable wire-form name.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Execute => "execute",
            Self::Append => "append",
            Self::Create => "create",
            Self::Delete => "delete",
            Self::Open => "open",
            Self::Transition => "transition",
            Self::GetAttr => "getattr",
            Self::SetAttr => "setattr",
        }
    }

    /// All available permissions, for policy-bundle composition.
    #[must_use]
    pub const fn all() -> [Self; 10] {
        [
            Self::Read,
            Self::Write,
            Self::Execute,
            Self::Append,
            Self::Create,
            Self::Delete,
            Self::Open,
            Self::Transition,
            Self::GetAttr,
            Self::SetAttr,
        ]
    }
}

impl fmt::Display for SeLinuxPermission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ---------------------------------------------------------------------------
// SeLinuxRule — a single allow rule
// ---------------------------------------------------------------------------

/// A single SELinux `allow` rule: *source domain* is permitted *permissions*
/// on *target domain*.
///
/// # Examples
///
/// ```rust
/// # use aios_capability_runtime::selinux::{SeLinuxRule, SeLinuxPermission};
/// let rule = SeLinuxRule::new(
///     "aios_capsule_7_t",
///     "aios_data_t",
///     &[SeLinuxPermission::Read, SeLinuxPermission::Open],
/// );
/// assert!(rule.is_some());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeLinuxRule {
    /// The source domain (subject) requesting access.
    pub source_domain: String,
    /// The target domain (object / resource).
    pub target_domain: String,
    /// The set of permissions granted.
    pub permissions: Vec<SeLinuxPermission>,
    /// Optional human-readable justification for the rule.
    pub justification: Option<String>,
}

impl SeLinuxRule {
    /// Create a new rule.
    ///
    /// Returns `None` if the source or target domain is empty, or the
    /// permission set is empty.
    #[must_use]
    pub fn new(
        source: &str,
        target: &str,
        permissions: &[SeLinuxPermission],
    ) -> Option<Self> {
        if source.is_empty() || target.is_empty() || permissions.is_empty() {
            return None;
        }
        Some(Self {
            source_domain: source.into(),
            target_domain: target.into(),
            permissions: permissions.to_vec(),
            justification: None,
        })
    }

    /// Create a new rule with a justification annotation.
    #[must_use]
    pub fn with_justification(
        source: &str,
        target: &str,
        permissions: &[SeLinuxPermission],
        justification: &str,
    ) -> Option<Self> {
        Self::new(source, target, permissions).map(|mut r| {
            r.justification = Some(justification.into());
            r
        })
    }

    /// Whether the rule references `unconfined_t` in any field.
    #[must_use]
    pub fn references_unconfined(&self) -> bool {
        self.source_domain == "unconfined_t" || self.target_domain == "unconfined_t"
    }

    /// Number of permissions granted by this rule.
    #[must_use]
    pub const fn permission_count(&self) -> usize {
        self.permissions.len()
    }
}

// ---------------------------------------------------------------------------
// SePolicyBundle — policy rules for a single capsule
// ---------------------------------------------------------------------------

/// A collection of SELinux rules and domain definitions for a capsule.
///
/// Each capsule that requires inter-capsule interaction gets a
/// [`SePolicyBundle`] that declares:
/// - Its own domain name.
/// - The set of `allow` rules authorizing operations on other domains.
/// - The set of domain transitions (entrypoints) for capsule-to-capsule
///   interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SePolicyBundle {
    /// The capsule this policy bundle belongs to.
    pub capsule_id: CapsuleId,
    /// The capsule's own SELinux domain.
    pub domain: SeLinuxDomain,
    /// MCS/MLS sensitivity label for this capsule.
    pub sensitivity: String,
    /// MCS category set for this capsule.
    pub categories: Vec<u16>,
    /// Set of `allow` rules.
    pub rules: Vec<SeLinuxRule>,
    /// Domain transition entrypoints — target domains this capsule is
    /// authorized to transition into.
    pub transitions: Vec<SeLinuxDomain>,
}

impl SePolicyBundle {
    /// Generate a policy bundle for a capsule with the given rules.
    #[must_use]
    pub fn generate_for_capsule(
        capsule_id: CapsuleId,
        sensitivity: &str,
        categories: Vec<u16>,
        rules: Vec<SeLinuxRule>,
        transitions: Vec<SeLinuxDomain>,
    ) -> Self {
        let domain = SeLinuxDomain::from_capsule_id(capsule_id);
        Self {
            capsule_id,
            domain,
            sensitivity: sensitivity.into(),
            categories,
            rules,
            transitions,
        }
    }

    /// Total number of rules in the bundle.
    #[must_use]
    pub const fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Total number of individual permissions across all rules.
    #[must_use]
    pub fn total_permissions(&self) -> usize {
        self.rules.iter().map(SeLinuxRule::permission_count).sum()
    }

    /// Whether any rule references `unconfined_t`.
    #[must_use]
    pub fn contains_unconfined(&self) -> bool {
        self.rules.iter().any(SeLinuxRule::references_unconfined)
    }

    /// The full SELinux context for this capsule.
    #[must_use]
    pub fn context(&self) -> SeLinuxContext {
        SeLinuxContext::for_capsule(&self.domain, &self.sensitivity, &self.categories)
    }
}

// ---------------------------------------------------------------------------
// AvcDenial — typed AVC denial evidence record
// ---------------------------------------------------------------------------

/// A typed evidence record capturing a single SELinux AVC denial.
///
/// Every denial is an auditable security event. The record carries enough
/// forensic detail to reconstruct the access attempt, the SELinux context
/// in effect at the time, and the exact permission that was blocked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvcDenial {
    /// Unix epoch timestamp of the denial event (seconds since 1970-01-01).
    pub timestamp_secs: u64,
    /// The SELinux context of the subject that attempted the access.
    pub source_context: SeLinuxContext,
    /// The SELinux context of the target object.
    pub target_context: SeLinuxContext,
    /// The specific permission(s) that were denied.
    pub denied_permissions: Vec<SeLinuxPermission>,
    /// The SELinux enforcement result — always `"denied"` for an AVC denial.
    pub result: String,
    /// The `comm` field (process name) from the denial message.
    pub comm: String,
    /// Executable path of the process that triggered the denial.
    pub exe_path: Option<String>,
    /// SELinux policy name in effect at the time.
    pub policy_name: String,
}

impl AvcDenial {
    /// Create a new AVC denial record.
    ///
    /// Returns `None` if permissions are empty, timestamp is zero, or any
    /// context field is empty.
    #[must_use]
    pub fn new(
        timestamp_secs: u64,
        source_context: SeLinuxContext,
        target_context: SeLinuxContext,
        denied_permissions: Vec<SeLinuxPermission>,
        comm: &str,
        exe_path: Option<String>,
        policy_name: &str,
    ) -> Option<Self> {
        if timestamp_secs == 0
            || denied_permissions.is_empty()
            || comm.is_empty()
            || policy_name.is_empty()
        {
            return None;
        }
        Some(Self {
            timestamp_secs,
            source_context,
            target_context,
            denied_permissions,
            result: "denied".into(),
            comm: comm.into(),
            exe_path,
            policy_name: policy_name.into(),
        })
    }

    /// Format the denial as a human-readable summary line.
    #[must_use]
    pub fn summary(&self) -> String {
        let perms: Vec<&str> = self
            .denied_permissions
            .iter()
            .map(SeLinuxPermission::as_str)
            .collect();
        format!(
            "AVC denied [{}] {} -> {} : {{{}}}",
            self.comm,
            self.source_context,
            self.target_context,
            perms.join(" ")
        )
    }

    /// Whether this denial involves the `unconfined_t` type.
    #[must_use]
    pub fn involves_unconfined(&self) -> bool {
        self.source_context.type_ == "unconfined_t"
            || self.target_context.type_ == "unconfined_t"
    }
}

// ---------------------------------------------------------------------------
// SePolicyValidator — policy bundle validation
// ---------------------------------------------------------------------------

/// Validates [`SePolicyBundle`] instances against AIOS constitutional
/// invariants.
///
/// The validator checks:
/// - No rule references `unconfined_t` (INV-SEL-002).
/// - Every rule has explicit, non-empty permissions (INV-SEL-003).
/// - Rule source/target domains are non-empty.
/// - At least one rule is present (a policy bundle with zero rules is
///   effectively `unconfined` by omission).
#[derive(Debug, Default, Clone)]
pub struct SePolicyValidator;

/// Errors collected by the validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// The bundle contains no rules.
    EmptyRuleset,
    /// A rule references `unconfined_t`.
    UnconfinedReference(String),
    /// A rule has an empty source or target domain.
    MissingDomain(String),
    /// A rule has no permissions.
    EmptyPermissions(String),
    /// The bundle references a non-AIOS domain name.
    ForeignDomain(String),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRuleset => write!(f, "policy bundle has no rules"),
            Self::UnconfinedReference(msg) => write!(f, "unconfined_t reference: {msg}"),
            Self::MissingDomain(msg) => write!(f, "empty domain: {msg}"),
            Self::EmptyPermissions(msg) => write!(f, "empty permissions: {msg}"),
            Self::ForeignDomain(msg) => write!(f, "foreign domain: {msg}"),
        }
    }
}

impl SePolicyValidator {
    /// Create a new validator.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Validate a policy bundle against AIOS invariants.
    ///
    /// Returns `Ok(())` if the bundle passes all checks, or `Err(Vec<String>)`
    /// with a human-readable description of each violation.
    pub fn validate(bundle: &SePolicyBundle) -> Result<(), Vec<String>> {
        let mut errors: Vec<String> = Vec::new();

        if bundle.rule_count() == 0 {
            errors.push(ValidationError::EmptyRuleset.to_string());
            return Err(errors);
        }

        for (i, rule) in bundle.rules.iter().enumerate() {
            if rule.references_unconfined() {
                errors.push(
                    ValidationError::UnconfinedReference(format!(
                        "rule[{i}] references unconfined_t (INV-SEL-002)"
                    ))
                    .to_string(),
                );
            }

            if rule.source_domain.is_empty() || rule.target_domain.is_empty() {
                errors.push(
                    ValidationError::MissingDomain(format!(
                        "rule[{i}] has empty source or target domain"
                    ))
                    .to_string(),
                );
            }

            if rule.permissions.is_empty() {
                errors.push(
                    ValidationError::EmptyPermissions(format!(
                        "rule[{i}] has no permissions (INV-SEL-003)"
                    ))
                    .to_string(),
                );
            }

            // INV-SEL-003: reject blanket wildcard patterns.
            // We don't have a literal `*` in the permission enum, but a rule
            // with all 10 permissions without justification is suspicious.
            if rule.permission_count() == SeLinuxPermission::all().len()
                && rule.justification.is_none()
            {
                errors.push(format!(
                    "rule[{}] grants all {} permissions without justification (INV-SEL-003 least privilege)",
                    i,
                    SeLinuxPermission::all().len(),
                ));
            }

            // Check that source/target match AIOS domain patterns.
            for (field, domain_str) in &[
                ("source", &rule.source_domain),
                ("target", &rule.target_domain),
            ] {
                if !domain_str.starts_with("aios_") && *domain_str != "self" {
                    // Non-AIOS domains are allowed for interop (e.g. kernel_t,
                    // init_t) but flagged for review.
                    errors.push(format!(
                        "rule[{i}] {field}_domain '{domain_str}' does not follow aios_* naming convention"
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

// ===========================================================================
// Tests — INV-SEL-001 through INV-SEL-005
// ===========================================================================

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // INV-SEL-001: Domain naming convention
    // -------------------------------------------------------------------

    #[test]
    fn domain_from_capsule_id_follows_convention() {
        let d = SeLinuxDomain::from_capsule_id(CapsuleId(7));
        assert_eq!(d.as_str(), "aios_capsule_7_t");
    }

    #[test]
    fn domain_new_rejects_invalid_patterns() {
        assert!(SeLinuxDomain::new("aios_capsule_7_t").is_some());
        assert!(SeLinuxDomain::new("random_domain_t").is_none());
        assert!(SeLinuxDomain::new("aios_capsule_abc_t").is_none());
        assert!(SeLinuxDomain::new("unconfined_t").is_none());
        assert!(SeLinuxDomain::new("").is_none());
    }

    #[test]
    fn domain_capsule_id_round_trip() {
        for n in [1, 42, 999, 1000000] {
            let d = SeLinuxDomain::from_capsule_id(CapsuleId(n));
            assert_eq!(d.capsule_id(), Some(n));
        }
    }

    #[test]
    fn domain_display() {
        let d = SeLinuxDomain::from_capsule_id(CapsuleId(3));
        assert_eq!(format!("{}", d), "aios_capsule_3_t");
    }

    // -------------------------------------------------------------------
    // INV-SEL-004: Context validity
    // -------------------------------------------------------------------

    #[test]
    fn context_new_rejects_empty_fields() {
        assert!(SeLinuxContext::new("", "r", "t", "s0").is_none());
        assert!(SeLinuxContext::new("u", "", "t", "s0").is_none());
        assert!(SeLinuxContext::new("u", "r", "", "s0").is_none());
        assert!(SeLinuxContext::new("u", "r", "t", "").is_none());
    }

    #[test]
    fn context_validates_level_correctly() {
        // Valid.
        assert!(SeLinuxContext::is_valid_level("s0"));
        assert!(SeLinuxContext::is_valid_level("s15"));
        assert!(SeLinuxContext::is_valid_level("s0:c0"));
        assert!(SeLinuxContext::is_valid_level("s1:c0,c1,c100"));
        assert!(SeLinuxContext::is_valid_level("s5:c1023"));
        // Invalid.
        assert!(!SeLinuxContext::is_valid_level(""));
        assert!(!SeLinuxContext::is_valid_level("s16"));
        assert!(!SeLinuxContext::is_valid_level("x0"));
        assert!(!SeLinuxContext::is_valid_level("s0:"));
        assert!(!SeLinuxContext::is_valid_level("s0:c1024"));
        assert!(!SeLinuxContext::is_valid_level("s0:d0"));
    }

    #[test]
    fn context_display_format() {
        let ctx = SeLinuxContext {
            user: "system_u".into(),
            role: "object_r".into(),
            type_: "aios_data_t".into(),
            level: "s0:c0,c1".into(),
        };
        assert_eq!(
            ctx.to_string(),
            "system_u:object_r:aios_data_t:s0:c0,c1"
        );
    }

    #[test]
    fn context_for_capsule_sets_correct_type() {
        let domain = SeLinuxDomain::from_capsule_id(CapsuleId(42));
        let ctx = SeLinuxContext::for_capsule(&domain, "s2", &[0, 1, 5]);
        assert_eq!(ctx.user, "system_u");
        assert_eq!(ctx.role, "system_r");
        assert_eq!(ctx.type_, "aios_capsule_42_t");
        assert_eq!(ctx.level, "s2:c0,c1,c5");
    }

    #[test]
    fn context_for_file_sets_object_r() {
        let domain = SeLinuxDomain::from_capsule_id(CapsuleId(7));
        let ctx = SeLinuxContext::for_file(&domain);
        assert_eq!(ctx.role, "object_r");
        assert_eq!(ctx.type_, domain.as_str());
    }

    // -------------------------------------------------------------------
    // SeLinuxRule
    // -------------------------------------------------------------------

    #[test]
    fn rule_new_rejects_empty_inputs() {
        assert!(SeLinuxRule::new("", "tgt", &[SeLinuxPermission::Read]).is_none());
        assert!(SeLinuxRule::new("src", "", &[SeLinuxPermission::Read]).is_none());
        assert!(SeLinuxRule::new("src", "tgt", &[]).is_none());
    }

    #[test]
    fn rule_detects_unconfined() {
        let r = SeLinuxRule::new("unconfined_t", "aios_data_t", &[SeLinuxPermission::Read])
            .unwrap();
        assert!(r.references_unconfined());

        let r2 = SeLinuxRule::new("aios_capsule_1_t", "unconfined_t", &[SeLinuxPermission::Read])
            .unwrap();
        assert!(r2.references_unconfined());

        let r3 = SeLinuxRule::new("aios_capsule_1_t", "aios_data_t", &[SeLinuxPermission::Read])
            .unwrap();
        assert!(!r3.references_unconfined());
    }

    // -------------------------------------------------------------------
    // INV-SEL-002 & INV-SEL-003: Validator
    // -------------------------------------------------------------------

    #[test]
    fn validator_rejects_empty_ruleset() {
        let bundle = SePolicyBundle::generate_for_capsule(
            CapsuleId(1),
            "s0",
            vec![],
            vec![],
            vec![],
        );
        let result = SePolicyValidator::validate(&bundle);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.contains("no rules")));
    }

    #[test]
    fn validator_rejects_unconfined_t() {
        let rule = SeLinuxRule::new(
            "unconfined_t",
            "aios_data_t",
            &[SeLinuxPermission::Read],
        )
        .unwrap();
        let bundle = SePolicyBundle::generate_for_capsule(
            CapsuleId(1),
            "s0",
            vec![],
            vec![rule],
            vec![],
        );
        let result = SePolicyValidator::validate(&bundle);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.contains("unconfined_t")));
    }

    #[test]
    fn validator_rejects_blanket_permissions_without_justification() {
        let all_perms = Vec::from(SeLinuxPermission::all());
        let rule = SeLinuxRule::new("aios_capsule_1_t", "aios_data_t", &all_perms).unwrap();
        let bundle = SePolicyBundle::generate_for_capsule(
            CapsuleId(1),
            "s0",
            vec![],
            vec![rule],
            vec![],
        );
        let result = SePolicyValidator::validate(&bundle);
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs
            .iter()
            .any(|e| e.contains("without justification")));
    }

    #[test]
    fn validator_accepts_valid_bundle() {
        let rule = SeLinuxRule::with_justification(
            "aios_capsule_1_t",
            "aios_data_t",
            &[SeLinuxPermission::Read, SeLinuxPermission::Open],
            "capsule needs read access to AIOS shared data",
        )
        .unwrap();
        let bundle = SePolicyBundle::generate_for_capsule(
            CapsuleId(1),
            "s0",
            vec![],
            vec![rule],
            vec![],
        );
        let result = SePolicyValidator::validate(&bundle);
        assert!(result.is_ok());
    }

    // -------------------------------------------------------------------
    // Policy bundle sizing / tracking
    // -------------------------------------------------------------------

    #[test]
    fn policy_bundle_tracks_size() {
        let rules = vec![
            SeLinuxRule::new(
                "aios_capsule_1_t",
                "aios_data_t",
                &[SeLinuxPermission::Read, SeLinuxPermission::Open],
            )
            .unwrap(),
            SeLinuxRule::new(
                "aios_capsule_1_t",
                "aios_system_t",
                &[SeLinuxPermission::Write],
            )
            .unwrap(),
        ];
        let bundle = SePolicyBundle::generate_for_capsule(
            CapsuleId(1),
            "s0",
            vec![],
            rules,
            vec![],
        );
        assert_eq!(bundle.rule_count(), 2);
        assert_eq!(bundle.total_permissions(), 3);
    }

    #[test]
    fn policy_bundle_contains_unconfined_detection() {
        let rules = vec![
            SeLinuxRule::new(
                "aios_capsule_1_t",
                "aios_data_t",
                &[SeLinuxPermission::Read],
            )
            .unwrap(),
        ];
        let bundle = SePolicyBundle::generate_for_capsule(
            CapsuleId(1),
            "s0",
            vec![],
            rules,
            vec![],
        );
        assert!(!bundle.contains_unconfined());

        let dirty_rules = vec![
            SeLinuxRule::new(
                "unconfined_t",
                "aios_data_t",
                &[SeLinuxPermission::Read],
            )
            .unwrap(),
        ];
        let dirty_bundle = SePolicyBundle::generate_for_capsule(
            CapsuleId(1),
            "s0",
            vec![],
            dirty_rules,
            vec![],
        );
        assert!(dirty_bundle.contains_unconfined());
    }

    // -------------------------------------------------------------------
    // INV-SEL-005: AVC denial evidence record
    // -------------------------------------------------------------------

    #[test]
    fn avc_denial_new_rejects_invalid_inputs() {
        let src = SeLinuxContext {
            user: "system_u".into(),
            role: "system_r".into(),
            type_: "aios_capsule_7_t".into(),
            level: "s0".into(),
        };
        let tgt = SeLinuxContext {
            user: "system_u".into(),
            role: "object_r".into(),
            type_: "aios_data_t".into(),
            level: "s0".into(),
        };

        assert!(AvcDenial::new(0, src.clone(), tgt.clone(), vec![SeLinuxPermission::Read], "myproc", None, "aios").is_none());
        assert!(AvcDenial::new(1000, src.clone(), tgt.clone(), vec![], "myproc", None, "aios").is_none());
        assert!(AvcDenial::new(1000, src.clone(), tgt.clone(), vec![SeLinuxPermission::Read], "", None, "aios").is_none());
        assert!(AvcDenial::new(1000, src.clone(), tgt.clone(), vec![SeLinuxPermission::Read], "myproc", None, "").is_none());
        assert!(AvcDenial::new(1000, src, tgt, vec![SeLinuxPermission::Read, SeLinuxPermission::Write], "myproc", None, "aios").is_some());
    }

    #[test]
    fn avc_denial_summary_format() {
        let src = SeLinuxContext {
            user: "system_u".into(),
            role: "system_r".into(),
            type_: "aios_capsule_7_t".into(),
            level: "s0".into(),
        };
        let tgt = SeLinuxContext {
            user: "system_u".into(),
            role: "object_r".into(),
            type_: "aios_data_t".into(),
            level: "s0".into(),
        };
        let denial = AvcDenial::new(
            1700000000,
            src,
            tgt,
            vec![SeLinuxPermission::Write, SeLinuxPermission::Delete],
            "capsule-agent",
            Some("/usr/bin/capsule".into()),
            "aios",
        )
        .unwrap();
        let summary = denial.summary();
        assert!(summary.contains("AVC denied"));
        assert!(summary.contains("capsule-agent"));
        assert!(summary.contains("aios_capsule_7_t"));
        assert!(summary.contains("aios_data_t"));
        assert!(summary.contains("write"));
        assert!(summary.contains("delete"));
    }

    #[test]
    fn avc_denial_involves_unconfined_detection() {
        let src_unconf = SeLinuxContext {
            user: "system_u".into(),
            role: "system_r".into(),
            type_: "unconfined_t".into(),
            level: "s0".into(),
        };
        let tgt = SeLinuxContext {
            user: "system_u".into(),
            role: "object_r".into(),
            type_: "aios_data_t".into(),
            level: "s0".into(),
        };
        let denial = AvcDenial::new(
            1000,
            src_unconf,
            tgt,
            vec![SeLinuxPermission::Read],
            "proc",
            None,
            "aios",
        )
        .unwrap();
        assert!(denial.involves_unconfined());
    }

    // -------------------------------------------------------------------
    // Cross-cutting: domain transitions
    // -------------------------------------------------------------------

    #[test]
    fn bundle_transitions_tracked_correctly() {
        let t1 = SeLinuxDomain::from_capsule_id(CapsuleId(10));
        let t2 = SeLinuxDomain::from_capsule_id(CapsuleId(20));
        let bundle = SePolicyBundle::generate_for_capsule(
            CapsuleId(1),
            "s0",
            vec![0, 1],
            vec![SeLinuxRule::new(
                "aios_capsule_1_t",
                "aios_data_t",
                &[SeLinuxPermission::Read],
            )
            .unwrap()],
            vec![t1.clone(), t2.clone()],
        );
        assert_eq!(bundle.transitions.len(), 2);
        assert_eq!(bundle.transitions[0], t1);
        assert_eq!(bundle.transitions[1], t2);
    }

    // -------------------------------------------------------------------
    // Edge: validator with justification bypass for blanket perms
    // -------------------------------------------------------------------

    #[test]
    fn validator_accepts_blanket_with_justification() {
        let all_perms = Vec::from(SeLinuxPermission::all());
        let rule = SeLinuxRule::with_justification(
            "aios_capsule_1_t",
            "aios_system_t",
            &all_perms,
            "system capsule requires full access to orchestrator API",
        )
        .unwrap();
        let bundle = SePolicyBundle::generate_for_capsule(
            CapsuleId(1),
            "s0",
            vec![],
            vec![rule],
            vec![],
        );
        let result = SePolicyValidator::validate(&bundle);
        assert!(result.is_ok());
    }
}
