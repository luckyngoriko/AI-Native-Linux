//! `InMemoryAdapterRegistry` — S10.1 §10 adapter manifest store.
//!
//! Each adapter is registered through a typed [`AdapterManifest`].
//! Registration is itself a typed action (`runtime.adapter.register`) that
//! flows through the runtime; this in-memory registry models the L3-internal
//! store the registration action terminates against.
//!
//! ## Responsibilities (T-028 surface)
//!
//! 1. **Signature verification (§10.2).** Every manifest is signed by an
//!    Ed25519 key recognised by the AIOS root or a recognised publisher; the
//!    trust chain mirrors the M3 policy bundle trust chain (S2.3 §12.3). The
//!    registry holds a `HashMap<signing_key_id → VerifyingKey>` and rejects
//!    any registration whose `signing_key_id` is not in the trust store
//!    ([`RuntimeError::AdapterUnknownAuthority`]) or whose `adapter_signature`
//!    does not verify over the manifest's canonical signed body
//!    ([`RuntimeError::AdapterSignatureInvalid`]).
//!
//! 2. **`adapter_id` uniqueness.** A second registration attempting the same
//!    `adapter_id` is rejected with
//!    [`RuntimeError::AdapterAlreadyRegistered`]. Operators rotate a manifest
//!    by submitting `runtime.adapter.register` with the same `adapter_id`
//!    and a fresh signature; the in-memory registry models a single live
//!    registration per id, so a rotation requires a fresh registry instance
//!    (production rotation orchestration is queued — see §10.4).
//!
//! 3. **Stability ladder enforcement (§3.4).** A [`RegisteredAdapter`] whose
//!    `declared_stability` is [`AdapterStability::Retired`] is **never**
//!    returned by [`InMemoryAdapterRegistry::lookup_by_id`] or
//!    [`InMemoryAdapterRegistry::lookup_for_target`]. The §3.4 ladder closes
//!    on `RETIRED` ("no new dispatches accepted") and the spec does not
//!    declare a `REMOVED` variant — `RETIRED` is the `FAIL_CLOSED` terminus.
//!    `DEPRECATED` adapters remain dispatchable; the runtime emits an
//!    `ADAPTER_DEPRECATED_DISPATCH` evidence record on every call (T-031
//!    wires the emission). `EXPERIMENTAL` and `REGISTERED` adapters are
//!    also dispatchable; the §3.2 decision rule (T-029) downgrades them
//!    away from `IN_PROCESS_RPC` at dispatch time.
//!
//! 4. **Action-kind discovery.** [`InMemoryAdapterRegistry::lookup_for_target`]
//!    scans the declared `action_kind`s across every registered manifest
//!    and returns the first match whose stability is not `RETIRED`. The
//!    §10.5 action-kind exclusivity rule guarantees at most one match per
//!    kind; T-028 does **not** yet enforce the exclusivity (a future task
//!    will reject a second registration that re-declares an existing
//!    `action_kind` with `ADAPTER_KIND_COLLISION` — left intentionally
//!    out of T-028 because the spec routes the enforcement through the
//!    `runtime.adapter.register` typed action, not through the registry
//!    primitive).
//!
//! ## Out of scope (queued)
//!
//! - The `manifest_expires_at` watchdog that auto-de-registers expired
//!   manifests with `reason = MANIFEST_EXPIRED` (§10.4). Expiry is recorded
//!   on the manifest but not enforced here; T-029 / T-035 wire the
//!   timer-driven de-registration path.
//! - The §10.5 action-kind exclusivity collision check.
//! - The §10.3 stability promotion path (`runtime.adapter.set_stability`).
//! - Publisher endorsement domain checks (§10.2 step 2: "Publisher must be
//!   endorsed for the adapter's domain"). The current trust store is flat
//!   (no domain scoping); production trust chains are queued.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Serialize;
use tokio::sync::RwLock;

use crate::adapter_handle::RealAdapterHandle;
use crate::adapter_manifest::{AdapterActionDeclaration, AdapterManifest};
use crate::dispatch::{ActionDispatchKind, AdapterIOMode, AdapterStability};
use crate::error::RuntimeError;
use crate::runtime::{AdapterHandle, AdapterRegistry};

