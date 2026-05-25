//! In-memory [`VerificationEngine`](crate::VerificationEngine) harness for T-065.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S2.4 verification engine vocabulary"
)]

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use strum::IntoEnumIterator;
use tokio::sync::RwLock;
use ulid::Ulid;

use crate::engine::{VerificationContext, VerificationEngine};
use crate::{
    IntentId, PrimitiveResult, VerificationError, VerificationIntent, VerificationPrimitive,
    VerificationResult, VerificationStatus,
};

/// HashMap-backed in-process verification engine used by tests and successor slices.
#[derive(Debug, Clone, Default)]
pub struct InMemoryVerificationEngine {
    completed: Arc<RwLock<HashMap<IntentId, VerificationResult>>>,
}

impl InMemoryVerificationEngine {
    /// Construct an empty in-memory verification engine.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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
        let primitives = parse_primitive_expression(&intent.expression)?;
        let started_at = context.started_at;
        let per_primitive = primitives
            .into_iter()
            .map(stub_primitive_result)
            .collect::<Vec<_>>();
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

fn parse_primitive_expression(
    expression: &str,
) -> Result<Vec<VerificationPrimitive>, VerificationError> {
    let primitive_names = serde_json::from_str::<Vec<String>>(expression)
        .map_err(|err| VerificationError::IntentParseFailed(err.to_string()))?;

    primitive_names
        .into_iter()
        .map(parse_primitive_wire_name)
        .collect()
}

fn parse_primitive_wire_name(name: String) -> Result<VerificationPrimitive, VerificationError> {
    serde_json::from_value(Value::String(name.clone()))
        .map_err(|_err| VerificationError::UnknownPrimitive(name))
}

fn stub_primitive_result(primitive: VerificationPrimitive) -> PrimitiveResult {
    let expected = Value::String(primitive.as_wire_str().to_owned());

    PrimitiveResult {
        primitive_kind: primitive,
        passed: true,
        actual: expected.clone(),
        expected,
        elapsed_ms: 0,
        error: None,
    }
}

fn aggregate_status(per_primitive: &[PrimitiveResult]) -> VerificationStatus {
    if per_primitive.iter().any(primitive_timed_out) {
        VerificationStatus::Timeout
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

fn error_mentions_timeout(error: &str) -> bool {
    error
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("timeout"))
}

fn duration_ms(started_at: DateTime<Utc>, completed_at: DateTime<Utc>) -> u64 {
    u64::try_from((completed_at - started_at).num_milliseconds()).unwrap_or(0)
}
