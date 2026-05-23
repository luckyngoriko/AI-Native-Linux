//! W3C trace context carried on the action envelope (S0.1 §9.1).
//!
//! Standard [W3C Trace Context](https://www.w3.org/TR/trace-context/) — no AIOS-specific
//! extensions. Populated once at envelope creation (caller, or by the Capability Runtime's
//! gRPC interceptor at the edge); stable across the lifecycle.

use serde::{Deserialize, Serialize};

/// W3C trace-context tuple carried on the envelope.
///
/// The full S0.1 §9.1 message also includes `trace_flags`, `trace_state`, and `baggage`;
/// those land in T-002 once gRPC interceptor integration is wired up. T-001 ships the
/// three fields the evidence projector needs to join traces with evidence receipts
/// (S0.1 §9.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    /// W3C trace-id (32 hex chars), stable across the envelope lifecycle.
    pub trace_id: String,

    /// W3C span-id (16 hex chars) — the caller's span, parent of any CR-created child spans.
    pub span_id: String,

    /// Optional parent span-id when this envelope is causally chained to another span.
    pub parent_span_id: Option<String>,
}

impl Trace {
    /// Build a trace from explicit ids (typical edge-interceptor path).
    #[must_use]
    pub fn new(
        trace_id: impl Into<String>,
        span_id: impl Into<String>,
        parent_span_id: Option<String>,
    ) -> Self {
        Self {
            trace_id: trace_id.into(),
            span_id: span_id.into(),
            parent_span_id,
        }
    }
}
