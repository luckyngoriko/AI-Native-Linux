//! [`PolicyKernel`] trait + [`PolicyContext`] + the [`InMemoryPolicyKernel`] harness.
//!
//! This module defines the contract every Policy Kernel implementation must satisfy
//! (T-023 will add a gRPC server impl, T-024 will add a caching wrapper, the production
//! binary will compose them). The trait is `async` because (a) the production gRPC
//! surface is async, and (b) future enrichment reads against AIOS-FS are async (S2.3 §8);
//! making the trait async today avoids an awkward sync/async bridge later.
//!
//! ## Layering
//!
//! ```text
//!   gRPC layer (T-023)
//!         |
//!         v
//!   PolicyKernel trait  <-- this module
//!         |
//!         v
//!   DecisionPipeline    <-- pipeline.rs
//! ```
//!
//! [`InMemoryPolicyKernel`] is the in-process harness used by tests today and by
//! T-018..T-025 as the substrate to attach the hard-deny engine, bundle loader, etc.

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use aios_action::ActionEnvelope;

use crate::decision::PolicyDecision;
use crate::error::PolicyError;
use crate::hard_deny_engine::HardDenyEngine;
use crate::pipeline::DecisionPipeline;
use crate::subject::HydratedSubject;

/// Resource-enrichment snapshot — S2.3 §8.
///
/// **STUB** — T-018 (hard-deny enforcement) and T-019 (conditions vocabulary) expand
/// this into the full SNAPSHOT-consistent read set (`privacy_class`, `policy_tags`,
/// `kind`, `lifecycle_state`, `created_by`, adapter manifest `risk_template`,
/// `sandbox_profile_id`, …). For T-017 the snapshot is identified by a single
/// `snapshot_id` string and carries no fields — the [`crate::pipeline::DecisionPipeline`]
/// reads nothing out of it.
///
/// The shape is fixed at the trait level today so downstream tasks can grow the struct
/// without touching the [`PolicyKernel`] signature.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrichmentSnapshot {
    /// Stable id for the snapshot (S2.3 §8). Per S2.3 §13 the triple
    /// `(request_hash, bundle_version, enrichment_snapshot_id)` must produce a
    /// deterministic decision; this id is the third component of that triple.
    pub snapshot_id: String,
}

/// Per-evaluation context the Policy Kernel needs alongside the [`ActionEnvelope`].
///
/// Constructed by the caller (today: the test harness; T-023: the gRPC server) and
/// passed by reference into [`PolicyKernel::evaluate_policy`]. The kernel does not own
/// or cache the context; callers are responsible for hydration freshness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyContext {
    /// L4-hydrated subject (S2.3 §7).
    pub subject: HydratedSubject,
    /// Resource enrichment snapshot (S2.3 §8). See [`EnrichmentSnapshot`].
    pub enrichment: EnrichmentSnapshot,
    /// Bundle version that the kernel is currently active on (S2.3 §4 field 4 / §12).
    pub bundle_version: String,
    /// Kernel code version — distinct from `bundle_version`; identifies the Rust
    /// binary build so two decisions that disagree under the same bundle can be
    /// traced to a code drift rather than a policy drift.
    pub code_version: String,
}

impl PolicyContext {
    /// Construct a fresh context with all required fields.
    #[must_use]
    pub fn new(
        subject: HydratedSubject,
        enrichment: EnrichmentSnapshot,
        bundle_version: impl Into<String>,
        code_version: impl Into<String>,
    ) -> Self {
        Self {
            subject,
            enrichment,
            bundle_version: bundle_version.into(),
            code_version: code_version.into(),
        }
    }
}

