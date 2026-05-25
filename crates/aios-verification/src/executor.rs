//! S2.4 verification grammar executor.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S2.4 verification engine vocabulary"
)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use tokio::time::{sleep, timeout};
use ulid::Ulid;

use crate::engine::VerificationContext;
use crate::in_memory_engine::InMemoryVerificationEngine;
use crate::primitives::{self, LocalProbe, PrimitiveTier};
use crate::{
    IntentId, PrimitiveInvocation, PrimitiveResult, VerificationDuration, VerificationDurationUnit,
    VerificationGrammar, VerificationResult, VerificationStatus,
};

type EvalFuture<'a> = Pin<Box<dyn Future<Output = NodeOutcome> + Send + 'a>>;

/// Executes parsed S2.4 verification grammar expressions.
#[derive(Clone)]
pub struct VerificationExecutor {
    engine: Arc<InMemoryVerificationEngine>,
    local_probe: Arc<dyn LocalProbe>,
    default_timeout_ms: u64,
}

impl VerificationExecutor {
    /// Construct a grammar executor.
    #[must_use]
    pub const fn new(
        engine: Arc<InMemoryVerificationEngine>,
        local_probe: Arc<dyn LocalProbe>,
        default_timeout_ms: u64,
    ) -> Self {
        Self {
            engine,
            local_probe,
            default_timeout_ms,
        }
    }

    /// Return the source in-memory engine.
    #[must_use]
    pub fn engine(&self) -> Arc<InMemoryVerificationEngine> {
        Arc::clone(&self.engine)
    }

    /// Execute a parsed grammar tree into a top-level verification result.
    pub async fn execute(
        &self,
        grammar: &VerificationGrammar,
        context: &VerificationContext,
    ) -> VerificationResult {
        self.execute_for_intent(grammar, context, synthetic_intent_id(context))
            .await
    }

    pub(crate) async fn execute_for_intent(
        &self,
        grammar: &VerificationGrammar,
        context: &VerificationContext,
        intent_id: IntentId,
    ) -> VerificationResult {
        let primitive_timeout_ms =
            self.primitive_timeout_ms(context.timeout_seconds, primitive_count(grammar));
        let outcome = self
            .evaluate(grammar, primitive_timeout_ms, EvaluationScope::default())
            .await;
        let completed_at = Utc::now();

        VerificationResult {
            result_id: format!("vrf_{}", Ulid::new()),
            intent_id,
            action_id: context.action_id.clone(),
            status: outcome.status,
            per_primitive: outcome.per_primitive,
            started_at: context.started_at,
            completed_at,
            duration_ms: duration_ms(context.started_at, completed_at),
            evidence_receipt_id: None,
        }
    }

    fn primitive_timeout_ms(&self, timeout_seconds: u32, count: usize) -> u64 {
        if self.default_timeout_ms > 0 {
            return self.default_timeout_ms;
        }
        per_primitive_timeout_ms(timeout_seconds, count)
    }

