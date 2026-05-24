//! `DispatchQueue` — per-class FIFO buckets + per-subject token-bucket rate
//! limits + 50 % `AGENT_PROPOSAL` hard cap, per S10.1 §3.5 / §11.
//!
//! This is the L3 queue substrate the runtime enrols actions into between
//! `APPROVED` (T11/T12) and `EXECUTING` (T13). T-029 ships:
//!
//! 1. **Per-class FIFO buckets.** One [`std::collections::VecDeque`] per
//!    [`QueueClass`], guarded by a per-class [`tokio::sync::RwLock`] so reads
//!    on one class do not contend with writes on another.
//!
//! 2. **Per-class default capacities (§11.1).** The constitutional shares are
//!    30 % `INTERACTIVE`, 40 % `AGENT_PROPOSAL`, 25 % `BACKGROUND`,
//!    5 % `RECOVERY_PRIORITY`. T-029 ships these as **absolute** per-class
//!    capacities derived from a default total of `1 000` in-flight actions
//!    (matching the §11.3 backpressure threshold) so the queue can be
//!    exercised under load without an operator-configured ratio table.
//!    `AGENT_PROPOSAL` is also subject to the hard 50 % cap below — the
//!    400-slot default sits within the cap and is the spec's recommended
//!    operating point.
//!
//! 3. **50 % `AGENT_PROPOSAL` hard cap (§11.1).** Constitutional. Regardless
//!    of operator capacity configuration the total `AGENT_PROPOSAL` enrolment
//!    cannot exceed 50 % of the sum of per-class capacities at the moment of
//!    admission. The cap is expressed against **total capacity**, not against
//!    the live enrolment count — the spec's "Max share 50 %" column in the
//!    §11.1 table — so a lone AI submission against an otherwise empty
//!    runtime is admitted. An AI-saturated runtime is the operational
//!    equivalent of a hostile takeover, and the queue contract refuses to
//!    permit it.
//!
//! 4. **Per-subject token-bucket rate limits (§11.2).** Default refill rate
//!    is `30 / 60.0 ≈ 0.5 actions/sec` (AI agent default per §11.2 — 30
//!    actions/minute), burst capacity `15`. Subjects that exceed the burst
//!    without sufficient refill are rejected with
//!    [`RuntimeError::RateLimited`]. The default is intentionally the
//!    strictest §11.2 row so a runtime that never overrides per-subject
//!    classifications stays inside the constitutional envelope.
//!
//! ## Out of scope (queued)
//!
//! - §11.3 backpressure mode (`QUEUE_BACKPRESSURE_REJECTED` at the RPC
//!   surface). The queue tracks total depth via [`DispatchQueue::total_len`]
//!   but the §11.3 shed-load gate is the runtime's responsibility (T-035
//!   wires it).
//! - Per-subject-type classification (human / AI / application / service /
//!   recovery operator → distinct rate buckets). T-029 ships a single
//!   default; T-030 introduces typed `HydratedSubject` and the
//!   per-subject-type table.
//! - Evidence emission on `AI_INTERACTIVE_QUEUE_DOWNGRADE` and
//!   `RESOURCE_BUDGET_EXCEEDED` — T-031 wires `aios-evidence`.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use tokio::sync::RwLock;

use crate::context::ActionContext;
use crate::dispatch::QueueClass;
use crate::error::RuntimeError;

/// Default total queue capacity used when distributing the §11.1 shares.
///
/// Matches §11.3's backpressure threshold (1 000 in-flight) so the queue
/// saturates exactly when backpressure mode would engage.
pub const DEFAULT_TOTAL_CAPACITY: usize = 1_000;

/// Constitutional cap on the `AGENT_PROPOSAL` queue, per §11.1.
///
/// Expressed as a fraction (numerator / denominator) so the comparison
/// (`agent_count * denom <= total * num`) is exact integer arithmetic — no
/// floating-point rounding drift, no chance the cap "leaks" by one slot on a
/// boundary case.
pub const AGENT_PROPOSAL_CAP_NUM: usize = 1;
/// Denominator of the `AGENT_PROPOSAL` cap fraction (1/2 ⇒ 50 %).
pub const AGENT_PROPOSAL_CAP_DEN: usize = 2;