// ---------------------------------------------------------------------------
// Canonical signed-body view (S10.1 §10.2 — Ed25519 over fields 1..11).
// ---------------------------------------------------------------------------

/// View of [`AdapterManifest`] containing only the fields 1..11 that the
/// publisher signs (S10.1 §10.1 / §10.2). Fields 12 (`adapter_signature`),
/// 13 (`signing_key_id`), 14 (`manifest_created_at`), and 15
/// (`manifest_expires_at`) are intentionally excluded — they are signature
/// metadata, not signed payload.
///
/// Field order MUST match [`AdapterManifest`] (fields 1..11) verbatim; a
/// re-ordering silently breaks every adapter signature in the wild.
///
/// `serde_json` preserves struct field declaration order and no map types are
/// involved, so the output is byte-deterministic across processes and
/// architectures — the same determinism contract the policy bundle loader
/// observes (S2.3 §13.1).
#[derive(Debug, Serialize)]
struct SignedManifestBody<'a> {
    adapter_id: &'a str,
    adapter_version: &'a str,
    vendor: &'a str,
    name: &'a str,
    declared_stability: &'a AdapterStability,
    io_mode: &'a AdapterIOMode,
    dispatch_kind: &'a ActionDispatchKind,
    declared_actions: &'a [AdapterActionDeclaration],
    declared_invariants_supported: &'a [String],
    default_adapter_timeout_seconds: u32,
    default_sandbox_profile_id: &'a str,
}

impl<'a> From<&'a AdapterManifest> for SignedManifestBody<'a> {
    fn from(m: &'a AdapterManifest) -> Self {
        Self {
            adapter_id: &m.adapter_id,
            adapter_version: &m.adapter_version,
            vendor: &m.vendor,
            name: &m.name,
            declared_stability: &m.declared_stability,
            io_mode: &m.io_mode,
            dispatch_kind: &m.dispatch_kind,
            declared_actions: &m.declared_actions,
            declared_invariants_supported: &m.declared_invariants_supported,
            default_adapter_timeout_seconds: m.default_adapter_timeout_seconds,
            default_sandbox_profile_id: &m.default_sandbox_profile_id,
        }
    }
}

/// Render the canonical signed body bytes for a manifest. The bytes returned
/// are the **exact** input the publisher key is expected to have signed; the
/// registry feeds these into the Ed25519 verifier.
///
/// Test code constructs the same body and signs it with
/// `SigningKey::sign(canonical_signed_manifest_bytes(&manifest))`.
///
/// # Errors
///
/// Returns [`RuntimeError::ManifestInvalid`] if `serde_json` serialisation
/// fails (this can only happen in pathological cases — e.g. non-UTF-8
/// payloads — that are excluded by the struct field types).
pub fn canonical_signed_manifest_bytes(
    manifest: &AdapterManifest,
) -> Result<Vec<u8>, RuntimeError> {
    let body = SignedManifestBody::from(manifest);
    serde_json::to_vec(&body)
        .map_err(|e| RuntimeError::ManifestInvalid(format!("signed-body serialise: {e}")))
}

// ---------------------------------------------------------------------------
// RegisteredAdapter.
// ---------------------------------------------------------------------------

