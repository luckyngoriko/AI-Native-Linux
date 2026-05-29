//! Closed, fail-closed 17-step install pipeline per S11.1 §6.
//!
//! # Architecture
//!
//! The pipeline walks the [`PackageInstallState`] FSM (see [`crate::install_fsm`])
//! through 17 strictly ordered steps. Every step has a closed failure outcome;
//! no step is "best-effort". Any step failure transitions the FSM to
//! `InstallFailed` with the [`PackageVerificationResult`] and failing
//! [`PipelineStep`] recorded in the [`InstallOutcome`].
//!
//! Steps 2, 3, 5, and 6 reuse the T-188 [`crate::TrustChainVerifier`] and
//! T-189 [`crate::manifest_pipeline::verify_manifest`]. Steps 1, 7, 8, 9,
//! 10, 11, 12, 14, and 15 are modelled through the injectable
//! [`InstallPipelineDeps`] trait — real cross-crate wiring is deferred to
//! T-197. Step 17 (First-run capability-lie audit) is a documented hook for
//! T-194; it is **not** executed by this pipeline.
//!
//! # Fail-closed guarantee
//!
//! Every gate is closed: a fetch failure, a signature failure, a sandbox
//! infeasibility, a policy deny, an approval expiration, a recovery-mode
//! violation, or an atomic-install rollback all transition the FSM to
//! `InstallFailed`. The happy path terminates at `Active`.

use chrono::DateTime;
use chrono::Utc;

use crate::install_fsm;
use crate::install_state::{PackageInstallState, PackageVerificationResult};
use crate::lie_audit::{self, AuditOutcome, FirstRunAudit};
use crate::manifest::PackageManifest;
use crate::manifest_pipeline::verify_manifest;
use crate::mirror::MirrorSemantic;
use crate::trust::PublisherTrustLevel;
use crate::trust_chain::LinkSignature;
use crate::verifier::TrustChainVerifier;

// ============================================================================
// PipelineStep — the 17 steps in §6 order
// ============================================================================

/// Each of the 17 install pipeline steps per S11.1 §6.
///
/// Steps are enumerated in the exact §6 order: `Fetch`, `SignatureVerify`,
/// `TrustChainVerify`, `PublisherStateCheck`, `ContentHashVerify`,
/// `ManifestFieldValidation`, `SandboxProfileValidation`, `CapabilityDeclaration`,
/// `NetworkManifestValidation`, `PolicyDecision`, `Approval`, `RecoveryModeGate`,
/// `MarkApprovedInstalling`, `AtomicInstall`, `CapabilityBinding`, `MarkActive`,
/// `FirstRunCapabilityLieAudit`.
///
/// Step 17 (`FirstRunCapabilityLieAudit`) is a documented hook for T-194;
/// the pipeline stops at `Active` (step 16).
#[allow(clippy::doc_markdown)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineStep {
    /// §6.1 — fetch bytes from LOCAL→CACHED→ORIGIN.
    Fetch,
    /// §6.2 — Ed25519 signature verification against the package signing key.
    SignatureVerify,
    /// §6.3 — walk the three-tier chain (manifest→signing key→publisher root→AIOS root).
    TrustChainVerify,
    /// §6.4 — publisher catalog state check (Deplatformed reject, Deprecated no new install).
    PublisherStateCheck,
    /// §6.5 — BLAKE3(content) vs manifest.content_hash.
    ContentHashVerify,
    /// §6.6 — manifest field validation per §5.1 table.
    ManifestFieldValidation,
    /// §6.7 — S3.2 sandbox profile feasibility check.
    SandboxProfileValidation,
    /// §6.8 — capability catalog resolution.
    CapabilityDeclaration,
    /// §6.9 — S8.1 network manifest validation.
    NetworkManifestValidation,
    /// §6.10 — S2.3 policy evaluation.
    PolicyDecision,
    /// §6.11 — S5.3 approval binding.
    Approval,
    /// §6.12 — recovery-mode gate for SYSTEM_ONLY / recovery-only kinds.
    RecoveryModeGate,
    /// §6.13 — transition APPROVED→INSTALLING.
    MarkApprovedInstalling,
    /// §6.14 — atomic install (staging + pointer flip).
    AtomicInstall,
    /// §6.15 — L4 capability bindings issued.
    CapabilityBinding,
    /// §6.16 — transition INSTALLING→ACTIVE; emit PACKAGE_INSTALLED.
    MarkActive,
    /// §6.17 — 60-second first-run capability-lie audit (T-194 hook; NOT executed).
    FirstRunCapabilityLieAudit,
}