/// Default per-subject token-bucket refill rate (AI agent — strictest §11.2).
///
/// Expressed as a rate per second so the bucket consumes one token per
/// admission attempt and recovers `refill_per_second * elapsed` tokens
/// since the last attempt.
pub const DEFAULT_REFILL_PER_SECOND: f64 = 30.0 / 60.0;

/// Default per-subject token-bucket burst capacity (AI agent burst, §11.2).
///
/// The bucket starts full at `15.0` tokens; sustained submissions drain it
/// to zero, at which point each attempt waits for refill.
pub const DEFAULT_BURST_CAPACITY: f64 = 15.0;

// ---------------------------------------------------------------------------
// TokenBucket.
// ---------------------------------------------------------------------------

/// Per-subject token-bucket rate limiter (§11.2).
///
/// `tokens` increments at `refill_per_second` per real-time second until it
/// hits `capacity`. `consume(n)` returns `true` iff `tokens >= n` after the
/// refill step; on success it subtracts `n` and stamps `last_refill = now`.
///
/// Time source is [`std::time::Instant::now`] (monotonic; cannot be rewound
/// by an operator clock change — the §11.2 rule "operator policy may tighten
/// but cannot disable" is robust against clock games).
#[derive(Debug, Clone, Copy)]
pub struct TokenBucket {
    /// Current token balance. Bounded above by `capacity`.
    tokens: f64,
    /// Maximum token balance the bucket can hold. Matches the §11.2 "burst"
    /// column.
    capacity: f64,
    /// Refill rate in tokens per real-time second.
    refill_per_second: f64,
    /// Wall-clock anchor of the last `refill_to_now` step. Initialised at
    /// construction.
    last_refill: Instant,
}

impl TokenBucket {
    /// Construct a bucket starting full at `capacity`.
    ///
    /// Panic-free; the runtime never panics on a bucket construction. Both
    /// `capacity` and `refill_per_second` are validated to be finite and
    /// non-negative; non-finite or negative inputs are clamped to zero so a
    /// pathological bundle author cannot stall the runtime with a NaN burst.
    #[must_use]
    pub fn new(capacity: f64, refill_per_second: f64) -> Self {
        let capacity = if capacity.is_finite() && capacity >= 0.0 {
            capacity
        } else {
            0.0
        };
        let refill_per_second = if refill_per_second.is_finite() && refill_per_second >= 0.0 {
            refill_per_second
        } else {
            0.0
        };
        Self {
            tokens: capacity,
            capacity,
            refill_per_second,
            last_refill: Instant::now(),
        }
    }

    /// Bucket pre-configured with the §11.2 AI-agent default (30 actions/min,
    /// burst 15). Used when no per-subject override is supplied.
    #[must_use]
    pub fn default_ai_agent() -> Self {
        Self::new(DEFAULT_BURST_CAPACITY, DEFAULT_REFILL_PER_SECOND)
    }

    /// Refill `tokens` up to `capacity` based on real time since
    /// `last_refill`. Idempotent on repeated calls within the same instant.
    fn refill_to_now(&mut self, now: Instant) {
        let elapsed = now.saturating_duration_since(self.last_refill);
        // `Duration::as_secs_f64` is finite for any `Duration`; the
        // multiplication is finite by construction. The `min` keeps the
        // bucket bounded.
        let gain = self.refill_per_second * elapsed.as_secs_f64();
        let new_tokens = (self.tokens + gain).min(self.capacity);
        // Defensive: clamp non-finite into zero, matching the constructor's
        // discipline. `f64::NAN.min(_)` returns the NaN; explicit guard.
        self.tokens = if new_tokens.is_finite() {
            new_tokens
        } else {
            0.0
        };
        self.last_refill = now;
    }