/// The Policy Kernel contract — S2.3 §3 / §20.
///
/// Every implementation must satisfy the 12-step pipeline (S2.3 §3) and the precedence
/// ladder (S2.3 §5). T-017 ships the [`InMemoryPolicyKernel`] harness; T-023 adds the
/// gRPC server impl; T-024 adds the caching wrapper.
///
/// The trait is `Send + Sync` so impls can be shared across `tokio` tasks (the
/// production server holds one kernel behind `Arc<dyn PolicyKernel>`).
///
/// ## Determinism contract
///
/// Per S2.3 §13 the triple `(request_hash, bundle_version, enrichment_snapshot_id)`
/// must produce the same [`PolicyDecision`]. The trait signature alone does not
/// enforce this — it is a contract on the impl. T-024 lands the cache that codifies it
/// in code; T-017 leaves it as a documented invariant.
#[async_trait]
pub trait PolicyKernel: Send + Sync {
    /// Evaluate a typed action envelope against the active policy bundle.
    ///
    /// Returns a fully populated [`PolicyDecision`] per S2.3 §4 on success, or a
    /// [`PolicyError`] when a pipeline pre-condition fails (subject hydration,
    /// enrichment read, bundle load, schema). Callers handle the error variant by
    /// short-circuiting to `DENY` themselves; the kernel does not silently fall through.
    async fn evaluate_policy(
        &self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
    ) -> Result<PolicyDecision, PolicyError>;
}

/// In-process [`PolicyKernel`] impl backed by the [`DecisionPipeline`].
///
/// T-017 ships this with all step-4..8 / 10 / 12 hooks deliberately stubbed (see
/// `pipeline.rs`). T-018..T-025 each replace one or more stubs with the real impl.
/// The harness is what the test suite + the future Capability Runtime integration
/// tests use.
///
/// Composes the stateless pipeline driver with an optional
/// [`HardDenyEngine`] (T-018). Future tasks attach a bundle index (T-022),
/// cache (T-024), and rate limiter through the same composition discipline:
/// add a field, expose a `new_with_*` ctor, and route the pipeline driver
/// through the engine-aware overload — never break the bare `new()` baseline
/// that the T-017 tests pin.
#[derive(Debug, Default, Clone)]
pub struct InMemoryPolicyKernel {
    pipeline: DecisionPipeline,
    /// `None` = T-017 baseline (step 4 is a stub pass-through). `Some(engine)`
    /// = T-018+ (step 4 enforces the §6 hard-deny table). The engine is held
    /// behind an `Arc` so cloning the kernel (and so cloning the future
    /// `Arc<dyn PolicyKernel>` server handle) is `O(1)`.
    hard_deny_engine: Option<Arc<HardDenyEngine>>,
}

impl InMemoryPolicyKernel {
    /// Construct a fresh in-memory kernel with no hard-deny engine attached.
    ///
    /// Step 4 of the decision pipeline remains the T-017 stub pass-through;
    /// every evaluation flows to the default-deny floor (S2.3 §11). This is
    /// the ctor the T-017 baseline tests use; T-018 keeps it stable.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pipeline: DecisionPipeline::new(),
            hard_deny_engine: None,
        }
    }

    /// Construct a kernel pre-loaded with a [`HardDenyEngine`] (T-018).
    ///
    /// Production wiring constructs the engine via
    /// [`HardDenyEngine::new_with_defaults`] and passes it in here; tests use
    /// custom configs to exercise individual §6 rows in isolation. The engine
    /// is taken by value and wrapped in an `Arc` so the kernel can be cloned
    /// freely.
    #[must_use]
    pub fn new_with_hard_deny(engine: HardDenyEngine) -> Self {
        Self {
            pipeline: DecisionPipeline::new(),
            hard_deny_engine: Some(Arc::new(engine)),
        }
    }

    /// `true` when this kernel has a hard-deny engine attached.
    #[must_use]
    pub const fn has_hard_deny_engine(&self) -> bool {
        self.hard_deny_engine.is_some()
    }
}

#[async_trait]
impl PolicyKernel for InMemoryPolicyKernel {
    async fn evaluate_policy(
        &self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
    ) -> Result<PolicyDecision, PolicyError> {
        // T-017 cannot raise PolicyError today — subject hydration is supplied by the
        // caller (T-021 lands the real hydrator that can fail), enrichment is a stub
        // (T-018+), and bundle loading is a stub (T-022). When those tasks land, the
        // failure modes will surface here as `Err(...)`. For now every evaluation flows
        // through the pipeline; with the hard-deny engine attached step 4 may
        // short-circuit on a §6 class, otherwise the evaluation lands at the
        // default-deny floor (S2.3 §11).
        Ok(self.pipeline.evaluate_with_hard_deny_engine(
            envelope,
            context,
            self.hard_deny_engine.as_deref(),
        ))
    }
}
