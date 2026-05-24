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

use aios_action::ActionEnvelope;

use crate::cache::SharedDecisionCache;
use crate::decision::PolicyDecision;
use crate::error::PolicyError;
use crate::hard_deny_engine::HardDenyEngine;
use crate::pipeline::DecisionPipeline;
use crate::snapshot::EnrichmentSnapshot;
use crate::subject::HydratedSubject;
use crate::subject_hydration::SubjectHydrator;
use std::sync::RwLock;

use crate::bundle::PolicyBundle;

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
#[derive(Default, Clone)]
pub struct InMemoryPolicyKernel {
    pipeline: DecisionPipeline,
    /// `None` = T-017 baseline (step 4 is a stub pass-through). `Some(engine)`
    /// = T-018+ (step 4 enforces the §6 hard-deny table). The engine is held
    /// behind an `Arc` so cloning the kernel (and so cloning the future
    /// `Arc<dyn PolicyKernel>` server handle) is `O(1)`.
    hard_deny_engine: Option<Arc<HardDenyEngine>>,
    /// `None` = T-017 baseline (step 2 uses the supplied [`HydratedSubject`]
    /// as-passed). `Some(hydrator)` = T-021+ (step 2 calls
    /// [`SubjectHydrator::hydrate`] with the envelope's provisional id and
    /// replaces the context's subject with the canonical record; lookup
    /// failure short-circuits to `DENY` / `SubjectUnauthenticated` per §7).
    /// The hydrator is held behind an `Arc<dyn ...>` so production wiring can
    /// swap implementations without touching this struct.
    subject_hydrator: Option<Arc<dyn SubjectHydrator + Send + Sync>>,
    /// `None` = T-024 baseline (every evaluation hits the pipeline).
    /// `Some(cache)` = §13.2 cache attached; on evaluation the cache is
    /// consulted by `(request_hash, bundle_version)` per §13.3 before the
    /// pipeline runs, and the result is inserted on a miss. Held behind an
    /// `Arc<RwLock<...>>` ([`SharedDecisionCache`]) so the gRPC adapter can
    /// share one cache across tokio worker tasks.
    decision_cache: Option<SharedDecisionCache>,
    /// Optional active bundle pointer for the `LoadBundle` RPC path
    /// (T-024 — see [`Self::set_active_bundle`]). Held behind
    /// `Arc<RwLock<Option<PolicyBundle>>>` so bundle hot-reload (§12.4) is
    /// atomic w.r.t. in-flight evaluations. `None` = no bundle activated
    /// (the kernel runs on the §11 default-deny floor). The pipeline does
    /// not yet consult this field — bundle-aware steps 6 / 7 land in T-025.
    /// The bundle is held here so the `LoadBundle` RPC can update one
    /// canonical location instead of the gRPC adapter carrying its own
    /// shadow copy that drifts from the kernel state.
    active_bundle: Arc<RwLock<Option<PolicyBundle>>>,
}

impl std::fmt::Debug for InMemoryPolicyKernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryPolicyKernel")
            .field("pipeline", &self.pipeline)
            .field("hard_deny_engine", &self.hard_deny_engine)
            .field("subject_hydrator", &self.subject_hydrator.is_some())
            .field("decision_cache", &self.decision_cache.is_some())
            .field("active_bundle", &"<RwLock<Option<PolicyBundle>>>")
            .finish()
    }
}

