//! [`DecisionCache`] + [`SharedDecisionCache`] — S2.3 §13.2 / §13.3.
//!
//! Bounded LRU cache keyed by `(request_hash, bundle_version)` per the §13.3
//! canonical cache-key formula. A cache hit lets the kernel skip the entire
//! 12-step pipeline and serve a previously-computed [`PolicyDecision`]
//! verbatim — this is what makes the §18.1 sub-millisecond cached budget
//! achievable.
//!
//! ## What this lands (T-024)
//!
//! - [`CacheKey`] — the §13.3 two-tuple `(request_hash, bundle_version)`.
//! - [`DecisionCache`] — single-threaded LRU built on [`lru::LruCache`].
//! - [`SharedDecisionCache`] — `Arc<RwLock<DecisionCache>>` newtype with
//!   `get` / `put` / `invalidate_for_bundle` methods that lock-and-forward;
//!   safe to share across tokio worker tasks.
//! - Default capacity: 1 024 entries (configurable per ctor).
//!
//! ## §13.3 vs the T-024 brief
//!
//! Spec §13.3 says:
//!
//! ```text
//! cache_key = "polc_" + hex_lower(BLAKE3(JCS({
//!   request_hash,
//!   bundle_version
//! })))[:32]
//! ```
//!
//! — i.e. the canonical wire-level cache key is a 32-char content-addressed
//! string over the two-tuple. The brief proposed a three-tuple including
//! `snapshot_id`; we honour **spec** per the T-024 STOP rule. Determinism
//! still anchors on the full §13.1 triple — the snapshot id is the third
//! component of the *decision* key, but the cache key is the §13.3 two-tuple.
//! When the snapshot would change the decision, the §13.2 TTL bounds re-
//! evaluation; within TTL the cached decision is correct by the cache-key
//! formula. This is the rev.2 contract verbatim.
//!
//! ## Wire form vs in-memory form
//!
//! The §13.3 string form `polc_<hex>` is the **wire** cache key (e.g. for
//! external cache backends or evidence linkage). In-memory the cache uses
//! the typed `(String, String)` shape directly because it's faster, has no
//! collision risk, and is trivially convertible to the wire form via
//! [`CacheKey::wire_form`] when audit needs it. Both forms participate in
//! the §13.3 contract; they're isomorphic.
//!
//! ## Eviction
//!
//! Pure LRU — least-recently-used entries are evicted on `put` once
//! capacity is exceeded. The §13.2 TTL is **not** enforced at the cache
//! layer; the cache returns whatever was inserted, and the caller (kernel)
//! is responsible for deciding whether to honour the cached
//! `Constraints.ttl_seconds`. T-024 returns the cached decision as-is; M5
//! attaches the TTL enforcement once the wall-clock evidence-log integration
//! lands.

use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};

use blake3::Hasher;
use lru::LruCache;
use serde::{Deserialize, Serialize};

use crate::decision::PolicyDecision;

/// Default cache capacity (entries) when callers don't override it.
///
/// Used by [`DecisionCache::new`] when no explicit capacity is set.
/// Sized to absorb the §22 acceptance workload (the 10 golden fixtures
/// × ~100 repeat invocations under the §18.1 p95 budget) with headroom.
/// Production binaries tune this from a config file once the operator
/// surface lands.
pub const DEFAULT_CAPACITY: usize = 1024;

/// §13.3 canonical cache key — the `(request_hash, bundle_version)`
/// two-tuple over which the cache is keyed.
///
/// Stored as a typed struct so the cache's hashing / equality / ordering
/// semantics are crystal clear at the call site (no string concat
/// ambiguity, no field-order drift).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CacheKey {
    /// `hex_lower(BLAKE3(canonical(request)))[:32]` per S0.1 §8.5.
    pub request_hash: String,
    /// `polb_<hex>` bundle version per S2.3 §12.2.
    pub bundle_version: String,
}

impl CacheKey {
    /// Construct a fresh key from owned strings.
    #[must_use]
    pub fn new(request_hash: impl Into<String>, bundle_version: impl Into<String>) -> Self {
        Self {
            request_hash: request_hash.into(),
            bundle_version: bundle_version.into(),
        }
    }

