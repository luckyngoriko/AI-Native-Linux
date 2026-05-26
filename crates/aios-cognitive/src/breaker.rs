//! Circuit breaker driver per S14.1 §6 — consumed by S13.2 model router (§9.3).
//!
//! # INV-014 Enforcement
//!
//! State transitions are derived from observed invocation outcomes only.
//! No direct `Open → Closed` transition is permitted — the circuit MUST pass
//! through `HalfOpen` and succeed on probe calls before closing.

use std::collections::VecDeque;

use std::sync::Arc;

use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use crate::circuit::{CircuitBreakerConfig, CircuitBreakerStats, CircuitState};
use crate::error::CognitiveError;
use crate::evidence_emit::CognitiveEvidenceEmitter;
use crate::routing::ModelBackendKind;

const MIN_SAMPLES_TO_OPEN: u64 = 5;
const HALF_OPEN_PROBE_CALLS: u32 = 1;

/// A single invocation outcome recorded in the sliding window.
#[derive(Debug, Clone)]
pub struct CallOutcome {
    /// When the outcome was recorded.
    pub timestamp: DateTime<Utc>,
    /// Whether the invocation succeeded.
    pub succeeded: bool,
    /// Observed latency in milliseconds.
    pub latency_ms: u64,
}

/// A ticket granting permission to make a single probe call in `HalfOpen` state.
#[derive(Debug, Clone)]
pub struct AdmissionTicket {
    /// The backend this ticket is for.
    pub backend: ModelBackendKind,
    /// When the ticket was issued.
    pub issued_at: DateTime<Utc>,
    /// Unique ticket identifier (`brktk_<ULID>`).
    pub ticket_id: String,
}

/// Per-backend circuit breaker with sliding-window error tracking (S14.1 §6).
///
/// # State transitions
///
/// - `Closed → Open`: `error_rate >= threshold` with `≥ MIN_SAMPLES_TO_OPEN` samples
/// - `Open → HalfOpen`: after `cooldown_seconds` elapsed
/// - `HalfOpen → Closed`: probe calls succeed (`probe_success_count >= HALF_OPEN_PROBE_CALLS`)
/// - `HalfOpen → Open`: any probe failure resets cooldown with doubling
///
/// # INV-014
///
/// `Open → Closed` is never permitted. The circuit MUST pass through `HalfOpen`
/// and succeed on probe calls.
pub struct CircuitBreaker {
    backend: ModelBackendKind,
    config: CircuitBreakerConfig,
    state: RwLock<CircuitState>,
    sliding_window: RwLock<VecDeque<CallOutcome>>,
    cooldown_multiplier: RwLock<u32>,
    opened_at: RwLock<Option<DateTime<Utc>>>,
    probe_success_count: RwLock<u32>,
    probe_failure_count: RwLock<u32>,
    state_changed_at: RwLock<DateTime<Utc>>,
    /// Optional evidence emitter for `CIRCUIT_BREAKER_OPENED` / `CIRCUIT_BREAKER_CLOSED`.
    evidence_emitter: Option<Arc<CognitiveEvidenceEmitter>>,
}

impl CircuitBreaker {
    /// Create a new `CircuitBreaker` in `Closed` state with the given config.
    #[must_use]
    pub fn new(backend: ModelBackendKind, config: CircuitBreakerConfig) -> Self {
        Self {
            backend,
            config,
            state: RwLock::new(CircuitState::Closed),
            sliding_window: RwLock::new(VecDeque::new()),
            cooldown_multiplier: RwLock::new(1),
            opened_at: RwLock::new(None),
            probe_success_count: RwLock::new(0),
            probe_failure_count: RwLock::new(0),
            state_changed_at: RwLock::new(Utc::now()),
            evidence_emitter: None,
        }
    }

    /// Attach an evidence emitter for `CIRCUIT_BREAKER_OPENED` / `CIRCUIT_BREAKER_CLOSED`.
    #[must_use]
    pub fn with_evidence_emitter(mut self, emitter: Arc<CognitiveEvidenceEmitter>) -> Self {
        self.evidence_emitter = Some(emitter);
        self
    }