impl PipelineStep {
    /// Returns the canonical §6 label for this pipeline step.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fetch => "Fetch",
            Self::SignatureVerify => "SignatureVerify",
            Self::TrustChainVerify => "TrustChainVerify",
            Self::PublisherStateCheck => "PublisherStateCheck",
            Self::ContentHashVerify => "ContentHashVerify",
            Self::ManifestFieldValidation => "ManifestFieldValidation",
            Self::SandboxProfileValidation => "SandboxProfileValidation",
            Self::CapabilityDeclaration => "CapabilityDeclaration",
            Self::NetworkManifestValidation => "NetworkManifestValidation",
            Self::PolicyDecision => "PolicyDecision",
            Self::Approval => "Approval",
            Self::RecoveryModeGate => "RecoveryModeGate",
            Self::MarkApprovedInstalling => "MarkApprovedInstalling",
            Self::AtomicInstall => "AtomicInstall",
            Self::CapabilityBinding => "CapabilityBinding",
            Self::MarkActive => "MarkActive",
            Self::FirstRunCapabilityLieAudit => "FirstRunCapabilityLieAudit",
        }
    }
}

// ============================================================================
// PolicyOutcome / ApprovalOutcome — enumerated decision surfaces
// ============================================================================

/// S2.3 policy decision outcomes (subset of S2.3 §15).
///
/// The install pipeline only consumes the policy outcome relevant to
/// install-authorisation; full S2.3 semantics (including fine-grained
/// reason codes) are deferred to T-197.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyOutcome {
    /// Proceed; no further approval required (rare — low-risk `VERIFIED` apps).
    Allow,
    /// Operator or designated approver must grant approval (S5.3 `EXACT_ACTION`).
    RequireApproval,
    /// Policy denied the install — terminal.
    Deny,
    /// Constitutional hard-deny (e.g. AI subject attempting direct install).
    HardDeny,
}

/// S5.3 approval outcomes (subset — consumed by the install pipeline).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalOutcome {
    /// Approval granted; binding consumed.
    Granted,
    /// Approver explicitly denied.
    Denied,
    /// TTL expired before the approver acted.
    Expired,
    /// Binding was revoked by the issuer.
    Revoked,
    /// Delivery channel failed (e.g. prompt could not reach the approver).
    FailedDelivery,
}

// ============================================================================
// FetchedBytesMeta — result of the fetch step (§6.1)
// ============================================================================

/// Metadata returned by the fetch step (§6.1).
///
/// Carries the host-computed content hash (used in step 5) and the
/// [`MirrorSemantic`] that successfully served the bytes.
#[derive(Debug, Clone)]
pub struct FetchedBytesMeta {
    /// The host-computed BLAKE3 content hash over the fetched bytes
    /// (32-char lowercase hex per §5.1).
    pub content_hash: String,
    /// Which mirror tier served the bytes (`LOCAL`, `CACHED`, or `ORIGIN`).
    pub mirror_semantic: MirrorSemantic,
}

// ============================================================================
// StepFailure — a single step's failure payload
// ============================================================================

/// A failed pipeline step records its identity, the verification result, and
/// a human-readable reason.
#[derive(Debug, Clone)]
pub struct StepFailure {
    /// Which step failed.
    pub step: PipelineStep,
    /// The [`PackageVerificationResult`] describing the failure.
    pub result: PackageVerificationResult,
    /// A human-readable reason string (e.g. `SANDBOX_INFEASIBLE`).
    pub reason: String,
}

// ============================================================================
// InstallOutcome — the pipeline's return value
// ============================================================================

/// The result of executing the install pipeline.
///
/// - On success: `final_state == Active`, `result ∈ {VerifiedAiosRoot,
///   VerifiedPublisher}`, `failed_step == None`.
/// - On failure: `final_state == InstallFailed`, `result` holds the
///   [`PackageVerificationResult`] of the failing step (or a sensible default
///   for non-verification failures like policy/approval/recovery), and
///   `failed_step == Some(step)`.
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    /// The final [`PackageInstallState`] after the pipeline.
    pub final_state: PackageInstallState,
    /// The verification result — on success, the trust-level outcome;
    /// on failure, the step's verification result or a default.
    pub result: PackageVerificationResult,
    /// If the pipeline failed, which step it failed at.
    pub failed_step: Option<PipelineStep>,
}

// ============================================================================
// InstallPipelineDeps — injectable cross-crate steps (T-197)
// ============================================================================