impl InMemoryPolicyKernel {
    /// Construct a fresh in-memory kernel with no hard-deny engine attached.
    ///
    /// Step 4 of the decision pipeline remains the T-017 stub pass-through;
    /// every evaluation flows to the default-deny floor (S2.3 §11). This is
    /// the ctor the T-017 baseline tests use; T-018 keeps it stable.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pipeline: DecisionPipeline::new(),
            hard_deny_engine: None,
            subject_hydrator: None,
            decision_cache: None,
            active_bundle: Arc::new(RwLock::new(None)),
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
            subject_hydrator: None,
            decision_cache: None,
            active_bundle: Arc::new(RwLock::new(None)),
        }
    }

    /// Construct a kernel pre-loaded with both a [`HardDenyEngine`] (T-018)
    /// and a [`SubjectHydrator`] (T-021).
    ///
    /// This is the full §3-pipeline wiring: step 2 calls the hydrator (and
    /// short-circuits on `SubjectUnauthenticated`), step 4 calls the
    /// hard-deny engine. The two are kept as separate ctors (`new()`,
    /// `new_with_hard_deny()`, this one) so the T-017 / T-018 baseline tests
    /// continue to pin the partial-wiring contract.
    #[must_use]
    pub fn new_with_full_chain(
        hydrator: Arc<dyn SubjectHydrator + Send + Sync>,
        engine: HardDenyEngine,
    ) -> Self {
        Self {
            pipeline: DecisionPipeline::new(),
            hard_deny_engine: Some(Arc::new(engine)),
            subject_hydrator: Some(hydrator),
            decision_cache: None,
            active_bundle: Arc::new(RwLock::new(None)),
        }
    }

    /// Construct a kernel pre-loaded with a [`SubjectHydrator`] only — no
    /// hard-deny engine. Used by the T-021 tests that exercise the §17
    /// AI self-approval prevention path in isolation from the §6 floor.
    #[must_use]
    pub fn new_with_subject_hydrator(hydrator: Arc<dyn SubjectHydrator + Send + Sync>) -> Self {
        Self {
            pipeline: DecisionPipeline::new(),
            hard_deny_engine: None,
            subject_hydrator: Some(hydrator),
            decision_cache: None,
            active_bundle: Arc::new(RwLock::new(None)),
        }
    }

    /// Construct a kernel with a [`SharedDecisionCache`] attached (T-024).
    ///
    /// Cache hits return the cached decision verbatim except for
    /// `evaluated_at`, which is refreshed to "now" so callers can tell when
    /// the decision was last served (the §13.2 TTL is still measured against
    /// the original constraints, not the refreshed timestamp). The cache is
    /// keyed by `(request_hash, bundle_version)` per §13.3.
    #[must_use]
    pub fn new_with_cache(cache: SharedDecisionCache) -> Self {
        Self {
            pipeline: DecisionPipeline::new(),
            hard_deny_engine: None,
            subject_hydrator: None,
            decision_cache: Some(cache),
            active_bundle: Arc::new(RwLock::new(None)),
        }
    }

    /// Construct a kernel with hard-deny + hydrator + cache all attached
    /// (T-024).
    ///
    /// This is the canonical production ctor: every §3 pipeline step that
    /// has a real implementation by T-024 is wired. T-025 attaches the
    /// override-boundary engine on top of this same shape.
    #[must_use]
    pub fn new_with_full_chain_and_cache(
        hydrator: Arc<dyn SubjectHydrator + Send + Sync>,
        engine: HardDenyEngine,
        cache: SharedDecisionCache,
    ) -> Self {
        Self {
            pipeline: DecisionPipeline::new(),
            hard_deny_engine: Some(Arc::new(engine)),
            subject_hydrator: Some(hydrator),
            decision_cache: Some(cache),
            active_bundle: Arc::new(RwLock::new(None)),
        }
    }

    /// Swap the active policy bundle pointer (called by the `LoadBundle` RPC
    /// in `service/server.rs` after a successful `BundleLoader` round-trip).
    ///
    /// The previous bundle (if any) is returned so the caller can fire the
    /// §13.2 cache invalidation pass on the **old** bundle version
    /// (clearing every cache entry whose `bundle_version` matches the
    /// pre-swap value). T-025 wires the override boundary on the same
    /// pointer; today the pipeline does not yet consult `active_bundle`,
    /// so the swap is a forward-compat capture.
    ///
    /// Atomicity: the swap is `RwLock::write()`-guarded so no concurrent
    /// evaluation observes a torn bundle.
    #[allow(clippy::must_use_candidate)]
    pub fn set_active_bundle(&self, bundle: PolicyBundle) -> Option<PolicyBundle> {
        let mut slot = match self.active_bundle.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        slot.replace(bundle)
    }

    /// Return a clone of the currently active bundle (or `None`).
    #[must_use]
    pub fn active_bundle_snapshot(&self) -> Option<PolicyBundle> {
        let slot = match self.active_bundle.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        slot.clone()
    }

    /// Return a clone of the kernel's [`SharedDecisionCache`] handle, when
    /// one is attached.
    ///
    /// Used by the `LoadBundle` RPC to call
    /// [`SharedDecisionCache::invalidate_for_bundle`] on the **old**
    /// bundle's version after a successful swap.
    #[must_use]
    pub fn cache_handle(&self) -> Option<SharedDecisionCache> {
        self.decision_cache.clone()
    }

    /// `true` when this kernel has a hard-deny engine attached.
    #[must_use]
    pub const fn has_hard_deny_engine(&self) -> bool {
        self.hard_deny_engine.is_some()
    }

    /// `true` when this kernel has a subject hydrator attached.
    #[must_use]
    pub const fn has_subject_hydrator(&self) -> bool {
        self.subject_hydrator.is_some()
    }
}

