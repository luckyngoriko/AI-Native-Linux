//! In-memory [`VerificationEngine`](crate::VerificationEngine) harness for M8.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the S2.4 verification engine vocabulary"
)]

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use strum::IntoEnumIterator;
use tokio::sync::RwLock;

use crate::engine::{VerificationContext, VerificationEngine};
use crate::evidence_emit::VerificationEvidenceEmitter;
use crate::executor::{per_primitive_timeout_ms, primitive_count, VerificationExecutor};
use crate::grammar_parser;
use crate::primitives::{LocalProbe, StateProbe, StdLocalProbe, StdStateProbe};
use crate::{
    IntentId, PrimitiveInvocation, VerificationError, VerificationGrammar, VerificationIntent,
    VerificationPrimitive, VerificationResult,
};

/// HashMap-backed in-process verification engine used by tests and successor slices.
#[derive(Clone)]
pub struct InMemoryVerificationEngine {
    completed: Arc<RwLock<HashMap<IntentId, VerificationResult>>>,
    local_probe: Arc<dyn LocalProbe>,
    state_probe: Arc<dyn StateProbe>,
    evidence_emitter: Option<Arc<VerificationEvidenceEmitter>>,
}

impl fmt::Debug for InMemoryVerificationEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryVerificationEngine")
            .field("completed", &self.completed)
            .field("local_probe", &"<dyn LocalProbe>")
            .field("state_probe", &"<dyn StateProbe>")
            .field("evidence_emitter", &self.evidence_emitter)
            .finish()
    }
}

impl Default for InMemoryVerificationEngine {
    fn default() -> Self {
        Self {
            completed: Arc::new(RwLock::new(HashMap::new())),
            local_probe: Arc::new(StdLocalProbe),
            state_probe: Arc::new(StdStateProbe),
            evidence_emitter: None,
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

    /// Replace the Tier-3 cross-layer state probe. Without one, Tier-3
    /// primitives fail closed with a `PROBE_ERROR`. A real deployment injects a
    /// probe backed by the live L2/L4/L8/L9 state holders; tests inject a
    /// [`crate::MockStateProbe`].
    #[must_use]
    pub fn with_state_probe(mut self, probe: Arc<dyn StateProbe>) -> Self {
        self.state_probe = probe;
        self
    }

    /// Enable S3.1 evidence emission for verification runs.
    #[must_use]
    pub fn with_evidence_emitter(
        mut self,
        evidence_emitter: Arc<VerificationEvidenceEmitter>,
    ) -> Self {
        self.evidence_emitter = Some(evidence_emitter);
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
        let grammar = compile_intent(intent)?;
        let default_timeout_ms =
            per_primitive_timeout_ms(intent.timeout_seconds, primitive_count(&grammar));
        if let Some(emitter) = &self.evidence_emitter {
            emitter
                .emit_verification_started(intent, context, None)
                .await?;
        }
        let executor = VerificationExecutor::new(
            Arc::new(self.clone()),
            Arc::clone(&self.local_probe),
            Arc::clone(&self.state_probe),
            default_timeout_ms,
        );
        let mut result = executor
            .execute_for_intent(&grammar, context, intent.intent_id.clone())
            .await;
        if let Some(emitter) = &self.evidence_emitter {
            let receipt_id = emitter
                .emit_verification_result(intent, &result, None)
                .await?;
            result.evidence_receipt_id = Some(receipt_id);
        }

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

/// Compile a verification intent expression into the typed S2.4 grammar AST.
///
/// JSON arrays/objects are accepted for T-065/T-066 backward compatibility.
/// Non-JSON sources are parsed with the S2.4 text grammar.
///
/// # Errors
///
/// Returns [`VerificationError`] when JSON decoding, primitive resolution, or
/// grammar parsing fails.
pub fn compile_intent(
    intent: &VerificationIntent,
) -> Result<VerificationGrammar, VerificationError> {
    let expression = intent.expression.trim_start();
    if expression.starts_with('[') || expression.starts_with('{') {
        parse_json_expression(expression)
    } else {
        grammar_parser::parse(expression)
    }
}

fn parse_json_expression(expression: &str) -> Result<VerificationGrammar, VerificationError> {
    let value = serde_json::from_str::<Value>(expression)
        .map_err(|err| VerificationError::IntentParseFailed(err.to_string()))?;
    match value {
        Value::Array(values) => values
            .into_iter()
            .map(parse_primitive_invocation)
            .map(|result| result.map(VerificationGrammar::Primitive))
            .collect::<Result<Vec<_>, _>>()
            .map(VerificationGrammar::All),
        single @ (Value::Object(_) | Value::String(_)) => parse_primitive_invocation(single)
            .map(VerificationGrammar::Primitive)
            .map(|primitive| VerificationGrammar::All(vec![primitive])),
        _ => Err(VerificationError::IntentParseFailed(
            "JSON verification expression must be an object or array".to_owned(),
        )),
    }
}

fn parse_primitive_invocation(value: Value) -> Result<PrimitiveInvocation, VerificationError> {
    match value {
        Value::String(name) => Ok(PrimitiveInvocation {
            kind: parse_primitive_wire_name(name)?,
            args: Value::Null,
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
                args: expected,
            })
        }
        _ => Err(VerificationError::IntentParseFailed(
            "primitive invocation must be a string or object".to_owned(),
        )),
    }
}

fn parse_primitive_wire_name(name: String) -> Result<VerificationPrimitive, VerificationError> {
    serde_json::from_value(Value::String(name.clone()))
        .or_else(|_err| {
            serde_json::from_value(Value::String(name.replace('.', "_").to_ascii_uppercase()))
        })
        .map_err(|_err| VerificationError::UnknownPrimitive(name))
}