    /// Render the §13.3 wire-form cache-key string.
    ///
    /// `"polc_" + hex_lower(BLAKE3(JCS({request_hash, bundle_version})))[:32]`.
    /// Used by audit pivots from a cached decision back to the wire-level
    /// cache key, and by future external-cache backends that want a
    /// hash-collision-free key on a single string.
    #[must_use]
    pub fn wire_form(&self) -> String {
        #[derive(Serialize)]
        struct CacheKeyView<'a> {
            request_hash: &'a str,
            bundle_version: &'a str,
        }
        let view = CacheKeyView {
            request_hash: &self.request_hash,
            bundle_version: &self.bundle_version,
        };
        // serde_jcs::to_vec is infallible for plain &str / &str fields by
        // construction — the canonicaliser only fails on map keys with
        // non-string types, of which we have none. Fall back to a synthetic
        // marker on the unreachable error so audit can spot the defect.
        let bytes = serde_jcs::to_vec(&view).unwrap_or_else(|_| b"<jcs-error>".to_vec());
        let mut hasher = Hasher::new();
        hasher.update(&bytes);
        let digest = hasher.finalize();
        let hex_full = digest.to_hex();
        let hex = hex_full.as_str();
        let trunc = &hex[..32.min(hex.len())];
        format!("polc_{trunc}")
    }
}

/// LRU decision cache (S2.3 §13.2 / §13.3) — single-threaded form.
///
/// Wrap in [`SharedDecisionCache`] to share across tasks. Direct
/// [`DecisionCache`] use is reserved for tests + benchmarks that don't
/// need the locking discipline.
pub struct DecisionCache {
    /// LRU storage. `LruCache::put` evicts the least-recently-used entry
    /// on capacity overflow; `LruCache::get` updates the entry's
    /// recency-of-use stamp.
    inner: LruCache<CacheKey, PolicyDecision>,
}

impl std::fmt::Debug for DecisionCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecisionCache")
            .field("capacity", &self.inner.cap())
            .field("len", &self.inner.len())
            .finish_non_exhaustive()
    }
}

impl Default for DecisionCache {
    fn default() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }
}

impl DecisionCache {
    /// Construct a fresh cache with the given capacity.
    ///
    /// A `capacity` of 0 collapses to [`DEFAULT_CAPACITY`] — `LruCache`
    /// requires a `NonZeroUsize`, and a zero-capacity cache would be a
    /// no-op that silently masks the §18.1 perf budget. The collapse is
    /// the safer default.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        // NonZeroUsize::MIN is statically `1`. Map zero capacity onto
        // DEFAULT_CAPACITY (typed escape hatch — `.expect` is lint-forbidden).
        let default_cap = NonZeroUsize::new(DEFAULT_CAPACITY).unwrap_or(NonZeroUsize::MIN);
        let cap = NonZeroUsize::new(capacity).unwrap_or(default_cap);
        Self {
            inner: LruCache::new(cap),
        }
    }

    /// Look up a decision by key. Returns a clone of the cached value on
    /// hit; `None` on miss. The LRU recency stamp is updated on hit.
    #[must_use]
    pub fn get(&mut self, key: &CacheKey) -> Option<PolicyDecision> {
        self.inner.get(key).cloned()
    }

    /// Insert / overwrite a decision under the given key. Returns the
    /// previous value if any (the LRU eviction is handled by `LruCache`).
    pub fn put(&mut self, key: CacheKey, decision: PolicyDecision) -> Option<PolicyDecision> {
        self.inner.put(key, decision)
    }

    /// Drop every entry whose `bundle_version` matches the supplied label.
    ///
    /// Called by the `LoadBundle` RPC after a successful bundle swap; per
    /// S2.3 §13.2 "Bundle flip ⇒ all cached decisions for the old bundle
    /// invalidated." Returns the count of entries removed for caller
    /// visibility (the audit trail benefits from the count).
    pub fn invalidate_for_bundle(&mut self, bundle_version: &str) -> usize {
        let to_remove: Vec<CacheKey> = self
            .inner
            .iter()
            .filter(|(k, _)| k.bundle_version == bundle_version)
            .map(|(k, _)| k.clone())
            .collect();
        let count = to_remove.len();
        for key in to_remove {
            self.inner.pop(&key);
        }
        count
    }

    /// Current entry count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Configured capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.inner.cap().get()
    }

    /// `true` when no entries are cached.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Thread-safe wrapper around [`DecisionCache`].