    fn evaluate<'a>(
        &'a self,
        grammar: &'a VerificationGrammar,
        primitive_timeout_ms: u64,
        scope: EvaluationScope,
    ) -> EvalFuture<'a> {
        Box::pin(async move {
            match grammar {
                VerificationGrammar::Primitive(invocation) => {
                    self.evaluate_primitive(invocation, primitive_timeout_ms, scope)
                        .await
                }
                VerificationGrammar::All(terms) => {
                    self.evaluate_all(terms, primitive_timeout_ms, scope).await
                }
                VerificationGrammar::Any(terms) => {
                    self.evaluate_any(terms, primitive_timeout_ms, scope).await
                }
                VerificationGrammar::Not(term) => {
                    self.evaluate_not(term, primitive_timeout_ms, scope).await
                }
                VerificationGrammar::Eventually {
                    term,
                    max_duration,
                    interval,
                } => {
                    self.evaluate_eventually(
                        term,
                        *max_duration,
                        *interval,
                        primitive_timeout_ms,
                        scope,
                    )
                    .await
                }
            }
        })
    }

    async fn evaluate_all(
        &self,
        terms: &[VerificationGrammar],
        primitive_timeout_ms: u64,
        scope: EvaluationScope,
    ) -> NodeOutcome {
        let mut per_primitive = Vec::new();

        for (index, term) in terms.iter().enumerate() {
            let child = self.evaluate(term, primitive_timeout_ms, scope).await;
            let child_status = child.status_for_parent();
            let load_bearing_probe_error = child.load_bearing_probe_error;
            per_primitive.extend(child.per_primitive);

            match child_status {
                VerificationStatus::Passed => {}
                VerificationStatus::Timeout => {
                    append_skipped_terms(&mut per_primitive, &terms[index + 1..]);
                    return NodeOutcome::new(
                        VerificationStatus::Timeout,
                        per_primitive,
                        load_bearing_probe_error,
                    );
                }
                VerificationStatus::ProbeError => {
                    append_skipped_terms(&mut per_primitive, &terms[index + 1..]);
                    return NodeOutcome::new(
                        VerificationStatus::ProbeError,
                        per_primitive,
                        load_bearing_probe_error,
                    );
                }
                VerificationStatus::Failed | VerificationStatus::Skipped => {
                    append_skipped_terms(&mut per_primitive, &terms[index + 1..]);
                    return NodeOutcome::new(
                        VerificationStatus::Failed,
                        per_primitive,
                        load_bearing_probe_error,
                    );
                }
            }
        }

        NodeOutcome::new(VerificationStatus::Passed, per_primitive, false)
    }

    async fn evaluate_any(
        &self,
        terms: &[VerificationGrammar],
        primitive_timeout_ms: u64,
        scope: EvaluationScope,
    ) -> NodeOutcome {
        let mut per_primitive = Vec::new();
        let mut saw_timeout = false;
        let mut saw_load_bearing_probe_error = false;

        for (index, term) in terms.iter().enumerate() {
            let child = self.evaluate(term, primitive_timeout_ms, scope).await;
            let child_status = child.status_for_parent();
            saw_timeout |= child_status == VerificationStatus::Timeout;
            saw_load_bearing_probe_error |= child.load_bearing_probe_error;
            per_primitive.extend(child.per_primitive);

            if child_status == VerificationStatus::Passed {
                append_skipped_terms(&mut per_primitive, &terms[index + 1..]);
                return NodeOutcome::new(VerificationStatus::Passed, per_primitive, false);
            }
        }

        if saw_load_bearing_probe_error {
            NodeOutcome::new(VerificationStatus::ProbeError, per_primitive, true)
        } else if saw_timeout {
            NodeOutcome::new(VerificationStatus::Timeout, per_primitive, false)
        } else {
            NodeOutcome::new(VerificationStatus::Failed, per_primitive, false)
        }
    }

    async fn evaluate_not(
        &self,
        term: &VerificationGrammar,
        primitive_timeout_ms: u64,
        _scope: EvaluationScope,
    ) -> NodeOutcome {
        let child = self
            .evaluate(
                term,
                primitive_timeout_ms,
                EvaluationScope { under_not: true },
            )
            .await;
        let status = match child.status {
            VerificationStatus::Passed => VerificationStatus::Failed,
            VerificationStatus::Failed => VerificationStatus::Passed,
            VerificationStatus::Timeout => VerificationStatus::Timeout,
            VerificationStatus::ProbeError => VerificationStatus::ProbeError,
            VerificationStatus::Skipped => VerificationStatus::Skipped,
        };

        NodeOutcome::new(status, child.per_primitive, false)
    }

    async fn evaluate_eventually(
        &self,
        term: &VerificationGrammar,
        max_duration: VerificationDuration,
        interval: VerificationDuration,
        primitive_timeout_ms: u64,
        scope: EvaluationScope,
    ) -> NodeOutcome {
        let max_duration_ms = duration_to_ms(max_duration).max(1);
        let interval_ms = duration_to_ms(interval).max(1);
        let deadline = Instant::now() + Duration::from_millis(max_duration_ms);
        let mut retry_count = 0_u64;
        let mut last_outcome = None;

        loop {
            let remaining = remaining_ms(deadline);
            if remaining == 0 {
                break;
            }
            let attempt_timeout_ms = primitive_timeout_ms.max(1).min(remaining).max(1);
            retry_count = retry_count.saturating_add(1);
            let mut outcome = self.evaluate(term, attempt_timeout_ms, scope).await;
            annotate_retry_count(&mut outcome.per_primitive, retry_count);

            if outcome.status == VerificationStatus::Passed {
                return NodeOutcome::new(
                    VerificationStatus::Passed,
                    outcome.per_primitive,
                    outcome.load_bearing_probe_error,
                );
            }

            last_outcome = Some(outcome);
            let remaining = remaining_ms(deadline);
            if remaining == 0 {
                break;
            }
            sleep(Duration::from_millis(interval_ms.min(remaining))).await;
        }

        let mut outcome = last_outcome.unwrap_or_else(|| {
            let mut per_primitive = Vec::new();
            append_skipped_terms(&mut per_primitive, std::slice::from_ref(term));
            NodeOutcome::new(VerificationStatus::Timeout, per_primitive, false)
        });
        let status = if outcome.load_bearing_probe_error {
            VerificationStatus::ProbeError
        } else {
            VerificationStatus::Timeout
        };
        annotate_eventually_terminal(
            &mut outcome.per_primitive,
            status,
            max_duration_ms,
            retry_count,
        );
        NodeOutcome::new(
            status,
            outcome.per_primitive,
            outcome.load_bearing_probe_error,
        )
    }

    async fn evaluate_primitive(
        &self,
        invocation: &PrimitiveInvocation,
        primitive_timeout_ms: u64,
        scope: EvaluationScope,
    ) -> NodeOutcome {
        let result = self
            .execute_primitive_with_timeout(invocation, primitive_timeout_ms)
            .await;
        let status = primitive_status(&result);
        let load_bearing_probe_error =
            status == VerificationStatus::ProbeError && !scope.under_not && is_probe_error(&result);
        NodeOutcome::new(status, vec![result], load_bearing_probe_error)
    }

    async fn execute_primitive_with_timeout(
        &self,
        invocation: &PrimitiveInvocation,
        primitive_timeout_ms: u64,
    ) -> PrimitiveResult {
        let started_at = Instant::now();
        let timeout_ms = primitive_timeout_ms.max(1);
        let execution = async {
            match primitives::primitive_tier(invocation.kind) {
                PrimitiveTier::Tier1 => {
                    primitives::tier1::execute(invocation.kind, &invocation.args)
                }
                PrimitiveTier::Tier2 => {
                    primitives::tier2::execute(
                        invocation.kind,
                        &invocation.args,
                        self.local_probe.as_ref(),
                    )
                    .await
                }
                PrimitiveTier::Tier3 => {
                    primitives::tier3::deferred_result(invocation.kind, &invocation.args)
                }
            }
        };

        match timeout(Duration::from_millis(timeout_ms), execution).await {
            Ok(mut result) => {
                result.elapsed_ms = elapsed_ms(started_at);
                result
            }
            Err(_elapsed) => PrimitiveResult {
                primitive_kind: invocation.kind,
                passed: false,
                actual: Value::Null,
                expected: invocation.args.clone(),
                elapsed_ms: elapsed_ms(started_at),
                error: Some(format!("TIMEOUT after {timeout_ms}ms")),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct EvaluationScope {
    under_not: bool,
}

#[derive(Debug, Clone)]
struct NodeOutcome {
    status: VerificationStatus,
    per_primitive: Vec<PrimitiveResult>,
    load_bearing_probe_error: bool,
}

impl NodeOutcome {
    const fn new(
        status: VerificationStatus,
        per_primitive: Vec<PrimitiveResult>,
        load_bearing_probe_error: bool,
    ) -> Self {
        Self {
            status,
            per_primitive,
            load_bearing_probe_error,
        }
    }

    const fn status_for_parent(&self) -> VerificationStatus {
        if matches!(self.status, VerificationStatus::ProbeError) && !self.load_bearing_probe_error {
            VerificationStatus::Failed
        } else {
            self.status
        }
    }
}

pub(crate) fn primitive_count(grammar: &VerificationGrammar) -> usize {
    match grammar {
        VerificationGrammar::Primitive(_) => 1,
        VerificationGrammar::All(terms) | VerificationGrammar::Any(terms) => {
            terms.iter().map(primitive_count).sum()
        }
        VerificationGrammar::Not(term) | VerificationGrammar::Eventually { term, .. } => {
            primitive_count(term)
        }
    }
}

pub(crate) fn per_primitive_timeout_ms(timeout_seconds: u32, count: usize) -> u64 {
    let timeout_ms = u64::from(timeout_seconds).saturating_mul(1_000);
    if count == 0 {
        return timeout_ms.max(1);
    }
    (timeout_ms / u64::try_from(count).unwrap_or(u64::MAX)).max(1)
}

fn append_skipped_terms(per_primitive: &mut Vec<PrimitiveResult>, terms: &[VerificationGrammar]) {
    for term in terms {
        append_skipped_term(per_primitive, term);
    }
}

fn append_skipped_term(per_primitive: &mut Vec<PrimitiveResult>, term: &VerificationGrammar) {
    match term {
        VerificationGrammar::Primitive(invocation) => {
            per_primitive.push(short_circuited_result(invocation));
        }
        VerificationGrammar::All(terms) | VerificationGrammar::Any(terms) => {
            append_skipped_terms(per_primitive, terms);
        }
        VerificationGrammar::Not(term) | VerificationGrammar::Eventually { term, .. } => {
            append_skipped_term(per_primitive, term);
        }
    }
}

fn short_circuited_result(invocation: &PrimitiveInvocation) -> PrimitiveResult {
    PrimitiveResult {
        primitive_kind: invocation.kind,
        passed: false,
        actual: Value::Null,
        expected: invocation.args.clone(),
        elapsed_ms: 0,
        error: Some("SHORT_CIRCUITED".to_owned()),
    }
}

fn primitive_status(result: &PrimitiveResult) -> VerificationStatus {
    match &result.error {
        Some(error) if error_mentions_timeout(error) => VerificationStatus::Timeout,
        Some(_error) => VerificationStatus::ProbeError,
        None if result.passed => VerificationStatus::Passed,
        None => VerificationStatus::Failed,
    }
}

fn is_probe_error(result: &PrimitiveResult) -> bool {
    result
        .error
        .as_deref()
        .is_some_and(|error| !error_mentions_timeout(error))
}

fn error_mentions_timeout(error: &str) -> bool {
    error
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("timeout"))
}

const fn duration_to_ms(duration: VerificationDuration) -> u64 {
    let multiplier = match duration.unit {
        VerificationDurationUnit::Milliseconds => 1,
        VerificationDurationUnit::Seconds => 1_000,
        VerificationDurationUnit::Minutes => 60_000,
        VerificationDurationUnit::Hours => 3_600_000,
    };
    duration.value.saturating_mul(multiplier)
}

fn remaining_ms(deadline: Instant) -> u64 {
    u64::try_from(
        deadline
            .saturating_duration_since(Instant::now())
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

fn annotate_retry_count(per_primitive: &mut [PrimitiveResult], retry_count: u64) {
    for result in per_primitive {
        add_retry_count_to_actual(&mut result.actual, retry_count);
    }
}

fn annotate_eventually_terminal(
    per_primitive: &mut [PrimitiveResult],
    status: VerificationStatus,
    max_duration_ms: u64,
    retry_count: u64,
) {
    for result in per_primitive {
        add_retry_count_to_actual(&mut result.actual, retry_count);
        match status {
            VerificationStatus::ProbeError => append_retry_count_to_error(result, retry_count),
            VerificationStatus::Timeout => {
                result.error = Some(format!(
                    "TIMEOUT after {max_duration_ms}ms; retry_count={retry_count}"
                ));
            }
            VerificationStatus::Passed
            | VerificationStatus::Failed
            | VerificationStatus::Skipped => {}
        }
    }
}

fn add_retry_count_to_actual(actual: &mut Value, retry_count: u64) {
    match actual {
        Value::Object(object) => {
            object.insert("retry_count".to_owned(), json!(retry_count));
        }
        other => {
            let previous = std::mem::take(other);
            *other = json!({
                "last_actual": previous,
                "retry_count": retry_count,
            });
        }
    }
}

fn append_retry_count_to_error(result: &mut PrimitiveResult, retry_count: u64) {
    let retry_suffix = format!("retry_count={retry_count}");
    result.error = Some(match result.error.take() {
        Some(error) if error.contains("retry_count=") => error,
        Some(error) => format!("{error}; {retry_suffix}"),
        None => retry_suffix,
    });
}

fn elapsed_ms(started_at: Instant) -> u64 {
    u64::try_from(started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
}

pub(crate) fn duration_ms(started_at: DateTime<Utc>, completed_at: DateTime<Utc>) -> u64 {
    u64::try_from((completed_at - started_at).num_milliseconds()).unwrap_or(0)
}

fn synthetic_intent_id(context: &VerificationContext) -> IntentId {
    let action_body = context
        .action_id
        .as_str()
        .strip_prefix(aios_action::ActionId::PREFIX)
        .unwrap_or("00000000000000000000000000");
    IntentId(format!("{}{}", IntentId::PREFIX, action_body))
}
