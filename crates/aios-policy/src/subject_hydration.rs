//! Subject hydration — S2.3 §7.
//!
//! The Policy Kernel accepts a provisional `<type>:<name>[/<sub_id>]` subject string
//! from the envelope's [`aios_action::Identity::subject_canonical_id`] and canonicalises
//! it through the L4 identity service into a full [`HydratedSubject`] (S2.3 §7). The
//! result carries the subject's stable canonical id, groups, capabilities, session class,
//! recovery-mode bit and the AI flag. Subject hydration is **part of the determinism
//! triple** — the hydrated subject contributes to the enrichment snapshot (§8) and so
//! every decision is reproducible against `(request_hash, bundle_version,
//! enrichment_snapshot_id)`.
//!
//! ## What lives here vs. M4+
//!
//! - **T-021 (this module):** the [`SubjectHydrator`] async trait + an in-memory
//!   [`InMemoryHydrator`] backed by a `HashMap<String, HydratedRecord>`. Production
//!   wires a real L4 identity backend (gRPC, certificate-based, vault-backed) through
//!   the same trait surface — no pipeline change required.
//! - **M4+:** L4 identity service itself. Live group resolution, capability propagation,
//!   recovery-mode credential check, certificate validation — all behind the trait.
//!
//! The trait method signature is the contract; an implementation may be sync underneath
//! but the trait stays `async` so the future gRPC-backed L4 service flows through
//! without adapter glue.
//!
//! ## Failure modes
//!
//! Per S2.3 §7 the pipeline short-circuits to `DENY` with
//! `reason_code = SubjectUnauthenticated` when any of:
//!
//! - the provisional subject id is unknown to the identity store,
//! - the canned record is expired (`valid_until <= now`),
//! - the record is revoked (`revoked = true`).
//!
//! All three cases collapse onto the single typed error
//! [`crate::PolicyError::SubjectUnauthenticated`] — the spec deliberately does not
//! discriminate between "unknown" and "expired" at the policy boundary because
//! discrimination would leak identity-existence information to a caller.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::error::PolicyError;
use crate::subject::{HydratedSubject, SubjectType};

/// A canned record inside the in-memory hydrator.
///
/// Models the slice of L4 identity state the Policy Kernel needs for §7
/// canonicalisation: a fully populated [`HydratedSubject`] plus the two
/// expiration / revocation flags that drive the `SubjectUnauthenticated`
/// short-circuit. The shape is deliberately small — the production L4 record is
/// richer (certificate chain, vault binding, act-as policies) but the policy
/// boundary only consumes the §7 fields, so [`InMemoryHydrator`] models only
/// those.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HydratedRecord {
    /// The hydrated subject returned on a successful lookup.
    pub subject: HydratedSubject,
    /// Absolute expiry. `None` = no expiry. `Some(t)` with `t <= now` triggers
    /// `SubjectUnauthenticated` per §7 ("expired").
    pub valid_until: Option<DateTime<Utc>>,
    /// Explicit revocation flag — `true` triggers `SubjectUnauthenticated` per
    /// §7 ("revoked") regardless of `valid_until`.
    pub revoked: bool,
}

impl HydratedRecord {
    /// Construct a fresh record from a [`HydratedSubject`], with no expiry and
    /// no revocation. The common fixture shape.
    #[must_use]
    pub const fn new(subject: HydratedSubject) -> Self {
        Self {
            subject,
            valid_until: None,
            revoked: false,
        }
    }

    /// Mark this record as revoked. Chainable builder helper.
    #[must_use]
    pub const fn revoked(mut self) -> Self {
        self.revoked = true;
        self
    }

    /// Attach an absolute expiry to this record. Chainable builder helper.
    #[must_use]
    pub const fn with_expiry(mut self, at: DateTime<Utc>) -> Self {
        self.valid_until = Some(at);
        self
    }

    /// Returns `true` when this record is currently usable (not revoked, not
    /// past its `valid_until`). Used internally by [`InMemoryHydrator::hydrate`]
    /// to collapse the two §7 failure modes onto a single
    /// `SubjectUnauthenticated` short-circuit.
    fn is_currently_valid(&self, now: DateTime<Utc>) -> bool {
        if self.revoked {
            return false;
        }
        self.valid_until.is_none_or(|t| t > now)
    }
}