/// A manifest that has been signature-verified and committed to the registry.
///
/// Carries the original [`AdapterManifest`] verbatim plus the wall-clock at
/// which the registry accepted it (`registered_at`). T-031's evidence
/// emission will read this timestamp for the `ADAPTER_REGISTERED` record;
/// T-029's dispatcher reads the manifest for per-action declarations.
///
/// Cloning copies the manifest field-by-field; for hot paths the registry
/// itself returns `Arc<AdapterManifest>` through [`RealAdapterHandle`].
#[derive(Debug, Clone)]
pub struct RegisteredAdapter {
    /// The verified manifest. Verbatim; the registry never mutates it.
    pub manifest: AdapterManifest,
    /// Wall-clock at which the registry committed the manifest. Distinct
    /// from `manifest.manifest_created_at` (issuance time) and from
    /// `manifest.manifest_expires_at` (revocation horizon); `registered_at`
    /// is the registry's own ingest timestamp.
    pub registered_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// InMemoryAdapterRegistry.
// ---------------------------------------------------------------------------

/// In-process [`AdapterRegistry`] implementation backed by a `HashMap` keyed
/// by `adapter_id`.
///
/// `tokio::sync::RwLock` guards the map so concurrent reads (the dispatcher
/// hot path) do not contend with each other; writes (registration) take the
/// write lock briefly. The lock surface is async-compatible because the
/// registry will eventually live behind the gRPC server which itself runs on
/// the tokio runtime; the synchronous [`AdapterRegistry::lookup`] trait
/// method uses `try_read` to avoid blocking the runtime when called from a
/// non-async context.
///
/// The trust store is a flat `HashMap<signing_key_id → VerifyingKey>`. The
/// spec's full §10.2 trust chain (AIOS root → publisher → manifest with
/// per-domain endorsement) is queued; T-028 ships the publisher-key step
/// only, identical in shape to the M3 policy bundle loader's
/// `trusted_authorities` map.
#[derive(Debug)]
pub struct InMemoryAdapterRegistry {
    /// Trust store. Keyed by `signing_key_id`; the value is the publisher
    /// verifying key. A registration whose `signing_key_id` is not present
    /// is rejected with [`RuntimeError::AdapterUnknownAuthority`].
    trusted_authorities: HashMap<String, VerifyingKey>,
    /// Registered adapters keyed by `adapter_id`. Held behind `RwLock` for
    /// concurrent read access; behind `Arc` (on the value side) so the
    /// dispatcher can hold the manifest across an `await` without keeping
    /// the lock.
    adapters: RwLock<HashMap<String, Arc<RegisteredAdapter>>>,
}

impl InMemoryAdapterRegistry {
    /// Construct a fresh registry with the given trust store.
    ///
    /// The trust store is immutable for the lifetime of the registry;
    /// rotating a publisher key requires building a new registry. This
    /// matches the M3 policy bundle loader discipline.
    #[must_use]
    pub fn new(trusted: HashMap<String, VerifyingKey>) -> Self {
        Self {
            trusted_authorities: trusted,
            adapters: RwLock::new(HashMap::new()),
        }
    }

    /// Construct an empty registry with **no** trust authorities.
    ///
    /// Every registration against such a registry will fail closed with
    /// [`RuntimeError::AdapterUnknownAuthority`]. Useful for tests that
    /// exercise the trust-store negative paths without needing keypair
    /// fixtures.
    #[must_use]
    pub fn empty() -> Self {
        Self::new(HashMap::new())
    }

    /// Returns `true` if the registry's trust store contains the named
    /// authority.
    #[must_use]
    pub fn trusts_authority(&self, signing_key_id: &str) -> bool {
        self.trusted_authorities.contains_key(signing_key_id)
    }

    /// Register a signed [`AdapterManifest`].
    ///
    /// Performs (in order):
    /// 1. Authority lookup — `manifest.signing_key_id` must be in the trust
    ///    store. Miss → [`RuntimeError::AdapterUnknownAuthority`].
    /// 2. Signature decode — `manifest.adapter_signature` is `hex_lower`
    ///    over a 64-byte Ed25519 signature.
    /// 3. Signature verify — Ed25519 over the canonical signed-body bytes
    ///    (`SignedManifestBody`: fields 1..11). Failure →
    ///    [`RuntimeError::AdapterSignatureInvalid`].
    /// 4. `adapter_id` uniqueness — duplicate →
    ///    [`RuntimeError::AdapterAlreadyRegistered`].
    /// 5. Commit — insert behind `Arc` into the adapter map; stamp
    ///    `registered_at = now`.
    ///
    /// # Errors
    ///
    /// See the variant list above. Every error is returned **before** the
    /// adapter map is mutated; a failed `register` leaves the registry
    /// unchanged (`FAIL_CLOSED`).
    pub async fn register(
        &self,
        manifest: AdapterManifest,
        now: DateTime<Utc>,
    ) -> Result<(), RuntimeError> {
        // 1. Authority lookup.
        let verifying_key = self
            .trusted_authorities
            .get(&manifest.signing_key_id)
            .ok_or_else(|| {
                RuntimeError::AdapterUnknownAuthority(manifest.signing_key_id.clone())
            })?;

        // 2. Signature decode (hex_lower per §10.1 field 12).
        let sig_bytes = decode_hex_signature(&manifest.adapter_signature)
            .ok_or(RuntimeError::AdapterSignatureInvalid)?;
        let signature = Signature::from_bytes(&sig_bytes);

        // 3. Signature verify over canonical signed body (fields 1..11).
        let body = canonical_signed_manifest_bytes(&manifest)?;
        verifying_key
            .verify(&body, &signature)
            .map_err(|_| RuntimeError::AdapterSignatureInvalid)?;

        // 4. Uniqueness check + 5. commit, atomic under the write lock.
        let mut guard = self.adapters.write().await;
        if guard.contains_key(&manifest.adapter_id) {
            let id = manifest.adapter_id.clone();
            drop(guard);
            return Err(RuntimeError::AdapterAlreadyRegistered(id));
        }
        let key = manifest.adapter_id.clone();
        guard.insert(
            key,
            Arc::new(RegisteredAdapter {
                manifest,
                registered_at: now,
            }),
        );
        drop(guard);
        Ok(())
    }

