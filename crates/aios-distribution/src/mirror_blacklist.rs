//! Per-mirror auto-blacklist on repeated content-hash mismatches per S11.1 §3.8 + §10.
//!
//! When a mirror serves tampered content (detected by [`crate::mirror_policy::verify_mirror_bytes`]),
//! the host increments that mirror's mismatch counter. Repeated mismatches from the
//! same mirror within a 24-hour window auto-blacklist the mirror (§3.8):
//!
//! > Repeated mismatches from the same mirror within a 24-hour window
//! > auto-blacklist the mirror with FOREVER `MIRROR_HASH_MISMATCH_BLACKLISTED`
//! > evidence; subsequent fetches from that mirror are pre-rejected at the
//! > fetch step.
//!
//! The blacklist entry persists (§10): "Blacklist persists for 30 days by
//! default; operator can lift earlier with explicit acknowledgement." This
//! implementation stores blacklisted entries indefinitely — the operator-lift
//! surface is deferred to the admin subsystem (T-197).
//!
//! # Evidence note
//!
//! Evidence emission (`MIRROR_HASH_MISMATCH_BLACKLISTED`) is T-196.
//! This module produces the **outcome** (the blacklist insertion) but does
//! not emit evidence records.

use crate::error::DistributionError;
use crate::mirror_policy::MirrorEndpoint;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;

/// Tracks per-mirror mismatch counts and auto-blacklists mirrors that exceed
/// the threshold within a sliding time window.
///
/// # Window semantics
///
/// Mismatch timestamps are recorded per `mirror_url`. On each new mismatch,
/// entries older than [`Self::window`] are pruned. If the count of remaining
/// in-window entries reaches [`Self::threshold`], the mirror is flagged as
/// blacklisted.
///
/// A blacklisted mirror is **pre-rejected** at fetch time — the host never
/// contacts it. Blacklisted entries persist until explicitly lifted by the
/// operator (lift surface deferred).
#[derive(Debug, Clone)]
pub struct MirrorBlacklist {
    /// Sliding time window for mismatch accumulation (default: 24 hours).
    window: Duration,
    /// Number of mismatches within the window that triggers blacklisting
    /// (default: 3). The spec says "repeated"; 3 is the implementation-chosen
    /// concrete value.
    threshold: u32,
    /// Per-mirror mismatch timestamps. Keyed by `mirror_url`.
    mismatches: HashMap<String, Vec<DateTime<Utc>>>,
    /// Blacklisted mirror URLs with the time they were blacklisted.
    /// Persists until operator lifts (lift surface deferred).
    blacklisted: HashMap<String, DateTime<Utc>>,
}

impl MirrorBlacklist {
    /// Creates a new blacklist tracker with explicit window and threshold.
    #[must_use]
    pub fn new(window: Duration, threshold: u32) -> Self {
        Self {
            window,
            threshold,
            mismatches: HashMap::new(),
            blacklisted: HashMap::new(),
        }
    }

