use chrono::{DateTime, Utc};

/// 6-state integration lifecycle FSM (S11.4 §2, invariant I1).
///
/// All integration resources — vendor contracts, standard subscriptions,
/// CVE bindings, composed systems — obey this state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntegrationLifecycleState {
    /// Resource newly proposed, awaiting evaluation.
    Proposed {
        /// Canonical identity of the proposer.
        proposer: String,
        /// UTC timestamp when the resource was proposed.
        proposed_at: DateTime<Utc>,
    },
    /// Evaluation complete; security audit results recorded.
    Evaluated {
        /// Canonical identity of the evaluator.
        evaluator: String,
        /// UTC timestamp when evaluation was completed.
        evaluated_at: DateTime<Utc>,
        /// Whether the security audit passed.
        security_audit_passed: bool,
    },
    /// Limited-scope pilot deployment active.
    Piloted {
        /// UTC timestamp when the pilot started.
        since: DateTime<Utc>,
        /// Pilot profile identifier (e.g. `"DEV_RELAXED"`).
        profile: String,
    },
    /// Full production deployment active.
    Production {
        /// UTC timestamp when the resource entered production.
        since: DateTime<Utc>,
    },
    /// Resource deprecated, may have a sunset deadline.
    Deprecated {
        /// UTC timestamp when deprecation was declared.
        since: DateTime<Utc>,
        /// Optional mandatory sunset date.
        sunset_due: Option<DateTime<Utc>>,
    },
    /// Resource permanently retired; data migration status recorded.
    Retired {
        /// UTC timestamp when the resource was retired.
        since: DateTime<Utc>,
        /// Reason for retirement.
        reason: String,
        /// Whether data migration was completed before retirement.
        data_migration_completed: bool,
    },
}

/// Closed label for each lifecycle variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntegrationLifecycleLabel {
    /// Corresponds to `IntegrationLifecycleState::Proposed`.
    Proposed,
    /// Corresponds to `IntegrationLifecycleState::Evaluated`.
    Evaluated,
    /// Corresponds to `IntegrationLifecycleState::Piloted`.
    Piloted,
    /// Corresponds to `IntegrationLifecycleState::Production`.
    Production,
    /// Corresponds to `IntegrationLifecycleState::Deprecated`.
    Deprecated,
    /// Corresponds to `IntegrationLifecycleState::Retired`.
    Retired,
}

impl IntegrationLifecycleLabel {
    /// Returns the canonical label for this lifecycle state (used in contract signing).
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Evaluated => "evaluated",
            Self::Piloted => "piloted",
            Self::Production => "production",
            Self::Deprecated => "deprecated",
            Self::Retired => "retired",
        }
    }
}

impl IntegrationLifecycleState {
    /// Returns the label for this lifecycle state.
    #[must_use]
    pub const fn label(&self) -> IntegrationLifecycleLabel {
        match self {
            Self::Proposed { .. } => IntegrationLifecycleLabel::Proposed,
            Self::Evaluated { .. } => IntegrationLifecycleLabel::Evaluated,
            Self::Piloted { .. } => IntegrationLifecycleLabel::Piloted,
            Self::Production { .. } => IntegrationLifecycleLabel::Production,
            Self::Deprecated { .. } => IntegrationLifecycleLabel::Deprecated,
            Self::Retired { .. } => IntegrationLifecycleLabel::Retired,
        }
    }
}