    /// Look up a registered adapter by its `adapter_id`.
    ///
    /// Returns `None` when:
    /// - no adapter with that id is registered; **or**
    /// - the adapter exists but its `declared_stability` is
    ///   [`AdapterStability::Retired`] (§3.4 `FAIL_CLOSED` — "no new
    ///   dispatches accepted").
    ///
    /// Retired adapters remain visible via [`Self::list`] for forensic /
    /// `ListAdapters` purposes (S10.1 §5.1 — the RPC still surfaces retired
    /// adapters); only dispatch-bound lookups close on retired.
    pub async fn lookup_by_id(&self, adapter_id: &str) -> Option<RegisteredAdapter> {
        let guard = self.adapters.read().await;
        let candidate = guard.get(adapter_id).and_then(|r| {
            if r.manifest.declared_stability == AdapterStability::Retired {
                None
            } else {
                Some((**r).clone())
            }
        });
        drop(guard);
        candidate
    }

    /// Look up an adapter that declares the given `action_kind` target.
    ///
    /// Scans every registered manifest's `declared_actions` and returns the
    /// first whose stability is not [`AdapterStability::Retired`]. The
    /// §10.5 exclusivity rule guarantees the result is unique when present.
    ///
    /// Returns `None` when no live adapter declares the kind (`FAIL_CLOSED` —
    /// the runtime emits `UNKNOWN_ACTION_KIND` evidence at the call site).
    pub async fn lookup_for_target(&self, target: &str) -> Option<RegisteredAdapter> {
        let guard = self.adapters.read().await;
        let found = guard
            .values()
            .find(|r| {
                r.manifest.declared_stability != AdapterStability::Retired
                    && r.manifest
                        .declared_actions
                        .iter()
                        .any(|d| d.action_kind == target)
            })
            .map(|r| (**r).clone());
        drop(guard);
        found
    }

    /// Return every registered adapter (including retired) for the
    /// `ListAdapters` RPC surface (T-033).
    ///
    /// Order is unspecified — `HashMap` iteration order is non-deterministic
    /// by design. T-033 sorts results for a deterministic wire ordering.
    pub async fn list(&self) -> Vec<RegisteredAdapter> {
        let guard = self.adapters.read().await;
        let snapshot: Vec<RegisteredAdapter> = guard.values().map(|r| (**r).clone()).collect();
        drop(guard);
        snapshot
    }

    /// Count of currently registered adapters (retired included). Useful
    /// for tests that assert no double-registration leaks.
    pub async fn len(&self) -> usize {
        self.adapters.read().await.len()
    }

    /// `true` iff no adapter is currently registered.
    pub async fn is_empty(&self) -> bool {
        self.adapters.read().await.is_empty()
    }