/// Subject hydrator — S2.3 §7.
///
/// Implementations canonicalise a provisional `<type>:<name>[/<sub_id>]` subject id
/// into a [`HydratedSubject`] or fail closed with
/// [`PolicyError::SubjectUnauthenticated`] when the subject is unknown, expired or
/// revoked.
///
/// The trait is `async` so a future gRPC-backed L4 service composes through the
/// same surface without an adapter layer. Implementations must also be
/// `Send + Sync` because [`crate::InMemoryPolicyKernel`] holds the hydrator
/// behind an `Arc<dyn SubjectHydrator + Send + Sync>` and is shared across
/// `tokio` tasks.
#[async_trait]
pub trait SubjectHydrator: Send + Sync {
    /// Look up the provisional subject id in the identity store and return a
    /// fully populated [`HydratedSubject`] per S2.3 §7.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError::SubjectUnauthenticated`] when the provisional id
    /// is unknown to the store, the record is expired (`valid_until <= now`),
    /// or the record is revoked. The three cases collapse onto a single error
    /// at the boundary by design (§7).
    async fn hydrate(&self, provisional: &str) -> Result<HydratedSubject, PolicyError>;
}

/// In-memory [`SubjectHydrator`] backed by a fixed `HashMap<String, HydratedRecord>`.
///
/// Used by:
///
/// - the T-021 test suite (canned records exercise every §7 failure mode);
/// - `cargo test --workspace` integration tests for downstream tasks that need a
///   deterministic, dependency-free identity source;
/// - the M3 acceptance fixtures (T-025) which pin the §17 AI self-approval
///   prevention behaviour against a known subject set.
///
/// Production never constructs this — the real L4 identity service ships as a
/// separate impl behind the [`SubjectHydrator`] trait in M4+.
#[derive(Debug, Default, Clone)]
pub struct InMemoryHydrator {
    /// Lookup table — provisional subject id → canned hydrated record.
    records: HashMap<String, HydratedRecord>,
}

impl InMemoryHydrator {
    /// Construct an empty hydrator. Every `hydrate` call returns
    /// `SubjectUnauthenticated`. Useful for the "no subject is recognised"
    /// pipeline-short-circuit test.
    #[must_use]
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
        }
    }

    /// Construct a hydrator pre-loaded with the four representative subject
    /// classes the test suite + downstream tasks rely on:
    ///
    /// - `human:lucky` — interactive human operator (`subject_type = Human`,
    ///   `is_ai = false`).
    /// - `agent:dev` — autonomous LLM agent (`subject_type = Agent`,
    ///   `is_ai = true`). This is the canonical AI subject used in the §17
    ///   self-approval prevention fixtures (`pk.fix.ai_self_approval_blocked.v1`).
    /// - `application:planner` — long-running application subject
    ///   (`subject_type = Application`, `is_ai = true`). Demonstrates that the
    ///   `is_ai` flag covers both AI subject types per §7.
    /// - `service:systemd` — non-AI service subject (`subject_type = Service`,
    ///   `is_ai = false`).
    ///
    /// All four records are fresh (no expiry, not revoked). Tests that need
    /// expired / revoked records construct them explicitly via
    /// [`HydratedRecord::revoked`] / [`HydratedRecord::with_expiry`] +
    /// [`Self::insert`].
    #[must_use]
    pub fn with_fixtures() -> Self {
        let mut hydrator = Self::new();
        hydrator.insert(
            "human:lucky",
            HydratedRecord::new(HydratedSubject {
                canonical_subject_id: "human:lucky:01HX0000000000000000000000".to_owned(),
                subject_type: SubjectType::Human,
                groups: vec!["operators".to_owned()],
                capabilities: vec![],
                session_class: "INTERNAL".to_owned(),
                recovery_mode: false,
                is_ai: false,
            }),
        );
        hydrator.insert(
            "agent:dev",
            HydratedRecord::new(HydratedSubject {
                canonical_subject_id: "agent:dev:01HX1111111111111111111111".to_owned(),
                subject_type: SubjectType::Agent,
                groups: vec!["cognitive-core".to_owned()],
                capabilities: vec![],
                session_class: "INTERNAL".to_owned(),
                recovery_mode: false,
                is_ai: true,
            }),
        );
        hydrator.insert(
            "application:planner",
            HydratedRecord::new(HydratedSubject {
                canonical_subject_id: "application:planner:01HX2222222222222222222222".to_owned(),
                subject_type: SubjectType::Application,
                groups: vec!["apps".to_owned()],
                capabilities: vec![],
                session_class: "INTERNAL".to_owned(),
                recovery_mode: false,
                is_ai: true,
            }),
        );
        hydrator.insert(
            "service:systemd",
            HydratedRecord::new(HydratedSubject {
                canonical_subject_id: "service:systemd:01HX3333333333333333333333".to_owned(),
                subject_type: SubjectType::Service,
                groups: vec!["system".to_owned()],
                capabilities: vec![],
                session_class: "INTERNAL".to_owned(),
                recovery_mode: false,
                is_ai: false,
            }),
        );
        hydrator
    }

    /// Insert (or overwrite) a record for the supplied provisional subject id.
    pub fn insert(&mut self, provisional: impl Into<String>, record: HydratedRecord) {
        self.records.insert(provisional.into(), record);
    }

    /// Returns the number of canned records currently held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns `true` when no canned records are held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[async_trait]