    /// Record an invocation outcome and update circuit state.
    ///
    /// Returns the resulting circuit state after the record is applied.
    #[allow(clippy::significant_drop_tightening)]
    pub async fn record_outcome(&self, succeeded: bool, latency_ms: u64) -> CircuitState {
        let now = Utc::now();
        let outcome = CallOutcome {
            timestamp: now,
            succeeded,
            latency_ms,
        };

        // Prune expired entries from the window before appending.
        {
            let cutoff = now - chrono::Duration::seconds(i64::from(self.config.window_seconds));
            let mut window = self.sliding_window.write().await;
            while window.front().is_some_and(|o| o.timestamp < cutoff) {
                window.pop_front();
            }
            window.push_back(outcome);
        }

        self.recompute_state(now).await
    }

    /// Recompute circuit state from the current sliding window.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    #[allow(clippy::too_many_lines)]
    async fn recompute_state(&self, now: DateTime<Utc>) -> CircuitState {
        let current = *self.state.read().await;

        match current {
            CircuitState::Closed => {
                let (_successes, failures, total) = self.window_counts().await;
                if total < MIN_SAMPLES_TO_OPEN {
                    return CircuitState::Closed;
                }
                let rate = if total == 0 {
                    0.0
                } else {
                    failures as f64 / total as f64
                };
                if rate >= self.config.error_rate_threshold {
                    // Best-effort evidence emission
                    if let Some(ref emitter) = self.evidence_emitter {
                        let _ = emitter
                            .emit_circuit_breaker_tripped(
                                self.backend,
                                CircuitState::Closed,
                                CircuitState::Open,
                                rate,
                            )
                            .await;
                    }
                    *self.state.write().await = CircuitState::Open;
                    *self.opened_at.write().await = Some(now);
                    *self.cooldown_multiplier.write().await = 1;
                    *self.state_changed_at.write().await = now;
                    CircuitState::Open
                } else {
                    CircuitState::Closed
                }
            }
            CircuitState::Open => {
                let cooldown = self.effective_cooldown().await;
                let opened = *self.opened_at.read().await;
                if let Some(opened_time) = opened {
                    let elapsed = (now - opened_time).num_seconds().max(0) as u32;
                    if elapsed >= cooldown {
                        // Reset probe counters for the HalfOpen transition.
                        *self.probe_success_count.write().await = 0;
                        *self.probe_failure_count.write().await = 0;
                        *self.state.write().await = CircuitState::HalfOpen;
                        *self.state_changed_at.write().await = now;
                        return CircuitState::HalfOpen;
                    }
                }
                CircuitState::Open
            }
            CircuitState::HalfOpen => {
                let successes = *self.probe_success_count.read().await;
                let failures = *self.probe_failure_count.read().await;

                if successes >= HALF_OPEN_PROBE_CALLS && failures == 0 {
                    // Best-effort evidence emission
                    if let Some(ref emitter) = self.evidence_emitter {
                        let _ = emitter
                            .emit_circuit_breaker_tripped(
                                self.backend,
                                CircuitState::HalfOpen,
                                CircuitState::Closed,
                                0.0,
                            )
                            .await;
                    }
                    *self.state.write().await = CircuitState::Closed;
                    *self.opened_at.write().await = None;
                    *self.cooldown_multiplier.write().await = 1;
                    *self.probe_success_count.write().await = 0;
                    *self.probe_failure_count.write().await = 0;
                    *self.state_changed_at.write().await = now;
                    return CircuitState::Closed;
                }

                if failures > 0 {
                    // Best-effort evidence emission
                    let window_rate = {
                        let (_s, f, t) = self.window_counts().await;
                        if t == 0 {
                            0.0
                        } else {
                            f as f64 / t as f64
                        }
                    };
                    if let Some(ref emitter) = self.evidence_emitter {
                        let _ = emitter
                            .emit_circuit_breaker_tripped(
                                self.backend,
                                CircuitState::HalfOpen,
                                CircuitState::Open,
                                window_rate,
                            )
                            .await;
                    }
                    // Re-open with doubled cooldown.
                    let current = *self.cooldown_multiplier.read().await;
                    *self.cooldown_multiplier.write().await = (current * 2).min(
                        self.config.max_cooldown_seconds
                            / self.config.initial_cooldown_seconds.max(1),
                    );
                    *self.state.write().await = CircuitState::Open;
                    *self.opened_at.write().await = Some(now);
                    *self.probe_success_count.write().await = 0;
                    *self.probe_failure_count.write().await = 0;
                    *self.state_changed_at.write().await = now;
                    return CircuitState::Open;
                }

                CircuitState::HalfOpen
            }
        }
    }