    /// T-035 — §10.4 manifest-expiry watchdog hook.
    ///
    /// Prunes every adapter whose `manifest.manifest_expires_at` is at or
    /// before `now`. Returns the number of adapters de-registered. The
    /// spec models this as an automatic background watchdog; production
    /// wiring (M5+) schedules this call from an L9 admin operations
    /// timer. The runtime exposes the primitive here so the watchdog is
    /// testable and operator-driven `prune_expired` calls (e.g. on a
    /// manual force-rotate) work without an extra scheduler.
    ///
    /// Pruning is `FAIL_CLOSED` for forensic purposes: a manifest whose
    /// expiry has passed is dropped from the registry; the
    /// `ADAPTER_DEREGISTERED` evidence with `reason = MANIFEST_EXPIRED`
    /// is emitted by the caller (the watchdog scheduler), not by this
    /// primitive — the primitive's contract is just the de-registration.
    pub async fn prune_expired(&self, now: DateTime<Utc>) -> usize {
        let mut guard = self.adapters.write().await;
        let before = guard.len();
        guard.retain(|_, reg| reg.manifest.manifest_expires_at > now);
        let after = guard.len();
        drop(guard);
        before - after
    }
}

impl AdapterRegistry for InMemoryAdapterRegistry {
    fn lookup(&self, action_kind: &str) -> Option<Arc<dyn AdapterHandle>> {
        // The trait surface is synchronous (T-027 decision — see
        // `runtime::AdapterRegistry`). Use `try_read` to avoid blocking the
        // tokio runtime when the caller is on an executor thread; if the
        // write lock is held (a concurrent registration) the caller observes
        // `None` and may retry. This matches the dispatcher's `FAIL_CLOSED`
        // discipline: a momentarily-unreadable registry is observationally
        // equivalent to "adapter not registered" and the runtime emits an
        // `AdapterUnknown` failure rather than blocking.
        let guard = self.adapters.try_read().ok()?;
        let manifest = guard
            .values()
            .find(|r| {
                r.manifest.declared_stability != AdapterStability::Retired
                    && r.manifest
                        .declared_actions
                        .iter()
                        .any(|d| d.action_kind == action_kind)
            })
            .map(|r| Arc::new(r.manifest.clone()));
        drop(guard);
        manifest.map(|m| {
            let handle: Arc<dyn AdapterHandle> = Arc::new(RealAdapterHandle::new(m));
            handle
        })
    }
}

// ---------------------------------------------------------------------------
// Hex signature decoder (lower-hex per §10.1 field 12).
// ---------------------------------------------------------------------------

/// Decode a lower-hex 128-character string into a 64-byte Ed25519 signature.
/// Returns `None` on any non-hex character, wrong length, or odd-length input.
fn decode_hex_signature(hex_lower: &str) -> Option<[u8; 64]> {
    if hex_lower.len() != 128 {
        return None;
    }
    let mut out = [0_u8; 64];
    for (i, chunk) in hex_lower.as_bytes().chunks_exact(2).enumerate() {
        let hi = hex_nibble(chunk[0])?;
        let lo = hex_nibble(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

/// Decode one lower-hex digit. Rejects upper-case to enforce the §10.1
/// `hex_lower` discipline.
const fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// Inverse of [`decode_hex_signature`]: encode a 64-byte signature as a
/// 128-character lower-hex string. Used by the test fixtures to produce the
/// `adapter_signature` field on a freshly-signed manifest.
#[must_use]
pub fn encode_hex_signature(sig: &[u8; 64]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(128);
    for byte in sig {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip_zero() {
        let zero = [0_u8; 64];
        let enc = encode_hex_signature(&zero);
        assert_eq!(enc.len(), 128);
        assert_eq!(decode_hex_signature(&enc), Some(zero));
    }

    #[test]
    fn hex_roundtrip_arbitrary() {
        let mut sig = [0_u8; 64];
        for (i, b) in sig.iter_mut().enumerate() {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "deterministic 64-byte buffer; the u8 wrap is intentional test fixture noise"
            )]
            let v = (i as u8).wrapping_mul(37).wrapping_add(11);
            *b = v;
        }
        let enc = encode_hex_signature(&sig);
        assert_eq!(decode_hex_signature(&enc), Some(sig));
    }

    #[test]
    fn hex_rejects_short() {
        assert_eq!(decode_hex_signature("abcd"), None);
    }

    #[test]
    fn hex_rejects_upper() {
        let mut s = String::from("0").repeat(128);
        // Inject one upper-case A — §10.1 mandates lower-hex.
        s.replace_range(0..1, "A");
        assert_eq!(decode_hex_signature(&s), None);
    }

    #[test]
    fn hex_rejects_non_hex() {
        let s = "z".repeat(128);
        assert_eq!(decode_hex_signature(&s), None);
    }
}
