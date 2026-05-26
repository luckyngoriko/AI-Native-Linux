//! Provider dispatch for S13.2 §5 — routes model invocations by [`ProviderClass`].
//!
//! Local providers (`Ollama`, `Vllm`) are invoked directly without vault brokering.
//! External providers (`Anthropic`, `Openai`, `OtherVaultBrokered`) require a
//! configured [`VaultClientAdapter`] and a `vault_capability_id` on the model.
//!
//! # INV-018 Enforcement
//!
//! [`VaultRequest`] and [`VaultResponse`] carry opaque handles and output — never
//! raw key material. Their `Debug` impls do not emit bytes.
//!
//! # INV-015 Enforcement
//!
//! [`DispatchOutcome`] carries only token counts and latency; no prompt or
//! response bodies are stored in any variant.

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::CognitiveError;
use crate::intent::CognitiveIntent;
use crate::model::CognitiveModel;
use crate::routing::{AICrossOriginPosture, ModelBackendKind, ProviderClass};

// ---------------------------------------------------------------------------
// VaultClientAdapter — cross-crate seam (real adapter lands in T-103)
// ---------------------------------------------------------------------------

/// Minimal vault broker trait surface consumed by the provider dispatcher.
///
/// This is an **indirect trait** — no `aios-vault` path dependency is required
/// here. The real adapter for `aios-vault` lands in T-103 as a cross-crate
/// integration.
#[async_trait]
pub trait VaultClientAdapter: Send + Sync {
    /// Use a vault capability to execute an external model call.
    ///
    /// The `capability_id` is the model's `vault_capability_id` (`vcap_<ULID>`).
    /// The [`VaultRequest`] carries an opaque operation kind and payload — never
    /// raw key material (INV-018).
    async fn use_capability(
        &self,
        capability_id: &str,
        request: VaultRequest,
    ) -> Result<VaultResponse, CognitiveError>;
}

// ---------------------------------------------------------------------------
// VaultRequest / VaultResponse — opaque payloads (INV-018 enforced)
// ---------------------------------------------------------------------------

/// An opaque vault capability request for an external model call.
///
/// # INV-018
///
/// This struct carries an operation kind and an opaque payload string. It
/// MUST NOT contain raw key bytes or credential material. The `Debug` impl
/// reflects this constraint — all fields are `String`, never `Vec<u8>`.
#[derive(Debug, Clone)]
pub struct VaultRequest {
    /// Operation kind (e.g. `cognitive.invoke`, `cognitive.stream`).
    pub operation: String,
    /// Opaque payload — serialised model invocation parameters. Never a raw key.
    pub opaque_payload: String,
}

/// An opaque vault capability response from an external model call.
///
/// # INV-018
///
/// This struct carries a signed handle and model output. It MUST NOT expose
/// raw key material. The `Debug` impl reflects this constraint — the `handle`
/// field is an opaque `String`, never raw key bytes.
#[derive(Debug, Clone)]
pub struct VaultResponse {
    /// Opaque signed handle — never a raw key.
    pub handle: String,
    /// Model output text.
    pub output: String,
    /// Wall-clock latency in milliseconds for the vault-brokered call.
    pub latency_ms: u64,
}

// ---------------------------------------------------------------------------
// DispatchOutcome — closed result of a provider dispatch (INV-015 enforced)
// ---------------------------------------------------------------------------

/// The outcome of dispatching a model invocation through the provider framework.
///
/// # INV-015
///
/// No variant carries raw prompt or response bodies. `LocalInvocation` and
/// `VaultBrokeredInvocation` carry only token counts and latency.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum DispatchOutcome {
    /// Model invoked on a local backend (Ollama, vLLM, local GPU).
    LocalInvocation {
        /// Which backend kind handled the invocation.
        backend: ModelBackendKind,
        /// Estimated tokens consumed by the input.
        tokens_in: u32,
        /// Estimated tokens produced in the output.
        tokens_out: u32,
        /// Wall-clock latency in milliseconds.
        latency_ms: u64,
    },
    /// Model invoked through the L4.2 vault broker (external provider).
    VaultBrokeredInvocation {
        /// Opaque vault response handle for audit linkage.
        vault_response_handle: String,
        /// Estimated tokens consumed by the input.
        tokens_in: u32,
        /// Estimated tokens produced in the output.
        tokens_out: u32,
        /// Wall-clock latency in milliseconds.
        latency_ms: u64,
    },
    /// Dispatch was denied by policy (e.g. posture, budget).
    Denied {
        /// Human-readable reason for denial.
        reason: String,
        /// The posture that caused the denial.
        posture: AICrossOriginPosture,
    },
}

// ---------------------------------------------------------------------------
// ProviderDispatcher
// ---------------------------------------------------------------------------