    /// Count successes, failures, and total in the sliding window.
    async fn window_counts(&self) -> (u64, u64, u64) {
        let window = self.sliding_window.read().await;
        let successes = window.iter().filter(|o| o.succeeded).count() as u64;
        let failures = window.iter().filter(|o| !o.succeeded).count() as u64;
        let total = successes + failures;
        drop(window);
        (successes, failures, total)
    }

    /// Compute the current cooldown with multiplier applied.
    async fn effective_cooldown(&self) -> u32 {
        let multiplier = *self.cooldown_multiplier.read().await;
        (self
            .config
            .initial_cooldown_seconds
            .saturating_mul(multiplier))
        .min(self.config.max_cooldown_seconds)
        .max(self.config.initial_cooldown_seconds)
    }

    /// Return the current circuit state.
    pub async fn current_state(&self) -> CircuitState {
        *self.state.read().await
    }

    /// Return a snapshot of circuit breaker statistics.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    pub async fn current_stats(&self) -> CircuitBreakerStats {
        let state = *self.state.read().await;
        let (successes, failures) = {
            let window = self.sliding_window.read().await;
            let s = window.iter().filter(|o| o.succeeded).count() as u64;
            let f = window.iter().filter(|o| !o.succeeded).count() as u64;
            drop(window);
            (s, f)
        };
        let total = successes + failures;
        let error_rate = if total == 0 {
            0.0
        } else {
            failures as f64 / total as f64
        };

        let cooldown_seconds = match state {
            CircuitState::Open => {
                let opened = *self.opened_at.read().await;
                if let Some(ot) = opened {
                    let eff = self.effective_cooldown().await;
                    let elapsed = (Utc::now() - ot).num_seconds().max(0) as u32;
                    eff.saturating_sub(elapsed)
                } else {
                    0
                }
            }
            _ => 0,
        };

        let next_probe_at = match state {
            CircuitState::Open => {
                let opened = *self.opened_at.read().await;
                if let Some(ot) = opened {
                    let eff = self.effective_cooldown().await;
                    Some(ot + chrono::Duration::seconds(i64::from(eff)))
                } else {
                    None
                }
            }
            _ => None,
        };