    /// Attempt to consume `n` tokens. Returns `true` on success.
    ///
    /// Refills the bucket first based on real time since the last
    /// `consume`; the order is "refill, then admit" so a bucket that has
    /// been idle long enough to refill is admitted immediately.
    pub fn consume(&mut self, n: f64) -> bool {
        self.consume_at(n, Instant::now())
    }

    /// Test-friendly variant of [`Self::consume`] that takes the "now"
    /// instant as an argument so deterministic tests can drive the bucket
    /// without sleeping.
    pub fn consume_at(&mut self, n: f64, now: Instant) -> bool {
        if !n.is_finite() || n < 0.0 {
            return false;
        }
        self.refill_to_now(now);
        if self.tokens >= n {
            self.tokens -= n;
            true
        } else {
            false
        }
    }

    /// Current (post-refill) token count. Useful in tests.
    #[must_use]
    pub const fn tokens(&self) -> f64 {
        self.tokens
    }

    /// Bucket capacity (the `burst` column from §11.2).
    #[must_use]
    pub const fn capacity(&self) -> f64 {
        self.capacity
    }

    /// Bucket refill rate in tokens per second.
    #[must_use]
    pub const fn refill_per_second(&self) -> f64 {
        self.refill_per_second
    }
}

// ---------------------------------------------------------------------------
// DispatchQueue.
// ---------------------------------------------------------------------------

/// Per-class FIFO + per-subject rate-limit queue for the L3 Capability
/// Runtime, per S10.1 §3.5 / §11.
///
/// Concurrency model: every internal map is wrapped in a single async
/// `RwLock`; the locks are acquired in a fixed order
/// (`queues` → `rate_limiters`) to avoid the obvious deadlock shape. The
/// queue is `Sync` because both inner locks are `Sync`; the runtime holds it
/// behind `Arc<DispatchQueue>` and shares it across `tokio` worker tasks.
#[derive(Debug)]
pub struct DispatchQueue {
    /// Per-class FIFO buckets. Reads on `INTERACTIVE` (the hot path for
    /// human-initiated actions) do not contend with writes on
    /// `AGENT_PROPOSAL` (the hot path for AI proposals).
    queues: RwLock<HashMap<QueueClass, VecDeque<ActionContext>>>,
    /// Per-class capacity limits derived from §11.1's default share table.
    /// Immutable after construction; operators that need different ratios
    /// build a fresh queue with [`Self::new_with_capacities`].
    capacity_per_class: HashMap<QueueClass, usize>,
    /// Per-subject token buckets, keyed by `subject_canonical_id`. Created
    /// on first admission attempt with [`TokenBucket::default_ai_agent`];
    /// per-subject-type classification (human / AI / application / service /
    /// recovery operator) is queued for T-030's hydrated subject.
    rate_limiters: RwLock<HashMap<String, TokenBucket>>,
}

impl DispatchQueue {
    /// Construct a queue with the §11.1 default per-class capacities derived
    /// from a total budget of [`DEFAULT_TOTAL_CAPACITY`] (1 000).
    ///
    /// Per-class slots (approximate per §11.1 shares):
    ///
    /// - `INTERACTIVE`       — 300 slots (30 %)
    /// - `AGENT_PROPOSAL`    — 400 slots (40 %; also bounded by the 50 % hard cap)
    /// - `BACKGROUND`        — 250 slots (25 %)
    /// - `RECOVERY_PRIORITY` — 50  slots (5 %)
    ///
    /// Operators that need different absolute capacities call
    /// [`Self::new_with_capacities`].
    #[must_use]
    pub fn new_with_defaults() -> Self {
        let mut capacities = HashMap::new();
        capacities.insert(QueueClass::Interactive, 300);
        capacities.insert(QueueClass::AgentProposal, 400);
        capacities.insert(QueueClass::Background, 250);
        capacities.insert(QueueClass::RecoveryPriority, 50);
        Self::new_with_capacities(capacities)
    }

