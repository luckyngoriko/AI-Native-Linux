//! `RealAdapterHandle` — concrete [`AdapterHandle`] backed by an
//! [`AdapterManifest`] (S10.1 §10.1).
//!
//! T-027 declared the [`AdapterHandle`] marker trait so the runtime could hold
//! adapter references without depending on T-028's full registry. T-028 lands
//! the real handle, which wraps an `Arc<AdapterManifest>` and surfaces:
//!
//! - the manifest's preferred [`ActionDispatchKind`] (§3.2) — the runtime
//!   composes this with subject `is_ai` and the policy decision's
//!   `Constraints.sandbox_profile_id` per §3.2's closed decision rule
//!   (T-029 will land the composition logic);
//! - the underlying manifest itself via [`RealAdapterHandle::manifest`] —
//!   the dispatcher in T-029 reads `declared_actions` to pick the per-action
//!   `rollback_strategy`, `target_schema`, `template_string`, and
//!   `timeout_seconds` overrides.
//!
//! The handle is reference-counted (`Arc<AdapterManifest>`) so the registry
//! and the dispatcher can hold the same manifest without cloning the
//! per-action declarations on every dispatch.

use std::sync::Arc;

use crate::adapter_manifest::AdapterManifest;
use crate::dispatch::ActionDispatchKind;
use crate::runtime::AdapterHandle;

/// Concrete [`AdapterHandle`] backed by a registered [`AdapterManifest`].
///
/// Cloning is cheap (`Arc<AdapterManifest>` clone bumps the refcount). The
/// handle is `Send + Sync` because [`AdapterManifest`] is composed of
/// `Send + Sync` types only (`String`, `chrono::DateTime<Utc>`, closed enums,
/// `serde_json::Value`).
#[derive(Debug, Clone)]
pub struct RealAdapterHandle {
    /// The manifest this handle wraps. Held behind `Arc` so the registry
    /// and the dispatcher can share it without per-dispatch clones of the
    /// per-action declarations.
    manifest: Arc<AdapterManifest>,
}

impl RealAdapterHandle {
    /// Wrap a manifest in a real handle.
    ///
    /// Consumes `Arc<AdapterManifest>` rather than `AdapterManifest` so the
    /// caller (the registry) can keep its own `Arc` to the same manifest
    /// for the `list()` / `lookup_by_id` paths without a second clone of
    /// the per-action declarations.
    #[must_use]
    pub const fn new(manifest: Arc<AdapterManifest>) -> Self {
        Self { manifest }
    }

    /// Borrow the wrapped manifest. T-029's dispatcher reads
    /// `declared_actions`, `default_adapter_timeout_seconds`, and
    /// `default_sandbox_profile_id` through this accessor.
    #[must_use]
    pub fn manifest(&self) -> &AdapterManifest {
        &self.manifest
    }
}

impl AdapterHandle for RealAdapterHandle {
    fn dispatch_kind(&self) -> ActionDispatchKind {
        self.manifest.dispatch_kind
    }
}
