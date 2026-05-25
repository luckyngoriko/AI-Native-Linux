//! Capability Runtime adapter for the S2.4 verification engine.

#![allow(
    clippy::module_name_repetitions,
    reason = "Public names mirror the cross-crate runtime integration vocabulary"
)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use aios_action::ActionId;
use aios_capability_runtime::runtime::RuntimeVerificationEngine;

use crate::{VerificationContext, VerificationEngine, VerificationIntent, VerificationStatus};

/// Bridges `aios-verification` into `aios-capability-runtime` without adding
/// a reverse dependency from the runtime crate back to verification.
#[derive(Clone)]
pub struct VerificationRuntimeAdapter {
    engine: Arc<dyn VerificationEngine>,
    subject: String,
    default_timeout_seconds: u32,
}

impl std::fmt::Debug for VerificationRuntimeAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerificationRuntimeAdapter")
            .field("engine", &"<dyn VerificationEngine>")
            .field("subject", &self.subject)
            .field("default_timeout_seconds", &self.default_timeout_seconds)
            .finish()
    }
}

impl VerificationRuntimeAdapter {
    /// Construct an adapter from any shared verification engine.
    #[must_use]
    pub fn new<E>(engine: Arc<E>) -> Self
    where
        E: VerificationEngine + 'static,
    {
        Self {
            engine,
            subject: "aios:capability-runtime".to_owned(),
            default_timeout_seconds: 5,
        }
    }

    /// Construct an adapter from an already-erased verification engine.
    #[must_use]
    pub fn from_dyn(engine: Arc<dyn VerificationEngine>) -> Self {
        Self {
            engine,
            subject: "aios:capability-runtime".to_owned(),
            default_timeout_seconds: 5,
        }
    }

    /// Override the subject recorded in the verification context.
    #[must_use]
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = subject.into();
        self
    }

    /// Override the timeout used when the runtime supplies a raw expression
    /// instead of a serialized [`VerificationIntent`].
    #[must_use]
    pub const fn with_default_timeout_seconds(mut self, timeout_seconds: u32) -> Self {
        self.default_timeout_seconds = timeout_seconds;
        self
    }

    fn parse_intent(
        &self,
        intent_json: &str,
        action_id: &ActionId,
    ) -> Result<Option<VerificationIntent>, String> {
        let source = intent_json.trim();
        if source.is_empty() {
            return Ok(None);
        }

        if source.starts_with('{') {
            let mut intent = serde_json::from_str::<VerificationIntent>(source)
                .map_err(|err| format!("invalid verification intent json: {err}"))?;
            intent.action_id = action_id.clone();
            return Ok(Some(intent));
        }

        Ok(Some(VerificationIntent::new(
            action_id.clone(),
            normalize_legacy_single_all(source),
            self.default_timeout_seconds,
        )))
    }
}

#[async_trait]
impl RuntimeVerificationEngine for VerificationRuntimeAdapter {
    async fn verify(&self, intent_json: &str, action_id: &str) -> Result<bool, String> {
        let action_id = ActionId::parse(action_id)
            .map_err(|err| format!("invalid runtime action_id `{action_id}`: {err}"))?;
        let Some(intent) = self.parse_intent(intent_json, &action_id)? else {
            return Ok(true);
        };
        let context = VerificationContext {
            subject: self.subject.clone(),
            action_id,
            started_at: Utc::now(),
            timeout_seconds: intent.timeout_seconds,
            dry_run: false,
        };
        let result = self
            .engine
            .run_verification(&intent, &context)
            .await
            .map_err(|err| err.to_string())?;
        Ok(result.status == VerificationStatus::Passed)
    }
}

fn normalize_legacy_single_all(source: &str) -> String {
    let trimmed = source.trim();
    let Some(inner) = trimmed
        .strip_prefix("all(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return trimmed.to_owned();
    };
    if inner.contains(',') {
        trimmed.to_owned()
    } else {
        inner.trim().to_owned()
    }
}