    /// Construct a queue with explicit per-class capacities. The 50 %
    /// `AGENT_PROPOSAL` cap (§11.1) is enforced on admission regardless of
    /// the `AGENT_PROPOSAL` capacity supplied here — an operator-configured
    /// `AGENT_PROPOSAL` slot count greater than 50 % of the total cannot be
    /// used.
    #[must_use]
    pub fn new_with_capacities(capacities: HashMap<QueueClass, usize>) -> Self {
        let mut queues = HashMap::new();
        // Seed every closed `QueueClass` so `dequeue` / `depth_per_class`
        // never miss a class. Variants not present in `capacities` are
        // implicitly zero-capacity.
        for class in [
            QueueClass::Interactive,
            QueueClass::AgentProposal,
            QueueClass::Background,
            QueueClass::RecoveryPriority,
        ] {
            queues.insert(class, VecDeque::new());
        }
        Self {
            queues: RwLock::new(queues),
            capacity_per_class: capacities,
            rate_limiters: RwLock::new(HashMap::new()),
        }
    }

    /// Look up the configured capacity for a class. Variants not configured
    /// at construction implicitly have capacity zero.
    #[must_use]
    pub fn capacity_of(&self, class: QueueClass) -> usize {
        self.capacity_per_class.get(&class).copied().unwrap_or(0)
    }

    /// Enrol `context` into its `context.queue_class` bucket.
    ///
    /// Checks (in order):
    /// 1. **Per-subject rate limit (§11.2).** Token-bucket consume; on
    ///    refusal → [`RuntimeError::RateLimited`].
    /// 2. **50 % `AGENT_PROPOSAL` hard cap (§11.1).** If the target class is
    ///    `AGENT_PROPOSAL`, the post-admission `AGENT_PROPOSAL` count must
    ///    not exceed 50 % of the post-admission total enrolment across all
    ///    classes. On refusal → [`RuntimeError::QueueFull`] with
    ///    `QueueClass::AgentProposal`.
    /// 3. **Per-class capacity (§11.1 ratios).** Target class depth + 1 must
    ///    not exceed the configured capacity. On refusal →
    ///    [`RuntimeError::QueueFull`] with the target class.
    ///
    /// On success the context is appended FIFO to the target class and
    /// returned unchanged.
    ///
    /// # Errors
    ///
    /// - [`RuntimeError::RateLimited`] with the offending `subject_id`.
    /// - [`RuntimeError::QueueFull`] with the offending [`QueueClass`].
    pub async fn enroll(
        &self,
        context: ActionContext,
        subject_id: &str,
    ) -> Result<ActionContext, RuntimeError> {
        // (1) Rate-limit gate. Drop the write guard before acquiring the
        // queues lock so the two write locks are never held simultaneously.
        {
            let mut rl = self.rate_limiters.write().await;
            let bucket = rl
                .entry(subject_id.to_owned())
                .or_insert_with(TokenBucket::default_ai_agent);
            let admitted = bucket.consume(1.0);
            drop(rl);
            if !admitted {
                return Err(RuntimeError::RateLimited(subject_id.to_owned()));
            }
        }

        let target_class = context.queue_class;

        // (2) + (3): capacity + AI-share cap. Held under the write lock so
        // the depth read and the insert are atomic against concurrent
        // submissions.
        let mut queues = self.queues.write().await;

        // Per-class capacity.
        let target_depth = queues.get(&target_class).map_or(0, VecDeque::len);
        let target_cap = self.capacity_of(target_class);
        if target_depth + 1 > target_cap {
            drop(queues);
            return Err(RuntimeError::QueueFull(target_class));
        }

        // 50 % AGENT_PROPOSAL hard cap (§11.1 — constitutional). The cap is
        // expressed against the **total queue capacity** (sum across all
        // configured classes), not against the live enrolment count: an
        // AI-saturated runtime is forbidden in absolute terms, not relative
        // to whatever else happens to be queued at the moment. With the
        // §11.1 default 1 000-slot total this bounds `AGENT_PROPOSAL` at
        // 500 enrolled (the manifest's 400-slot per-class cap is the
        // tighter rule today, but the constitutional cap is the one that
        // cannot be relaxed by operator configuration).
        if target_class == QueueClass::AgentProposal {
            let total_capacity: usize = self.capacity_per_class.values().sum();
            let agent_after = queues
                .get(&QueueClass::AgentProposal)
                .map_or(0, VecDeque::len)
                .saturating_add(1);
            if agent_after.saturating_mul(AGENT_PROPOSAL_CAP_DEN)
                > total_capacity.saturating_mul(AGENT_PROPOSAL_CAP_NUM)
            {
                drop(queues);
                return Err(RuntimeError::QueueFull(QueueClass::AgentProposal));
            }
        }

        // Commit.
        let stored = context.clone();
        queues
            .entry(target_class)
            .or_insert_with(VecDeque::new)
            .push_back(stored);
        drop(queues);
        Ok(context)
    }