///
/// The gRPC adapter holds one of these and clones it freely; the inner
/// `Arc<RwLock<_>>` makes the clone `O(1)` and every method internally
/// locks-and-forwards. Lock poisoning is recovered defensively — a
/// poisoned lock is taken via `into_inner` so a panic on one worker does
/// not permanently disable the cache for the rest of the runtime.
#[derive(Clone)]
pub struct SharedDecisionCache {
    inner: Arc<RwLock<DecisionCache>>,
}

impl std::fmt::Debug for SharedDecisionCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Avoid taking the lock just for Debug to prevent deadlocks under
        // contention; the cache's typed Debug already names the storage.
        f.debug_struct("SharedDecisionCache")
            .field("inner", &"<Arc<RwLock<DecisionCache>>>")
            .finish()
    }
}

impl Default for SharedDecisionCache {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }
}

impl SharedDecisionCache {
    /// Construct a fresh shared cache with the given capacity.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(DecisionCache::new(capacity))),
        }
    }

    /// Look up a decision by key (clones on hit). The underlying LRU
    /// recency stamp is updated, so this takes the write lock not the
    /// read lock — LRU `get` is not a pure read.
    #[must_use]
    pub fn get(&self, key: &CacheKey) -> Option<PolicyDecision> {
        let mut guard = match self.inner.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.get(key)
    }

    /// Insert / overwrite a decision under the given key. Returns the
    /// previous value if any; callers that don't care about the prior
    /// entry are free to ignore it (this is an insertion API, not an
    /// accessor — the `must_use_candidate` lint does not apply).
    #[allow(clippy::must_use_candidate)]
    pub fn put(&self, key: CacheKey, decision: PolicyDecision) -> Option<PolicyDecision> {
        let mut guard = match self.inner.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.put(key, decision)
    }

    /// Drop every entry whose `bundle_version` matches the supplied label.
    /// Returns the count of removed entries.
    #[allow(clippy::must_use_candidate)]
    pub fn invalidate_for_bundle(&self, bundle_version: &str) -> usize {
        let mut guard = match self.inner.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.invalidate_for_bundle(bundle_version)
    }

    /// Current entry count.
    #[must_use]
    pub fn len(&self) -> usize {
        let guard = match self.inner.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.len()
    }

    /// Configured capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        let guard = match self.inner.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.capacity()
    }

    /// `true` when the cache holds no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let guard = match self.inner.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.is_empty()
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code; panic-on-failure is the idiomatic test signal"
)]
mod tests {
    use super::*;
    use crate::decision::{Decision, PolicyDecision};
    use chrono::Utc;

    fn fixture_decision(id: &str, request_hash: &str, bundle_version: &str) -> PolicyDecision {
        PolicyDecision {
            policy_decision_id: id.to_owned(),
            action_id: aios_action::ActionId::default(),
            request_hash: request_hash.to_owned(),
            bundle_version: bundle_version.to_owned(),
            enrichment_snapshot_id: "polb_snap_test".to_owned(),
            decision: Decision::Deny,
            reason_code: "Test".to_owned(),
            reason_message: "test".to_owned(),
            constraints: crate::constraints::Constraints::default(),
            approval: crate::constraints::ApprovalRequirement::default(),
            evidence_receipt_id: String::new(),
            evaluated_at: Utc::now(),
            rules_consulted: 0,
            simulated: false,
        }
    }

    #[test]
    fn put_then_get_returns_inserted_decision() {
        let mut c = DecisionCache::new(8);
        let k = CacheKey::new("rh1", "polb_v1");
        let d = fixture_decision("poldec_1", "rh1", "polb_v1");
        c.put(k.clone(), d.clone());
        assert_eq!(c.get(&k).unwrap().policy_decision_id, d.policy_decision_id);
    }