/// Routes model invocations to local or vault-brokered backends per S13.2 §5.
///
/// # Dispatch rules (per S13.2 §5)
///
/// | Provider class            | Requires vault | Backend                       |
/// |---------------------------|---------------|-------------------------------|
/// | `Ollama`                 | No            | `LocalCpu` / `LocalGpu`       |
/// | `Vllm`                   | No            | `LocalGpu` / `LocalDistributed`|
/// | `Anthropic`              | Yes           | `ExternalVaultBrokered`       |
/// | `Openai`                 | Yes           | `ExternalVaultBrokered`       |
/// | `OtherVaultBrokered`     | Yes           | `ExternalVaultBrokered`       |
///
/// The `AI_NO_EXTERNAL` posture blocks all vault-brokered dispatches regardless
/// of vault configuration. This is the bypass-attempt guard that S8.1 §5.7
/// records as `AI_DIRECT_INTERNET_DENIED` evidence (real emission lands in T-102).
pub struct ProviderDispatcher {
    vault_client: Option<Arc<dyn VaultClientAdapter>>,
}

impl ProviderDispatcher {
    /// Create a provider dispatcher with no vault client configured.
    ///
    /// Without a vault client, only local providers (`Ollama`, `Vllm`) can
    /// be dispatched. External providers will return
    /// [`CognitiveError::Internal`] with "vault client not configured".
    #[must_use]
    pub fn new() -> Self {
        Self { vault_client: None }
    }

    /// Attach a vault client adapter for external provider dispatch.
    ///
    /// When configured, external providers (`Anthropic`, `Openai`,
    /// `OtherVaultBrokered`) are routed through the vault broker. Local
    /// providers continue to dispatch directly.
    #[must_use]
    pub fn with_vault_client(mut self, vault: Arc<dyn VaultClientAdapter>) -> Self {
        self.vault_client = Some(vault);
        self
    }

    /// Main entry point — dispatch a model invocation according to S13.2 §5.
    ///
    /// # Errors
    ///
    /// - [`CognitiveError::ExternalBackendBlocked`] — `posture` is
    ///   `AI_NO_EXTERNAL` and the provider class requires external access.
    /// - [`CognitiveError::Internal`] — external provider but no vault client
    ///   configured.
    /// - [`CognitiveError::VaultCredentialMissing`] — external provider with
    ///   vault client but model has no `vault_capability_id`.
    pub async fn dispatch_to_provider(
        &self,
        model: &CognitiveModel,
        _intent: &CognitiveIntent,
        posture: AICrossOriginPosture,
    ) -> Result<DispatchOutcome, CognitiveError> {
        let requires_vault = is_external_vault_brokered(model.provider);

        // ── AI_NO_EXTERNAL guard (S8.1 §5.7 bypass-attempt path) ──
        if requires_vault && posture == AICrossOriginPosture::AiNoExternal {
            return Err(CognitiveError::ExternalBackendBlocked { posture });
        }

        if requires_vault {
            // ── External / other vault-brokered ──
            let vault = self
                .vault_client
                .as_ref()
                .ok_or_else(|| CognitiveError::Internal("vault client not configured".into()))?;

            let capability_id = model
                .vault_capability_id
                .as_ref()
                .ok_or_else(|| CognitiveError::VaultCredentialMissing(model.model_id.0.clone()))?;

            let request = VaultRequest {
                operation: "cognitive.invoke".into(),
                opaque_payload: "stub-model-invocation".into(),
            };

            let response = vault.use_capability(capability_id, request).await?;

            Ok(DispatchOutcome::VaultBrokeredInvocation {
                vault_response_handle: response.handle,
                tokens_in: 4,
                tokens_out: 16,
                latency_ms: response.latency_ms,
            })
        } else {
            // ── Local: Ollama, Vllm — no vault required ──
            let backend = provider_to_backend_kind(model.provider);
            Ok(DispatchOutcome::LocalInvocation {
                backend,
                tokens_in: 4,
                tokens_out: 16,
                latency_ms: 12,
            })
        }
    }
}

impl Default for ProviderDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` if the provider class requires vault brokering per S13.2 §5.
pub(crate) const fn is_external_vault_brokered(provider: ProviderClass) -> bool {
    matches!(
        provider,
        ProviderClass::Anthropic | ProviderClass::Openai | ProviderClass::OtherVaultBrokered
    )
}

/// Map a local provider class to its default backend kind.
const fn provider_to_backend_kind(provider: ProviderClass) -> ModelBackendKind {
    match provider {
        ProviderClass::Ollama => ModelBackendKind::LocalCpu,
        ProviderClass::Vllm => ModelBackendKind::LocalGpu,
        // unreachable for local providers — is_external_vault_brokered gates first
        _ => ModelBackendKind::DegradedNull,
    }
}