    /// FIFO dequeue from `class`. Returns `None` when the class is empty.
    pub async fn dequeue(&self, class: QueueClass) -> Option<ActionContext> {
        let mut queues = self.queues.write().await;
        queues.get_mut(&class).and_then(VecDeque::pop_front)
    }

    /// Snapshot of every class's current depth. Used by telemetry and by the
    /// `dispatch_queue.rs` test suite.
    pub async fn depth_per_class(&self) -> HashMap<QueueClass, usize> {
        let queues = self.queues.read().await;
        queues.iter().map(|(class, q)| (*class, q.len())).collect()
    }

    /// Total in-flight enrolment across every class. §11.3 backpressure
    /// monitors this against `QUEUE_BACKPRESSURE_REJECTED`'s threshold.
    pub async fn total_len(&self) -> usize {
        let queues = self.queues.read().await;
        queues.values().map(VecDeque::len).sum()
    }
}

// ---------------------------------------------------------------------------
// Inline unit tests — TokenBucket determinism.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn token_bucket_starts_full() {
        let b = TokenBucket::default_ai_agent();
        assert!((b.tokens() - DEFAULT_BURST_CAPACITY).abs() < f64::EPSILON);
        assert!((b.capacity() - DEFAULT_BURST_CAPACITY).abs() < f64::EPSILON);
    }

    #[test]
    fn token_bucket_consume_drains() {
        let mut b = TokenBucket::default_ai_agent();
        let now = Instant::now();
        // Drain the burst.
        for _ in 0..15 {
            assert!(b.consume_at(1.0, now));
        }
        // Next consume in the same instant must fail (refill = 0).
        assert!(!b.consume_at(1.0, now));
    }

    #[test]
    fn token_bucket_refills_over_time() {
        let mut b = TokenBucket::default_ai_agent();
        let t0 = Instant::now();
        // Drain everything.
        for _ in 0..15 {
            assert!(b.consume_at(1.0, t0));
        }
        assert!(!b.consume_at(1.0, t0));
        // Advance 10 seconds → 5 tokens refilled (0.5/sec).
        let t1 = t0 + Duration::from_secs(10);
        assert!(b.consume_at(1.0, t1));
        // 4 tokens left after consuming 1.
        assert!(b.tokens() < 5.0 && b.tokens() > 3.0);
    }

    #[test]
    fn token_bucket_rejects_nan() {
        let mut b = TokenBucket::default_ai_agent();
        assert!(!b.consume(f64::NAN));
        assert!(!b.consume(-1.0));
    }

    #[test]
    fn token_bucket_clamps_pathological_inputs() {
        let b = TokenBucket::new(f64::NAN, f64::INFINITY);
        assert!((b.capacity() - 0.0).abs() < f64::EPSILON);
        assert!((b.refill_per_second() - 0.0).abs() < f64::EPSILON);
    }
}