#[async_trait]
impl PolicyKernel for InMemoryPolicyKernel {
    async fn evaluate_policy(
        &self,
        envelope: &ActionEnvelope,
        context: &PolicyContext,
    ) -> Result<PolicyDecision, PolicyError> {
        // T-024: cache lookup BEFORE hydration / pipeline. Per S2.3 §13.3 the
        // cache key is `(request_hash, bundle_version)`; computing it here
        // means a cache hit short-circuits the entire pipeline (and crucially
        // the §18.1 sub-millisecond cached budget is achievable). Cache hits
        // refresh the `evaluated_at` timestamp so callers see "decision served
        // at time T" while the §13.2 TTL is still measured against the
        // original `constraints.ttl_seconds`.
        let request_hash = envelope.request.request_hash().unwrap_or_default();
        if let Some(cache) = &self.decision_cache {
            if let Some(mut cached) = cache.get(&crate::cache::CacheKey {
                request_hash: request_hash.clone(),
                bundle_version: context.bundle_version.clone(),
            }) {
                cached.evaluated_at = chrono::Utc::now();
                return Ok(cached);
            }
        }

        // T-021: step 2 calls the hydrator when one is attached and replaces
        // the context's subject with the canonical record. Hydrator failure
        // (`SubjectUnauthenticated`) is converted into a `DENY` decision by
        // the pipeline driver so callers see a uniform short-circuit shape;
        // the typed error is not propagated out — per §7 every envelope
        // produces a decision, never `Err(...)`. Enrichment + bundle loading
        // can still raise their own typed errors when those wires land.
        let hydrated_context = if let Some(hydrator) = self.subject_hydrator.as_deref() {
            match hydrator
                .hydrate(&envelope.identity.subject_canonical_id)
                .await
            {
                Ok(subject) => {
                    let mut c = context.clone();
                    c.subject = subject;
                    Some(c)
                }
                Err(PolicyError::SubjectUnauthenticated) => None,
                Err(other) => return Err(other),
            }
        } else {
            Some(context.clone())
        };

        let decision = self.pipeline.evaluate_with_chain(
            envelope,
            context,
            hydrated_context.as_ref(),
            self.hard_deny_engine.as_deref(),
        );

        // T-024: write-through insert on cache miss. The cached decision
        // retains its `policy_decision_id` even though that id is per-
        // evaluation by §4; the cache contract intentionally preserves the
        // FIRST id so a downstream audit can pivot from a follow-up cached
        // hit back to the original decision record. The freshened
        // `evaluated_at` on subsequent reads is what tells the audit "this
        // is a re-served instance, not a new evaluation".
        if let Some(cache) = &self.decision_cache {
            cache.put(
                crate::cache::CacheKey {
                    request_hash,
                    bundle_version: context.bundle_version.clone(),
                },
                decision.clone(),
            );
        }

        Ok(decision)
    }
}