    /// Creates a blacklist tracker with the default parameters.
    ///
    /// Defaults:
    /// - window: 24 hours
    /// - threshold: 3 mismatches (the spec says "repeated"; 3 is the
    ///   implementation-chosen concrete default)
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(Duration::hours(24), 3)
    }

    /// Records a content-hash mismatch for a mirror.
    ///
    /// Appends the timestamp, prunes entries older than `window`, and if the
    /// remaining in-window count meets or exceeds `threshold`, inserts the
    /// mirror into the blacklist.
    ///
    /// Returns `true` if the mirror was blacklisted as a result of this call.
    pub fn record_mismatch(&mut self, mirror_url: &str, now: DateTime<Utc>) -> bool {
        let timestamps = self.mismatches.entry(mirror_url.to_string()).or_default();

        timestamps.push(now);

        // Prune entries outside the sliding window
        let cutoff = now
            .checked_sub_signed(self.window)
            .unwrap_or(DateTime::<Utc>::MIN_UTC);
        timestamps.retain(|ts| *ts >= cutoff);

        // Check threshold
        let should_blacklist = timestamps.len() >= self.threshold as usize;
        if should_blacklist {
            self.blacklisted.insert(mirror_url.to_string(), now);
        }
        should_blacklist
    }

    /// Returns `true` if the mirror is currently blacklisted.
    #[must_use]
    pub fn is_blacklisted(&self, mirror_url: &str) -> bool {
        self.blacklisted.contains_key(mirror_url)
    }

    /// Pre-rejects a fetch attempt against a blacklisted mirror.
    ///
    /// Per S11.1 §3.8, blacklisted mirrors are pre-rejected at the fetch step.
    /// Returns `Err(DistributionError::MirrorBlacklisted(...))` if the
    /// endpoint's URL is in the blacklist.
    ///
    /// # Errors
    ///
    /// Returns [`DistributionError::MirrorBlacklisted`] when the endpoint is
    /// blacklisted.
    pub fn pre_reject(
        &self,
        endpoint: &MirrorEndpoint,
        _now: DateTime<Utc>,
    ) -> Result<(), DistributionError> {
        if self.is_blacklisted(&endpoint.url) {
            Err(DistributionError::MirrorBlacklisted(format!(
                "mirror {} is blacklisted — pre-rejecting fetch per S11.1 §3.8",
                endpoint.url
            )))
        } else {
            Ok(())
        }
    }

    /// Returns the mismatch count for a mirror within the current window.
    #[must_use]
    #[cfg(test)]
    pub(crate) fn mismatch_count(&self, mirror_url: &str) -> usize {
        self.mismatches.get(mirror_url).map_or(0, Vec::len)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

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
    use crate::error::DistributionErrorCode;
    use crate::mirror::MirrorSemantic;

    fn test_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-29T12:00:00Z")
            .map_or_else(|_| Utc::now(), |dt| dt.with_timezone(&Utc))
    }

    // ── Blacklist basics ────────────────────────────────────────────────

    #[test]
    fn below_threshold_not_blacklisted() {
        let mut bl = MirrorBlacklist::with_defaults();
        let now = test_now();
        let url = "https://bad-mirror.example.com";

        // Two mismatches — below threshold of 3
        let result1 = bl.record_mismatch(url, now);
        assert!(!result1);
        assert!(!bl.is_blacklisted(url));

        let result2 = bl.record_mismatch(url, now + Duration::minutes(1));
        assert!(!result2);
        assert!(!bl.is_blacklisted(url));
    }

    #[test]
    fn reaching_threshold_blacklists() {
        let mut bl = MirrorBlacklist::with_defaults();
        let now = test_now();
        let url = "https://bad-mirror.example.com";

        bl.record_mismatch(url, now);
        bl.record_mismatch(url, now + Duration::minutes(1));
        let result3 = bl.record_mismatch(url, now + Duration::minutes(2));
        assert!(result3);
        assert!(bl.is_blacklisted(url));
    }

    #[test]
    fn mismatches_beyond_window_do_not_accumulate_to_blacklist() {
        let mut bl = MirrorBlacklist::new(Duration::hours(24), 3);
        let now = test_now();
        let url = "https://slow-bad-mirror.example.com";

        // First mismatch
        bl.record_mismatch(url, now);
        // Second mismatch — 25 hours later, first should be pruned
        bl.record_mismatch(url, now + Duration::hours(25));
        // Third mismatch — 26 hours later, second is still in window but first is gone
        let result3 = bl.record_mismatch(url, now + Duration::hours(26));
        // Only 2 are within the window (at +25h and +26h) → not blacklisted
        assert!(!result3);
        assert!(!bl.is_blacklisted(url));
    }

    #[test]
    fn pruning_works_with_exact_boundary() {
        // Use a window slightly wider than 24h so all three entries stay in-window.
        let mut bl = MirrorBlacklist::new(Duration::hours(25), 3);
        let now = test_now();

        bl.record_mismatch("https://edge.example.com", now);
        bl.record_mismatch("https://edge.example.com", now + Duration::hours(24));
        // At +25h from the first, the first entry is at cutoff and retained by >=
        let result3 = bl.record_mismatch("https://edge.example.com", now + Duration::hours(25));
        assert!(result3);
    }

    #[test]
    fn is_blacklisted_true_after_threshold() {
        let mut bl = MirrorBlacklist::with_defaults();
        let now = test_now();
        let url = "https://evil.example.com";

        bl.record_mismatch(url, now);
        bl.record_mismatch(url, now + Duration::minutes(5));
        bl.record_mismatch(url, now + Duration::minutes(10));
        assert!(bl.is_blacklisted(url));
    }

    #[test]
    fn pre_reject_errors_for_blacklisted_endpoint() {
        let mut bl = MirrorBlacklist::with_defaults();
        let now = test_now();
        let url = "https://blocked.example.com";

        // Blacklist the mirror
        bl.record_mismatch(url, now);
        bl.record_mismatch(url, now + Duration::minutes(1));
        bl.record_mismatch(url, now + Duration::minutes(2));

        let ep = MirrorEndpoint::new(url, MirrorSemantic::Cached);
        let result = bl.pre_reject(&ep, test_now());
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err().code(),
            DistributionErrorCode::MirrorBlacklisted
        );
    }

    #[test]
    fn pre_reject_ok_for_non_blacklisted_endpoint() {
        let bl = MirrorBlacklist::with_defaults();
        let ep = MirrorEndpoint::new("https://good.example.com", MirrorSemantic::Cached);
        let result = bl.pre_reject(&ep, test_now());
        assert!(result.is_ok());
    }

    #[test]
    fn different_mirrors_independent_counters() {
        let mut bl = MirrorBlacklist::with_defaults();
        let now = test_now();

        bl.record_mismatch("https://mirror-a.example.com", now);
        bl.record_mismatch("https://mirror-a.example.com", now + Duration::minutes(1));
        bl.record_mismatch("https://mirror-a.example.com", now + Duration::minutes(2));
        assert!(bl.is_blacklisted("https://mirror-a.example.com"));

        // Mirror B has only 1 mismatch — not blacklisted
        bl.record_mismatch("https://mirror-b.example.com", now);
        assert!(!bl.is_blacklisted("https://mirror-b.example.com"));
    }

    #[test]
    fn blacklist_entry_persists_across_windows() {
        let mut bl = MirrorBlacklist::with_defaults();
        let now = test_now();
        let url = "https://persistent-bad.example.com";

        bl.record_mismatch(url, now);
        bl.record_mismatch(url, now + Duration::minutes(1));
        bl.record_mismatch(url, now + Duration::minutes(2));
        assert!(bl.is_blacklisted(url));

        // After 48 hours, the mismatch timestamps are old but the blacklist
        // entry persists (§10: "Blacklist persists for 30 days by default")
        let far_future = now + Duration::hours(48);
        assert!(bl.is_blacklisted(url));
        // pre_reject still fires
        let ep = MirrorEndpoint::new(url, MirrorSemantic::Cached);
        let result = bl.pre_reject(&ep, far_future);
        assert!(result.is_err());
    }
}
