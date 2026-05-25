//! In-memory [`VerificationEngine`](crate::VerificationEngine) harness for M8.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S2.4 verification engine vocabulary"
)]

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use strum::IntoEnumIterator;
use tokio::sync::RwLock;
use ulid::Ulid;

use crate::engine::{VerificationContext, VerificationEngine};
use crate::primitives::{self, LocalProbe, PrimitiveTier, StdLocalProbe};
use crate::{
    IntentId, PrimitiveResult, VerificationError, VerificationIntent, VerificationPrimitive,
    VerificationResult, VerificationStatus,
};

/// HashMap-backed in-process verification engine used by tests and successor slices.
#[derive(Clone)]
pub struct InMemoryVerificationEngine {
    completed: Arc<RwLock<HashMap<IntentId, VerificationResult>>>,
    local_probe: Arc<dyn LocalProbe>,
}

impl fmt::Debug for InMemoryVerificationEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryVerificationEngine")
            .field("completed", &self.completed)
            .field("local_probe", &"<dyn LocalProbe>")
            .finish()
    }
}

impl Default for InMemoryVerificationEngine {
    fn default() -> Self {
        Self {
            completed: Arc::new(RwLock::new(HashMap::new())),
            local_probe: Arc::new(StdLocalProbe),
        }
    }
}

impl InMemoryVerificationEngine {
    /// Construct an empty in-memory verification engine.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the Tier-2 local probe, primarily for deterministic tests.
    #[must_use]
    pub fn with_local_probe(mut self, probe: Arc<dyn LocalProbe>) -> Self {
        self.local_probe = probe;
        self
    }

    /// Return a completed verification result from the in-memory cache.
    pub async fn get_result(&self, intent_id: &IntentId) -> Option<VerificationResult> {
        self.completed.read().await.get(intent_id).cloned()
    }
}

#[async_trait]
impl VerificationEngine for InMemoryVerificationEngine {
    async fn run_verification(
        &self,
        intent: &VerificationIntent,
        context: &VerificationContext,
    ) -> Result<VerificationResult, VerificationError> {
        let invocations = parse_primitive_expression(&intent.expression)?;
        let started_at = context.started_at;
        let mut per_primitive = Vec::with_capacity(invocations.len());
        for invocation in invocations {
            per_primitive.push(execute_primitive(invocation, self.local_probe.as_ref()).await);
        }
        let completed_at = Utc::now();
        let result = VerificationResult {
            result_id: format!("vrf_{}", Ulid::new()),
            intent_id: intent.intent_id.clone(),
            action_id: context.action_id.clone(),
            status: aggregate_status(&per_primitive),
            per_primitive,
            started_at,
            completed_at,
            duration_ms: duration_ms(started_at, completed_at),
            evidence_receipt_id: None,
        };

        self.completed
            .write()
            .await
            .insert(intent.intent_id.clone(), result.clone());

        Ok(result)
    }

    async fn list_primitives(&self) -> Vec<VerificationPrimitive> {
        VerificationPrimitive::iter().collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PrimitiveInvocation {
    kind: VerificationPrimitive,
    expected: Value,
}

fn parse_primitive_expression(
    expression: &str,
) -> Result<Vec<PrimitiveInvocation>, VerificationError> {
    let primitive_values = serde_json::from_str::<Vec<Value>>(expression)
        .map_err(|err| VerificationError::IntentParseFailed(err.to_string()))?;

    primitive_values
        .into_iter()
        .map(parse_primitive_invocation)
        .collect()
}

fn parse_primitive_invocation(value: Value) -> Result<PrimitiveInvocation, VerificationError> {
    match value {
        Value::String(name) => Ok(PrimitiveInvocation {
            kind: parse_primitive_wire_name(name)?,
            expected: Value::Null,
        }),
        Value::Object(mut object) => {
            let Some(name) = object
                .remove("primitive")
                .or_else(|| object.remove("kind"))
                .or_else(|| object.remove("type"))
            else {
                return Err(VerificationError::IntentParseFailed(
                    "primitive invocation is missing `primitive`".to_owned(),
                ));
            };
            let Some(name) = name.as_str() else {
                return Err(VerificationError::IntentParseFailed(
                    "`primitive` must be a string".to_owned(),
                ));
            };
            let expected = object.remove("expected").unwrap_or(Value::Object(object));

            Ok(PrimitiveInvocation {
                kind: parse_primitive_wire_name(name.to_owned())?,
                expected,
            })
        }
        _ => Err(VerificationError::IntentParseFailed(
            "primitive invocation must be a string or object".to_owned(),
        )),
    }
}

fn parse_primitive_wire_name(name: String) -> Result<VerificationPrimitive, VerificationError> {
    serde_json::from_value(Value::String(name.clone()))
        .map_err(|_err| VerificationError::UnknownPrimitive(name))
}

async fn execute_primitive(
    invocation: PrimitiveInvocation,
    local_probe: &dyn LocalProbe,
) -> PrimitiveResult {
    match primitives::primitive_tier(invocation.kind) {
        PrimitiveTier::Tier1 => primitives::tier1::execute(invocation.kind, &invocation.expected),
        PrimitiveTier::Tier2 => {
            primitives::tier2::execute(invocation.kind, &invocation.expected, local_probe).await
        }
        PrimitiveTier::Tier3 => {
            primitives::tier3::deferred_result(invocation.kind, &invocation.expected)
        }
    }
}

fn aggregate_status(per_primitive: &[PrimitiveResult]) -> VerificationStatus {
    if per_primitive.iter().any(primitive_timed_out) {
        VerificationStatus::Timeout
    } else if per_primitive.iter().any(primitive_probe_error) {
        VerificationStatus::ProbeError
    } else if per_primitive.iter().all(|primitive| primitive.passed) {
        VerificationStatus::Passed
    } else {
        VerificationStatus::Failed
    }
}

fn primitive_timed_out(primitive: &PrimitiveResult) -> bool {
    primitive
        .error
        .as_deref()
        .is_some_and(error_mentions_timeout)
}

const fn primitive_probe_error(primitive: &PrimitiveResult) -> bool {
    primitive.error.is_some()
}

fn error_mentions_timeout(error: &str) -> bool {
    error
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("timeout"))
}

fn duration_ms(started_at: DateTime<Utc>, completed_at: DateTime<Utc>) -> u64 {
    u64::try_from((completed_at - started_at).num_milliseconds()).unwrap_or(0)
}