impl SubjectHydrator for InMemoryHydrator {
    async fn hydrate(&self, provisional: &str) -> Result<HydratedSubject, PolicyError> {
        let record = self
            .records
            .get(provisional)
            .ok_or(PolicyError::SubjectUnauthenticated)?;
        if !record.is_currently_valid(Utc::now()) {
            return Err(PolicyError::SubjectUnauthenticated);
        }
        Ok(record.subject.clone())
    }
}

/// Blanket impl: an `Arc<dyn SubjectHydrator + Send + Sync>` is itself a
/// `SubjectHydrator`. This lets [`crate::InMemoryPolicyKernel`] hold the
/// hydrator behind an `Arc` and clone the kernel handle freely while still
/// passing `&dyn SubjectHydrator` to the pipeline driver.
#[async_trait]
impl<T> SubjectHydrator for Arc<T>
where
    T: SubjectHydrator + ?Sized + Send + Sync,
{
    async fn hydrate(&self, provisional: &str) -> Result<HydratedSubject, PolicyError> {
        (**self).hydrate(provisional).await
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
    use chrono::Duration;

    #[tokio::test]
    async fn empty_hydrator_rejects_every_lookup() {
        let h = InMemoryHydrator::new();
        let err = h.hydrate("agent:dev").await.expect_err("must error");
        assert_eq!(err, PolicyError::SubjectUnauthenticated);
    }

    #[tokio::test]
    async fn revoked_record_is_rejected() {
        let mut h = InMemoryHydrator::new();
        h.insert(
            "agent:dev",
            HydratedRecord::new(HydratedSubject {
                canonical_subject_id: "agent:dev:01HXAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
                subject_type: SubjectType::Agent,
                groups: vec![],
                capabilities: vec![],
                session_class: "INTERNAL".to_owned(),
                recovery_mode: false,
                is_ai: true,
            })
            .revoked(),
        );
        let err = h.hydrate("agent:dev").await.expect_err("must error");
        assert_eq!(err, PolicyError::SubjectUnauthenticated);
    }

    #[tokio::test]
    async fn past_expiry_is_rejected() {
        let mut h = InMemoryHydrator::new();
        h.insert(
            "agent:dev",
            HydratedRecord::new(HydratedSubject {
                canonical_subject_id: "agent:dev:01HXAAAAAAAAAAAAAAAAAAAAAA".to_owned(),
                subject_type: SubjectType::Agent,
                groups: vec![],
                capabilities: vec![],
                session_class: "INTERNAL".to_owned(),
                recovery_mode: false,
                is_ai: true,
            })
            .with_expiry(Utc::now() - Duration::hours(1)),
        );
        let err = h.hydrate("agent:dev").await.expect_err("must error");
        assert_eq!(err, PolicyError::SubjectUnauthenticated);
    }
}