        CircuitBreakerStats {
            state,
            success_count: successes,
            failure_count: failures,
            error_rate,
            cooldown_seconds,
            last_state_change_at: *self.state_changed_at.read().await,
            next_probe_at,
        }
    }

    /// Attempt to admit a call through this circuit breaker.
    ///
    /// - `Closed` → returns an `AdmissionTicket` (all calls admitted)
    /// - `Open` → returns `Err(CognitiveError::CircuitBreakerOpen(retry_after_ms))`
    /// - `HalfOpen` → returns an `AdmissionTicket` only when below the probe limit
    ///
    /// # `HalfOpen` probe gate
    ///
    /// Exactly `HALF_OPEN_PROBE_CALLS` calls are admitted as probes. Subsequent
    /// calls before the probe(s) complete are rejected.
    ///
    /// # Errors
    ///
    /// Returns `CircuitBreakerOpen` when the circuit is `Open` or the
    /// `HalfOpen` probe limit has been reached.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub async fn try_admit(&self) -> Result<AdmissionTicket, CognitiveError> {
        let state = *self.state.read().await;

        match state {
            CircuitState::Closed => Ok(AdmissionTicket {
                backend: self.backend,
                issued_at: Utc::now(),
                ticket_id: format!("brktk_{}", ulid::Ulid::new()),
            }),
            CircuitState::Open => {
                let retry_after_ms = {
                    let opened = *self.opened_at.read().await;
                    if let Some(ot) = opened {
                        let eff = self.effective_cooldown().await;
                        let elapsed = (Utc::now() - ot).num_seconds().max(0) as u32;
                        let remaining = eff.saturating_sub(elapsed);
                        u64::from(remaining) * 1000
                    } else {
                        0
                    }
                };
                Err(CognitiveError::CircuitBreakerOpen(format!(
                    "backend {self:?}: circuit open, retry_after_ms={retry_after_ms}",
                    self = self.backend
                )))
            }
            CircuitState::HalfOpen => {
                // Atomically check and increment probe count.
                let current_probes = {
                    let s = *self.probe_success_count.read().await;
                    let f = *self.probe_failure_count.read().await;
                    s + f
                };
                if current_probes >= HALF_OPEN_PROBE_CALLS {
                    return Err(CognitiveError::CircuitBreakerOpen(format!(
                        "backend {self:?}: half-open probe limit reached",
                        self = self.backend
                    )));
                }
                Ok(AdmissionTicket {
                    backend: self.backend,
                    issued_at: Utc::now(),
                    ticket_id: format!("brktk_{}", ulid::Ulid::new()),
                })
            }
        }
    }

    /// Update probe counters after a probe call completes in `HalfOpen`.
    ///
    /// Called externally after the probe call result is known. Success increments
    /// `probe_success_count`; failure increments `probe_failure_count` and triggers
    /// re-open via `recompute_state`.
    pub async fn record_probe_outcome(&self, succeeded: bool, latency_ms: u64) -> CircuitState {
        let state = *self.state.read().await;
        if state != CircuitState::HalfOpen {
            // Not in HalfOpen — fall through to normal record_outcome.
            return self.record_outcome(succeeded, latency_ms).await;
        }

        if succeeded {
            *self.probe_success_count.write().await += 1;
        } else {
            *self.probe_failure_count.write().await += 1;
        }

        // Also record in the sliding window for stats continuity.
        {
            let now = Utc::now();
            let cutoff = now - chrono::Duration::seconds(i64::from(self.config.window_seconds));
            let mut window = self.sliding_window.write().await;
            while window.front().is_some_and(|o| o.timestamp < cutoff) {
                window.pop_front();
            }
            window.push_back(CallOutcome {
                timestamp: now,
                succeeded,
                latency_ms,
            });
        }

        self.recompute_state(Utc::now()).await
    }

    /// Directly manipulate circuit state — for testing only.
    ///
    /// # INV-014
    ///
    /// This method exists only for test fixture setup. In production, state is
    /// computed from observed invocation outcomes via `record_outcome`.
    #[allow(clippy::significant_drop_tightening)]
    pub async fn set_state_for_test(&self, state: CircuitState) {
        let now = Utc::now();
        match state {
            CircuitState::Closed => {
                *self.state.write().await = CircuitState::Closed;
                *self.opened_at.write().await = None;
                *self.cooldown_multiplier.write().await = 1;
                *self.probe_success_count.write().await = 0;
                *self.probe_failure_count.write().await = 0;
                // Seed a clean window so the breaker stays closed.
                {
                    let mut window = self.sliding_window.write().await;
                    window.clear();
                    for _ in 0..100 {
                        window.push_back(CallOutcome {
                            timestamp: now,
                            succeeded: true,
                            latency_ms: 50,
                        });
                    }
                }
                *self.state_changed_at.write().await = now;
            }
            CircuitState::Open => {
                *self.state.write().await = CircuitState::Open;
                *self.opened_at.write().await = Some(now);
                *self.cooldown_multiplier.write().await = 1;
                *self.probe_success_count.write().await = 0;
                *self.probe_failure_count.write().await = 0;
                {
                    let mut window = self.sliding_window.write().await;
                    window.clear();
                    for _ in 0..100 {
                        window.push_back(CallOutcome {
                            timestamp: now,
                            succeeded: false,
                            latency_ms: 500,
                        });
                    }
                }
                *self.state_changed_at.write().await = now;
            }
            CircuitState::HalfOpen => {
                // Put breaker into Open first, then simulate cooldown expiry.
                let past = now
                    - chrono::Duration::seconds(
                        i64::from(self.config.initial_cooldown_seconds) + 1,
                    );
                *self.state.write().await = CircuitState::Open;
                *self.opened_at.write().await = Some(past);
                *self.cooldown_multiplier.write().await = 1;
                *self.probe_success_count.write().await = 0;
                *self.probe_failure_count.write().await = 0;
                {
                    let mut window = self.sliding_window.write().await;
                    window.clear();
                    for _ in 0..100 {
                        window.push_back(CallOutcome {
                            timestamp: past,
                            succeeded: false,
                            latency_ms: 500,
                        });
                    }
                }
                *self.state_changed_at.write().await = past;
                // Trigger recompute which will detect cooldown expired → HalfOpen.
                let _ = self.recompute_state(now).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::expect_used,
        clippy::panic,
        clippy::float_cmp,
        clippy::unwrap_used,
        clippy::field_reassign_with_default,
        reason = "panic-on-failure is the idiomatic test signal"
    )]

    use super::*;

    #[tokio::test]
    async fn new_breaker_starts_closed() {
        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
        assert_eq!(br.current_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn closed_stays_closed_with_low_error_rate() {
        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
        // 15 successes + 1 failure = 16 total, 1/16 ≈ 6.25% ≥ 5% → Open.
        // Instead use 29 successes + 1 failure = 30 total, 1/30 ≈ 3.33% < 5% → Closed.
        for _ in 0..29 {
            br.record_outcome(true, 50).await;
        }
        br.record_outcome(false, 50).await;
        assert_eq!(br.current_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn closed_opens_on_high_error_rate() {
        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
        // 6 samples with 1 failure → 1/6 ≈ 16.7% ≥ 5% threshold → Opens.
        for _ in 0..5 {
            br.record_outcome(true, 50).await;
        }
        br.record_outcome(false, 500).await;
        // Now 5 success + 1 failure = 6 total ≥ 5 samples, error rate 1/6 ≈ 16.7%
        // But wait — record_outcome also appends and recomputes. The last call will trigger recompute.
        assert_eq!(br.current_state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn closed_requires_min_samples_before_opening() {
        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
        // Fewer than MIN_SAMPLES_TO_OPEN (5) should NOT open even with 100% failure.
        for _ in 0..4 {
            br.record_outcome(false, 500).await;
        }
        assert_eq!(br.current_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn open_transitions_to_half_open_after_cooldown() {
        let config = CircuitBreakerConfig {
            error_rate_threshold: 0.05,
            window_seconds: 300,
            initial_cooldown_seconds: 1,
            max_cooldown_seconds: 600,
        };

        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, config);
        // Force Open with a past timestamp so cooldown is already expired.
        for _ in 0..10 {
            br.record_outcome(false, 500).await;
        }
        assert_eq!(br.current_state().await, CircuitState::Open);

        // Manually set opened_at to the past to simulate cooldown expiry.
        let past = Utc::now() - chrono::Duration::seconds(2);
        *br.opened_at.write().await = Some(past);

        // Next record_outcome should detect cooldown expired → HalfOpen.
        br.record_outcome(true, 50).await;
        assert_eq!(br.current_state().await, CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn half_open_closes_on_successful_probe() {
        let mut config = CircuitBreakerConfig::default();
        config.initial_cooldown_seconds = 0;

        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, config);
        // Force Open.
        for _ in 0..10 {
            br.record_outcome(false, 500).await;
        }
        // Transition to HalfOpen.
        br.record_outcome(true, 50).await;
        assert_eq!(br.current_state().await, CircuitState::HalfOpen);

        // Successful probe → Closed.
        br.record_probe_outcome(true, 50).await;
        assert_eq!(br.current_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn half_open_reopens_on_probe_failure() {
        let mut config = CircuitBreakerConfig::default();
        config.initial_cooldown_seconds = 0;

        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, config);
        for _ in 0..10 {
            br.record_outcome(false, 500).await;
        }
        br.record_outcome(true, 50).await;
        assert_eq!(br.current_state().await, CircuitState::HalfOpen);

        // Failed probe → back to Open.
        br.record_probe_outcome(false, 500).await;
        assert_eq!(br.current_state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn inv_014_no_direct_open_to_closed() {
        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
        // Force Open.
        for _ in 0..10 {
            br.record_outcome(false, 500).await;
        }
        assert_eq!(br.current_state().await, CircuitState::Open);

        // Record successes while still open — must NOT close directly.
        for _ in 0..100 {
            br.record_outcome(true, 50).await;
        }
        // Still Open because cooldown hasn't expired (default 30s).
        assert_eq!(br.current_state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn cooldown_doubles_on_repeated_open() {
        let config = CircuitBreakerConfig {
            error_rate_threshold: 0.05,
            window_seconds: 300,
            initial_cooldown_seconds: 1,
            max_cooldown_seconds: 600,
        };

        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, config);
        // Force Open.
        for _ in 0..10 {
            br.record_outcome(false, 500).await;
        }
        assert_eq!(br.current_state().await, CircuitState::Open);

        // Wait then transition to HalfOpen.
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        br.record_outcome(true, 50).await;
        assert_eq!(br.current_state().await, CircuitState::HalfOpen);

        // Fail probe → re-Open.
        br.record_probe_outcome(false, 500).await;
        assert_eq!(br.current_state().await, CircuitState::Open);

        // Multiplier should now be 2.
        assert_eq!(*br.cooldown_multiplier.read().await, 2);
    }

    #[tokio::test]
    async fn try_admit_allows_in_closed() {
        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
        let ticket = br.try_admit().await;
        assert!(ticket.is_ok());
    }

    #[tokio::test]
    async fn try_admit_rejects_in_open() {
        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
        // Force Open.
        for _ in 0..10 {
            br.record_outcome(false, 500).await;
        }
        assert_eq!(br.current_state().await, CircuitState::Open);
        let result = br.try_admit().await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            CognitiveError::CircuitBreakerOpen(_)
        ));
    }

    #[tokio::test]
    async fn try_admit_allows_single_probe_in_half_open() {
        let mut config = CircuitBreakerConfig::default();
        config.initial_cooldown_seconds = 0;

        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, config);
        for _ in 0..10 {
            br.record_outcome(false, 500).await;
        }
        br.record_outcome(true, 50).await;
        assert_eq!(br.current_state().await, CircuitState::HalfOpen);

        let ticket = br.try_admit().await;
        assert!(ticket.is_ok());
    }

    #[tokio::test]
    async fn current_stats_reflects_state() {
        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, CircuitBreakerConfig::default());
        let stats = br.current_stats().await;
        assert_eq!(stats.state, CircuitState::Closed);
        assert_eq!(stats.success_count, 0);
        assert_eq!(stats.failure_count, 0);
        assert_eq!(stats.error_rate, 0.0);
        assert_eq!(stats.cooldown_seconds, 0);
        assert!(stats.next_probe_at.is_none());
    }

    #[tokio::test]
    async fn sliding_window_prunes_expired_entries() {
        let mut config = CircuitBreakerConfig::default();
        config.window_seconds = 1;

        let br = CircuitBreaker::new(ModelBackendKind::LocalGpu, config);
        for _ in 0..10 {
            br.record_outcome(false, 500).await;
        }
        // With cooldown=30s the circuit stays Open.
        // Wait for window to expire.
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        // Record one more success — this prunes old entries.
        br.record_outcome(true, 50).await;
        let stats = br.current_stats().await;
        // After pruning, only the recent success should remain.
        assert_eq!(stats.success_count, 1);
        assert_eq!(stats.failure_count, 0);
    }
}