/// Injectable dependencies for cross-crate pipeline steps.
///
/// Each method models a step whose real implementation lives in another crate
/// (S3.2 Sandbox, S5.3 Approval, S8.1 Network, etc.). The trait is
/// implemented by [`InMemoryPipelineDeps`] for tests; real wiring lands in
/// T-197.
///
/// # Failure convention
///
/// Methods that can fail return `Result<T, StepFailure>`. The `step` field
/// in [`StepFailure`] is pre-populated by the pipeline before calling the
/// deps method, and each implementation should preserve it or replace it
/// with the correct step identifier.
#[allow(clippy::missing_errors_doc)]
pub trait InstallPipelineDeps {
    /// §6.1 — fetch the package bytes.
    ///
    /// Attempts `LOCAL → CACHED → ORIGIN` per spec. Returns
    /// [`FetchedBytesMeta`] with the host-computed content hash and the
    /// successful mirror semantic.
    fn fetch(&self, manifest: &PackageManifest) -> Result<FetchedBytesMeta, StepFailure>;

    /// §6.7 — validate the sandbox profile via S3.2 `ComposeProfile`.
    ///
    /// Failure → `StepFailure` with `result = BundleTampered` and reason
    /// `SANDBOX_INFEASIBLE`.
    fn validate_sandbox(&self, manifest: &PackageManifest) -> Result<(), StepFailure>;

    /// §6.8 — resolve each declared capability in the L5/S1.1 catalog.
    ///
    /// Unknown capability → `BundleTampered` with reason `UNKNOWN_CAPABILITY`.
    /// AI-forbidden capability on an `AGENT` package → `BundleTampered`.
    fn check_capabilities(&self, manifest: &PackageManifest) -> Result<(), StepFailure>;

    /// §6.9 — validate the network outbound manifest per S8.1 §G.
    ///
    /// Failure → `BundleTampered` with reason `NETWORK_MANIFEST_INVALID`.
    /// An `AGENT` declaring internet access → reject.
    fn validate_network_manifest(&self, manifest: &PackageManifest) -> Result<(), StepFailure>;

    /// §6.10 — submit to S2.3 `EvaluatePolicy`.
    ///
    /// Returns one of `Allow`, `RequireApproval`, `Deny`, `HardDeny`.
    /// `Deny` and `HardDeny` are terminal for the pipeline.
    fn policy_decision(&self, manifest: &PackageManifest) -> PolicyOutcome;

    /// §6.11 — request S5.3 `EXACT_ACTION` approval.
    ///
    /// Only `Granted` proceeds. All other outcomes are terminal.
    fn request_approval(&self, manifest: &PackageManifest) -> ApprovalOutcome;

    /// §6.12 — is the host currently in recovery mode?
    ///
    /// Must return `true` when the host is in `RecoveryMode = RECOVERY`
    /// (S9.1 §3.2). Required for `SYSTEM_ONLY`-scope and recovery-only
    /// package kinds per §7.
    fn recovery_mode_active(&self) -> bool;

    /// §6.14 — atomic install (write files, run hooks, pointer flip).
    ///
    /// On failure, the state transitions `Installing → InstallFailed`.
    fn atomic_install(&self, manifest: &PackageManifest) -> Result<(), StepFailure>;

    /// §6.15 — issue runtime capability bindings through L4.
    ///
    /// Failure triggers rollback of the atomic install (step 14).
    fn bind_capabilities(&self, manifest: &PackageManifest) -> Result<(), StepFailure>;
}

// ============================================================================
// InMemoryPipelineDeps — configurable stub for tests
// ============================================================================

/// A fully-configurable in-memory implementation of [`InstallPipelineDeps`].
///
/// Each boolean/outcome field controls the result of the corresponding
/// deps method. The `Default` implementation returns success for every
/// step, suitable for happy-path tests.
///
/// # Example
///
/// ```ignore
/// let deps = InMemoryPipelineDeps {
///     sandbox_valid: false,
///     sandbox_failure_reason: "SANDBOX_INFEASIBLE".into(),
///     ..Default::default()
/// };
/// ```
#[allow(clippy::struct_excessive_bools)]
pub struct InMemoryPipelineDeps {
    /// If `true`, `fetch` returns `Ok` with the configured content hash.
    /// Otherwise returns `Err(StepFailure { ... })`.
    pub fetch_success: bool,
    /// The content hash to return on a successful fetch (32-char lowercase hex).
    pub fetched_content_hash: String,
    /// The mirror semantic to return on a successful fetch.
    pub fetched_mirror_semantic: MirrorSemantic,

    /// If `true`, `validate_sandbox` returns `Ok(())`.
    /// Otherwise returns `Err(StepFailure { result: BundleTampered, ... })`.
    pub sandbox_valid: bool,
    /// The reason string for sandbox validation failure (e.g. `SANDBOX_INFEASIBLE`).
    pub sandbox_failure_reason: String,