    #[test]
    fn get_returns_none_for_unknown_key() {
        let mut c = DecisionCache::new(8);
        assert!(c.get(&CacheKey::new("unknown", "polb_v1")).is_none());
    }

    #[test]
    fn invalidate_for_bundle_removes_only_matching_entries() {
        let mut c = DecisionCache::new(8);
        c.put(
            CacheKey::new("rh1", "polb_v1"),
            fixture_decision("a", "rh1", "polb_v1"),
        );
        c.put(
            CacheKey::new("rh2", "polb_v1"),
            fixture_decision("b", "rh2", "polb_v1"),
        );
        c.put(
            CacheKey::new("rh1", "polb_v2"),
            fixture_decision("c", "rh1", "polb_v2"),
        );
        let n = c.invalidate_for_bundle("polb_v1");
        assert_eq!(n, 2);
        assert!(c.get(&CacheKey::new("rh1", "polb_v1")).is_none());
        assert!(c.get(&CacheKey::new("rh2", "polb_v1")).is_none());
        assert!(c.get(&CacheKey::new("rh1", "polb_v2")).is_some());
    }

    #[test]
    fn capacity_zero_falls_back_to_default() {
        let c = DecisionCache::new(0);
        assert_eq!(c.capacity(), DEFAULT_CAPACITY);
    }

    #[test]
    fn lru_eviction_drops_oldest_on_overflow() {
        let mut c = DecisionCache::new(2);
        c.put(CacheKey::new("rh1", "v"), fixture_decision("a", "rh1", "v"));
        c.put(CacheKey::new("rh2", "v"), fixture_decision("b", "rh2", "v"));
        c.put(CacheKey::new("rh3", "v"), fixture_decision("c", "rh3", "v"));
        assert!(c.get(&CacheKey::new("rh1", "v")).is_none());
        assert!(c.get(&CacheKey::new("rh2", "v")).is_some());
        assert!(c.get(&CacheKey::new("rh3", "v")).is_some());
    }

    #[test]
    fn wire_form_is_polc_prefixed_and_32_hex() {
        let k = CacheKey::new("rh1", "polb_v1");
        let w = k.wire_form();
        assert!(w.starts_with("polc_"));
        assert_eq!(w.len(), 5 + 32);
    }

    #[test]
    fn wire_form_is_deterministic_for_same_input() {
        let a = CacheKey::new("rh1", "polb_v1").wire_form();
        let b = CacheKey::new("rh1", "polb_v1").wire_form();
        assert_eq!(a, b);
    }

    #[test]
    fn wire_form_changes_on_any_field_flip() {
        let a = CacheKey::new("rh1", "polb_v1").wire_form();
        let b = CacheKey::new("rh1", "polb_v2").wire_form();
        let c = CacheKey::new("rh2", "polb_v1").wire_form();
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    #[test]
    fn shared_cache_put_get_roundtrip() {
        let c = SharedDecisionCache::with_capacity(8);
        let k = CacheKey::new("rh1", "polb_v1");
        let d = fixture_decision("poldec_1", "rh1", "polb_v1");
        c.put(k.clone(), d.clone());
        let back = c.get(&k).unwrap();
        assert_eq!(back.policy_decision_id, d.policy_decision_id);
    }

    #[test]
    fn shared_cache_invalidate_for_bundle_returns_count() {
        let c = SharedDecisionCache::with_capacity(8);
        c.put(
            CacheKey::new("rh1", "polb_v1"),
            fixture_decision("a", "rh1", "polb_v1"),
        );
        c.put(
            CacheKey::new("rh2", "polb_v1"),
            fixture_decision("b", "rh2", "polb_v1"),
        );
        c.put(
            CacheKey::new("rh1", "polb_v2"),
            fixture_decision("c", "rh1", "polb_v2"),
        );
        let removed = c.invalidate_for_bundle("polb_v1");
        assert_eq!(removed, 2);
        assert_eq!(c.len(), 1);
    }
}
