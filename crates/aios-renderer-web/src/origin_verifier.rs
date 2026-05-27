//! Per-origin verifier for signed iframe composition (S7.5 INV I4).
//!
//! Every group gets a unique origin token. The `OriginVerifier` registers per-group
//! iframe origin bindings and rejects composition attempts where the presented origin
//! token does not match the bound `group_id`.

use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::error::WebRendererError;
use crate::origin::OriginScheme;
use crate::types::WebSurfaceId;

/// An iframe origin binding registered with the origin verifier (INV I4).
///
/// Each binding ties an iframe origin to a specific group id. The
/// `scope_binding_evidence_id` links back to the S0.1 `ScopeBinding` evidence
/// receipt that authorized this registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IframeOriginBinding {
    /// The origin string of the iframe (e.g. `"https://acme-app.aios.localhost:8443"`).
    pub iframe_origin: String,
    /// The surface identifier for this iframe.
    pub surface_id: WebSurfaceId,
    /// The group identifier that owns this origin (must match the `AppOrigin` token).
    pub bound_group_id: String,
    /// The S0.1 `ScopeBinding` evidence receipt id that authorized this binding.
    pub scope_binding_evidence_id: String,
}

/// Per-origin verifier enforcing INV I4 group-level origin binding.
///
/// Before a composition (iframe display) is admitted, the verifier checks that
/// the presented origin matches the bound group id registered for that surface.
pub struct OriginVerifier {
    bindings: RwLock<HashMap<WebSurfaceId, IframeOriginBinding>>,
}

impl OriginVerifier {
    /// Create a new empty origin verifier.
    #[must_use]
    pub fn new() -> Self {
        Self {
            bindings: RwLock::new(HashMap::new()),
        }
    }

    /// Register an iframe origin binding.
    ///
    /// Parses `iframe_origin` via `OriginScheme::parse`, ensures the parsed scheme
    /// is `AppOrigin(token)`, and verifies the token matches `bound_group_id`.
    ///
    /// # Errors
    ///
    /// Returns `OriginVerificationFailed` if the parsed origin is not an
    /// `AppOrigin` or the token does not match `bound_group_id`.
    pub async fn register_binding(
        &self,
        binding: IframeOriginBinding,
    ) -> Result<(), WebRendererError> {
        let parsed = OriginScheme::parse(&binding.iframe_origin)?;
        match &parsed.scheme {
            OriginScheme::AppOrigin(token) => {
                if token.0 != binding.bound_group_id {
                    return Err(WebRendererError::OriginVerificationFailed {
                        expected_group_id: binding.bound_group_id,
                        presented_origin: binding.iframe_origin.clone(),
                    });
                }
            }
            _ => {
                return Err(WebRendererError::OriginVerificationFailed {
                    expected_group_id: binding.bound_group_id,
                    presented_origin: binding.iframe_origin.clone(),
                });
            }
        }
        self.bindings
            .write()
            .await
            .insert(binding.surface_id.clone(), binding);
        Ok(())
    }

    /// Verify that a composition's presented origin matches the registered binding.
    ///
    /// # Errors
    ///
    /// Returns `OriginVerificationFailed` if no binding is registered for
    /// `surface_id` or the presented origin does not match the bound origin.
    pub async fn verify_composition(
        &self,
        surface_id: &WebSurfaceId,
        presented_origin: &str,
    ) -> Result<(), WebRendererError> {
        let binding = self
            .bindings
            .read()
            .await
            .get(surface_id)
            .cloned()
            .ok_or_else(|| WebRendererError::OriginVerificationFailed {
                expected_group_id: String::new(),
                presented_origin: presented_origin.to_string(),
            })?;
        if presented_origin != binding.iframe_origin {
            return Err(WebRendererError::OriginVerificationFailed {
                expected_group_id: binding.bound_group_id,
                presented_origin: presented_origin.to_string(),
            });
        }
        Ok(())
    }

    /// Revoke a previously registered binding.
    ///
    /// # Errors
    ///
    /// Returns `OriginVerificationFailed` if no binding exists for `surface_id`.
    pub async fn revoke_binding(&self, surface_id: &WebSurfaceId) -> Result<(), WebRendererError> {
        let mut guard = self.bindings.write().await;
        guard.remove(surface_id).map(|_| ()).ok_or_else(|| {
            WebRendererError::OriginVerificationFailed {
                expected_group_id: String::new(),
                presented_origin: surface_id.to_string(),
            }
        })
    }

    /// List all currently registered bindings.
    pub async fn list_bindings(&self) -> Vec<IframeOriginBinding> {
        let guard = self.bindings.read().await;
        guard.values().cloned().collect()
    }
}

impl Default for OriginVerifier {
    fn default() -> Self {
        Self::new()
    }
}