    /// If `true`, `check_capabilities` returns `Ok(())`.
    /// Otherwise returns `Err(StepFailure { result: BundleTampered, ... })`.
    pub capabilities_valid: bool,
    /// The reason string for capability check failure (e.g. `UNKNOWN_CAPABILITY`).
    pub capabilities_failure_reason: String,

    /// If `true`, `validate_network_manifest` returns `Ok(())`.
    /// Otherwise returns `Err(StepFailure { result: BundleTampered, ... })`.
    pub network_manifest_valid: bool,
    /// The reason string for network manifest failure (e.g. `NETWORK_MANIFEST_INVALID`).
    pub network_manifest_failure_reason: String,

    /// The policy outcome returned by `policy_decision`.
    pub policy_outcome: PolicyOutcome,

    /// The approval outcome returned by `request_approval`.
    pub approval_outcome: ApprovalOutcome,

    /// The value returned by `recovery_mode_active`.
    pub recovery_active: bool,

    /// If `true`, `atomic_install` returns `Ok(())`.
    /// Otherwise returns `Err(StepFailure { ... })`.
    pub atomic_install_success: bool,
    /// The reason string for atomic install failure.
    pub atomic_install_failure_reason: String,

    /// If `true`, `bind_capabilities` returns `Ok(())`.
    /// Otherwise returns `Err(StepFailure { ... })`.
    pub bind_capabilities_success: bool,
    /// The reason string for capability binding failure.
    pub bind_capabilities_failure_reason: String,
}

impl Default for InMemoryPipelineDeps {
    /// Returns an all-success configuration suitable for happy-path tests.
    fn default() -> Self {
        Self {
            fetch_success: true,
            fetched_content_hash: "0".repeat(32),
            fetched_mirror_semantic: MirrorSemantic::Origin,
            sandbox_valid: true,
            sandbox_failure_reason: String::new(),
            capabilities_valid: true,
            capabilities_failure_reason: String::new(),
            network_manifest_valid: true,
            network_manifest_failure_reason: String::new(),
            policy_outcome: PolicyOutcome::RequireApproval,
            approval_outcome: ApprovalOutcome::Granted,
            recovery_active: true,
            atomic_install_success: true,
            atomic_install_failure_reason: String::new(),
            bind_capabilities_success: true,
            bind_capabilities_failure_reason: String::new(),
        }
    }
}

impl InstallPipelineDeps for InMemoryPipelineDeps {
    fn fetch(&self, _manifest: &PackageManifest) -> Result<FetchedBytesMeta, StepFailure> {
        if self.fetch_success {
            Ok(FetchedBytesMeta {
                content_hash: self.fetched_content_hash.clone(),
                mirror_semantic: self.fetched_mirror_semantic,
            })
        } else {
            Err(StepFailure {
                step: PipelineStep::Fetch,
                result: PackageVerificationResult::HashMismatch,
                reason: "FETCH_EXHAUSTED".into(),
            })
        }
    }

    fn validate_sandbox(&self, _manifest: &PackageManifest) -> Result<(), StepFailure> {
        if self.sandbox_valid {
            Ok(())
        } else {
            Err(StepFailure {
                step: PipelineStep::SandboxProfileValidation,
                result: PackageVerificationResult::BundleTampered,
                reason: self.sandbox_failure_reason.clone(),
            })
        }
    }

    fn check_capabilities(&self, _manifest: &PackageManifest) -> Result<(), StepFailure> {
        if self.capabilities_valid {
            Ok(())
        } else {
            Err(StepFailure {
                step: PipelineStep::CapabilityDeclaration,
                result: PackageVerificationResult::BundleTampered,
                reason: self.capabilities_failure_reason.clone(),
            })
        }
    }

    fn validate_network_manifest(&self, _manifest: &PackageManifest) -> Result<(), StepFailure> {
        if self.network_manifest_valid {
            Ok(())
        } else {
            Err(StepFailure {
                step: PipelineStep::NetworkManifestValidation,
                result: PackageVerificationResult::BundleTampered,
                reason: self.network_manifest_failure_reason.clone(),
            })
        }
    }

    fn policy_decision(&self, _manifest: &PackageManifest) -> PolicyOutcome {
        self.policy_outcome
    }

    fn request_approval(&self, _manifest: &PackageManifest) -> ApprovalOutcome {
        self.approval_outcome
    }

    fn recovery_mode_active(&self) -> bool {
        self.recovery_active
    }

    fn atomic_install(&self, _manifest: &PackageManifest) -> Result<(), StepFailure> {
        if self.atomic_install_success {
            Ok(())
        } else {
            Err(StepFailure {
                step: PipelineStep::AtomicInstall,
                result: PackageVerificationResult::BundleTampered,
                reason: self.atomic_install_failure_reason.clone(),
            })
        }
    }

    fn bind_capabilities(&self, _manifest: &PackageManifest) -> Result<(), StepFailure> {
        if self.bind_capabilities_success {
            Ok(())
        } else {
            Err(StepFailure {
                step: PipelineStep::CapabilityBinding,
                result: PackageVerificationResult::BundleTampered,
                reason: self.bind_capabilities_failure_reason.clone(),
            })
        }
    }
}

// ============================================================================
// run_install — the main pipeline entry-point
// ============================================================================

/// Executes the closed, fail-closed 17-step install pipeline (§6).
///
/// # Pipeline walk (normative §6 order)
///
/// 1. **Fetch** — via `deps.fetch()`.
/// 2. **Signature verify** — via [`verify_manifest`] (T-188/T-189).
/// 3. **Trust chain verify** — via [`verify_manifest`].
/// 4. **Publisher state check** — manifest `publisher_trust` checked for
///    `Deprecated` (reject new installs); `Deplatformed` is caught by the
///    verifier chain walk and mapped to this step.
/// 5. **Content hash verify** — via [`verify_manifest`].
/// 6. **Manifest field validation** — via [`verify_manifest`].
/// 7. **Sandbox profile validation** — via `deps.validate_sandbox()`.
/// 8. **Capability declaration** — via `deps.check_capabilities()`.
/// 9. **Network manifest validation** — via `deps.validate_network_manifest()`.
/// 10. **Policy decision** — via `deps.policy_decision()`; `Deny`/`HardDeny` → fail.
/// 11. **Approval** — via `deps.request_approval()`; non-`Granted` → fail.
/// 12. **Recovery-mode gate** — via `deps.recovery_mode_active()`; required-but-
///     not-active → fail.
/// 13. **Mark APPROVED→INSTALLING** — FSM transition.
/// 14. **Atomic install** — via `deps.atomic_install()`.
/// 15. **Capability binding** — via `deps.bind_capabilities()`.
/// 16. **Mark ACTIVE** — FSM transition; stop here.
/// 17. **First-run capability-lie audit** — **NOT executed**; T-194 hook.
///
/// # Happy-path FSM walk
///
/// `Draft → Validating → AwaitingApproval → Approved → Installing → Active`
///
/// # Fail-closed
///
/// Any step failure transitions `→ InstallFailed`. The [`InstallOutcome`]
/// records the failing step and result.
///
/// # Parameters
///
/// - `manifest` — the package manifest to install.
/// - `verifier` — the T-188 trust chain verifier with catalogs.
/// - `deps` — injectable cross-crate dependency provider.
/// - `publisher_root_link_sig` — AIOS root's signature over the publisher root
///   catalog entry (T-188 chain link).
/// - `signing_key_link_sig` — publisher root's signature over the signing key
///   catalog entry (T-188 chain link).
/// - `now` — current host time.
#[must_use]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn run_install(
    manifest: &PackageManifest,
    verifier: &TrustChainVerifier<'_>,
    deps: &dyn InstallPipelineDeps,
    publisher_root_link_sig: &LinkSignature,
    signing_key_link_sig: &LinkSignature,
    now: DateTime<Utc>,
) -> InstallOutcome {
    // ── State tracking ──────────────────────────────────────────────────
    let mut state = PackageInstallState::Draft;

    // Step 0: Enter Validating
    if let Err(err) = install_fsm::apply(&mut state, PackageInstallState::Validating) {
        return install_failed_outcome(
            PackageVerificationResult::BundleTampered,
            Some(PipelineStep::Fetch),
            &err.to_string(),
        );
    }

    // ── Step 1: Fetch ───────────────────────────────────────────────────
    let fetched = match deps.fetch(manifest) {
        Ok(meta) => meta,
        Err(failure) => {
            return install_failed_outcome(
                failure.result,
                Some(PipelineStep::Fetch),
                &failure.reason,
            );
        }
    };

    // ── Steps 2–6: Signature + Chain + Content-Hash + Manifest fields ──
    // Reuses T-188/T-189 verify_manifest. On failure, we map the
    // PackageVerificationResult back to the specific pipeline step.

    let verify_result = verify_manifest(
        manifest,
        verifier,
        &fetched.content_hash,
        publisher_root_link_sig,
        signing_key_link_sig,
        now,
    );

    match verify_result {
        PackageVerificationResult::VerifiedAiosRoot
        | PackageVerificationResult::VerifiedPublisher => {
            // Steps 2–6 passed — continue
        }
        PackageVerificationResult::SignatureFailed => {
            return install_failed_outcome(
                PackageVerificationResult::SignatureFailed,
                Some(PipelineStep::SignatureVerify),
                "Ed25519 signature verification failed",
            );
        }
        PackageVerificationResult::TrustChainBroken => {
            return install_failed_outcome(
                PackageVerificationResult::TrustChainBroken,
                Some(PipelineStep::TrustChainVerify),
                "trust chain broken: missing catalog entry or key revoked",
            );
        }
        PackageVerificationResult::TrustChainTooDeep => {
            return install_failed_outcome(
                PackageVerificationResult::TrustChainTooDeep,
                Some(PipelineStep::TrustChainVerify),
                "trust chain depth exceeds 3 hops",
            );
        }
        PackageVerificationResult::PublisherDeplatformed => {
            return install_failed_outcome(
                PackageVerificationResult::PublisherDeplatformed,
                Some(PipelineStep::PublisherStateCheck),
                "publisher is DEPLATFORMED",
            );
        }
        PackageVerificationResult::HashMismatch => {
            return install_failed_outcome(
                PackageVerificationResult::HashMismatch,
                Some(PipelineStep::ContentHashVerify),
                "content hash does not match manifest",
            );
        }
        PackageVerificationResult::ManifestForged
        | PackageVerificationResult::BundleTampered
        | PackageVerificationResult::RepositoryKindMismatch => {
            return install_failed_outcome(
                verify_result,
                Some(PipelineStep::ManifestFieldValidation),
                "manifest field validation failed",
            );
        }
        PackageVerificationResult::CapabilityLie => {
            // Capability-lie is a runtime audit result (T-194), not an install-time
            // verification result. If it appears here, treat as a field validation
            // failure.
            return install_failed_outcome(
                PackageVerificationResult::CapabilityLie,
                Some(PipelineStep::ManifestFieldValidation),
                "verification failed: capability-lie at install time",
            );
        }
    }

    // ── Step 4: Publisher state check (Deprecated path) ─────────────────
    // Deplatformed was already caught by the verifier above and mapped to
    // PublisherStateCheck. Here we check Deprecated.
    if manifest.publisher_trust == PublisherTrustLevel::Deprecated {
        return install_failed_outcome(
            PackageVerificationResult::ManifestForged,
            Some(PipelineStep::PublisherStateCheck),
            "PUBLISHER_DEPRECATED: no new installs from deprecated publishers",
        );
    }

    // ── Step 7: Sandbox profile validation ──────────────────────────────
    if let Err(failure) = deps.validate_sandbox(manifest) {
        return install_failed_outcome(
            failure.result,
            Some(PipelineStep::SandboxProfileValidation),
            &failure.reason,
        );
    }

    // ── Step 8: Capability declaration vs catalog ───────────────────────
    if let Err(failure) = deps.check_capabilities(manifest) {
        return install_failed_outcome(
            failure.result,
            Some(PipelineStep::CapabilityDeclaration),
            &failure.reason,
        );
    }

    // ── Step 9: Network manifest validation ─────────────────────────────
    if let Err(failure) = deps.validate_network_manifest(manifest) {
        return install_failed_outcome(
            failure.result,
            Some(PipelineStep::NetworkManifestValidation),
            &failure.reason,
        );
    }

    // ── Transition: Validating → AwaitingApproval ───────────────────────
    if let Err(err) = install_fsm::apply(&mut state, PackageInstallState::AwaitingApproval) {
        return install_failed_outcome(
            PackageVerificationResult::BundleTampered,
            Some(PipelineStep::PolicyDecision),
            &err.to_string(),
        );
    }

    // ── Step 10: Policy decision ────────────────────────────────────────
    let policy = deps.policy_decision(manifest);
    match policy {
        PolicyOutcome::Deny | PolicyOutcome::HardDeny => {
            // Non-verification failure → result is a sensible default;
            // the failed_step disambiguates.
            return InstallOutcome {
                final_state: PackageInstallState::InstallFailed,
                result: PackageVerificationResult::BundleTampered,
                failed_step: Some(PipelineStep::PolicyDecision),
            };
        }
        PolicyOutcome::Allow => {
            // Auto-binding — skip approval, transition directly to Approved
            if let Err(err) = install_fsm::apply(&mut state, PackageInstallState::Approved) {
                return install_failed_outcome(
                    PackageVerificationResult::BundleTampered,
                    Some(PipelineStep::Approval),
                    &err.to_string(),
                );
            }
        }
        PolicyOutcome::RequireApproval => {
            // Step 11: Approval
            let approval = deps.request_approval(manifest);
            match approval {
                ApprovalOutcome::Granted => {
                    // Transition: AwaitingApproval → Approved
                    if let Err(err) = install_fsm::apply(&mut state, PackageInstallState::Approved)
                    {
                        return install_failed_outcome(
                            PackageVerificationResult::BundleTampered,
                            Some(PipelineStep::Approval),
                            &err.to_string(),
                        );
                    }
                }
                _ => {
                    // Denied / Expired / Revoked / FailedDelivery → fail
                    return InstallOutcome {
                        final_state: PackageInstallState::InstallFailed,
                        result: PackageVerificationResult::BundleTampered,
                        failed_step: Some(PipelineStep::Approval),
                    };
                }
            }
        }
    }

    // ── Step 12: Recovery-mode gate ─────────────────────────────────────
    // Required for SYSTEM_ONLY scope and recovery-only kinds (§7).
    // The relevant PackageKind values: InvariantBundle, PolicyBundle,
    // IdentityBundle, KernelCandidate, CapabilityCatalogDelta.
    let requires_recovery = manifest.installable_scope
        == crate::package_kind::InstallScope::SystemOnly
        || matches!(
            manifest.kind,
            crate::package_kind::PackageKind::InvariantBundle
                | crate::package_kind::PackageKind::PolicyBundle
                | crate::package_kind::PackageKind::IdentityBundle
                | crate::package_kind::PackageKind::KernelCandidate
                | crate::package_kind::PackageKind::CapabilityCatalogDelta
        );

    if requires_recovery && !deps.recovery_mode_active() {
        return install_failed_outcome(
            PackageVerificationResult::BundleTampered,
            Some(PipelineStep::RecoveryModeGate),
            "RECOVERY_REQUIRED_FOR_PACKAGE_KIND",
        );
    }

    // ── Step 13: Mark APPROVED → INSTALLING ─────────────────────────────
    if let Err(err) = install_fsm::apply(&mut state, PackageInstallState::Installing) {
        return install_failed_outcome(
            PackageVerificationResult::BundleTampered,
            Some(PipelineStep::MarkApprovedInstalling),
            &err.to_string(),
        );
    }

    // ── Step 14: Atomic install ─────────────────────────────────────────
    if let Err(failure) = deps.atomic_install(manifest) {
        // Transition to InstallFailed
        let _ = install_fsm::apply(&mut state, PackageInstallState::InstallFailed);
        return InstallOutcome {
            final_state: state,
            result: failure.result,
            failed_step: Some(PipelineStep::AtomicInstall),
        };
    }

    // ── Step 15: Capability binding ─────────────────────────────────────
    if let Err(failure) = deps.bind_capabilities(manifest) {
        let _ = install_fsm::apply(&mut state, PackageInstallState::InstallFailed);
        return InstallOutcome {
            final_state: state,
            result: failure.result,
            failed_step: Some(PipelineStep::CapabilityBinding),
        };
    }

    // ── Step 16: Mark ACTIVE ────────────────────────────────────────────
    // Transition Installing → Active; STOP here.
    if let Err(err) = install_fsm::apply(&mut state, PackageInstallState::Active) {
        return install_failed_outcome(
            PackageVerificationResult::BundleTampered,
            Some(PipelineStep::MarkActive),
            &err.to_string(),
        );
    }

    // ── Step 17: First-run capability-lie audit ─────────────────────────
    // T-194: Implemented in [`crate::lie_audit`]. The pipeline STOPS at
    // Active (step 16). The audit is a SEPARATE post-activation call via
    // [`run_first_run_audit`] so it stays deterministic and test-injectable
    // (no wall-clock dependency).
    //
    // Call after step 16:
    //   let outcome = run_first_run_audit(&audit, &mut state, now);
    //
    // On drift, the state transitions Active → Quarantined and the outcome
    // is CapabilityLie. Evidence emission (CAPABILITY_LIE_DETECTED) is T-196.

    InstallOutcome {
        final_state: PackageInstallState::Active,
        result: verify_result,
        failed_step: None,
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Constructs an [`InstallOutcome`] for a failed pipeline step.
///
/// Sets `final_state` to `InstallFailed` and populates `result` and
/// `failed_step`. The `reason` is not stored in the outcome (it is
/// available in the [`StepFailure`] when the deps method returns one);
/// for verification failures discovered inside `run_install`, the reason
/// is embedded in the call-site string.
const fn install_failed_outcome(
    result: PackageVerificationResult,
    failed_step: Option<PipelineStep>,
    _reason: &str,
) -> InstallOutcome {
    InstallOutcome {
        final_state: PackageInstallState::InstallFailed,
        result,
        failed_step,
    }
}

// ============================================================================
// run_first_run_audit — step-17 post-activation convenience (§6.17)
// ============================================================================

/// Runs the first-run capability-lie audit (pipeline step 17, §6.17).
///
/// This is a **separate post-activation call** — [`run_install`] stops at
/// `Active` (step 16). The runtime monitor calls this function after the
/// 60-second observation window closes.
///
/// # What it does
///
/// 1. Calls [`FirstRunAudit::evaluate`] to compare `observed ⊆ declared`.
/// 2. Calls [`lie_audit::apply_audit_outcome`] to transition the FSM state.
///
/// # Returns
///
/// The [`AuditOutcome`]:
/// - [`AuditOutcome::Passed`] — all observed capabilities are declared; state
///   stays `Active`.
/// - [`AuditOutcome::Failed`] — under-declaration detected; state transitions
///   `Active → Quarantined`; the `drift` field contains the undeclared
///   capability IDs (sorted, deterministic).
///
/// See [`crate::lie_audit`] for the full audit type, `§9` semantics, and the
/// `§9.4` no-re-audit-release rule.
#[must_use]
pub fn run_first_run_audit(
    audit: &FirstRunAudit,
    state: &mut PackageInstallState,
    now: DateTime<Utc>,
) -> AuditOutcome {
    let outcome = audit.evaluate(now);
    let _ = lie_audit::apply_audit_outcome(state, &outcome);
    outcome
}

// ============================================================================
// Unit tests — FSM integration tests
// ============================================================================

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::similar_names,
    clippy::cast_possible_wrap,
    clippy::too_many_lines,
    clippy::needless_collect,
    clippy::format_collect,
    clippy::too_many_arguments,
    clippy::float_cmp
)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_step_label_non_empty() {
        let steps = [
            PipelineStep::Fetch,
            PipelineStep::SignatureVerify,
            PipelineStep::TrustChainVerify,
            PipelineStep::PublisherStateCheck,
            PipelineStep::ContentHashVerify,
            PipelineStep::ManifestFieldValidation,
            PipelineStep::SandboxProfileValidation,
            PipelineStep::CapabilityDeclaration,
            PipelineStep::NetworkManifestValidation,
            PipelineStep::PolicyDecision,
            PipelineStep::Approval,
            PipelineStep::RecoveryModeGate,
            PipelineStep::MarkApprovedInstalling,
            PipelineStep::AtomicInstall,
            PipelineStep::CapabilityBinding,
            PipelineStep::MarkActive,
            PipelineStep::FirstRunCapabilityLieAudit,
        ];
        for step in &steps {
            assert!(!step.label().is_empty(), "label for {step:?} is empty");
        }
    }

    #[test]
    fn all_seventeen_steps_have_unique_labels() {
        use std::collections::HashSet;
        let labels: Vec<&str> = [
            PipelineStep::Fetch,
            PipelineStep::SignatureVerify,
            PipelineStep::TrustChainVerify,
            PipelineStep::PublisherStateCheck,
            PipelineStep::ContentHashVerify,
            PipelineStep::ManifestFieldValidation,
            PipelineStep::SandboxProfileValidation,
            PipelineStep::CapabilityDeclaration,
            PipelineStep::NetworkManifestValidation,
            PipelineStep::PolicyDecision,
            PipelineStep::Approval,
            PipelineStep::RecoveryModeGate,
            PipelineStep::MarkApprovedInstalling,
            PipelineStep::AtomicInstall,
            PipelineStep::CapabilityBinding,
            PipelineStep::MarkActive,
            PipelineStep::FirstRunCapabilityLieAudit,
        ]
        .iter()
        .map(|s| s.label())
        .collect();
        let set: HashSet<&str> = labels.iter().copied().collect();
        assert_eq!(set.len(), 17, "all 17 steps must have unique labels");
    }

    #[test]
    fn in_memory_deps_default_all_success() {
        let deps = InMemoryPipelineDeps::default();
        let manifest = crate::manifest::PackageManifest::empty_stub();
        assert!(deps.fetch(&manifest).is_ok());
        assert!(deps.validate_sandbox(&manifest).is_ok());
        assert!(deps.check_capabilities(&manifest).is_ok());
        assert!(deps.validate_network_manifest(&manifest).is_ok());
        assert_eq!(
            deps.policy_decision(&manifest),
            PolicyOutcome::RequireApproval
        );
        assert_eq!(deps.request_approval(&manifest), ApprovalOutcome::Granted);
        assert!(deps.recovery_mode_active());
        assert!(deps.atomic_install(&manifest).is_ok());
        assert!(deps.bind_capabilities(&manifest).is_ok());
    }
}
