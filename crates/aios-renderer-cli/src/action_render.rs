//! Cross-crate renderers for action and capability-runtime state.

use serde::Serialize;

use aios_action::ActionEnvelope;
use aios_capability_runtime::{
    ActionContext, ActionLifecycleState, ExecutionFailureReason, RollbackOutcome,
};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

const EVIDENCE_HEAD_LIMIT: usize = 3;
const RECEIPT_TRUNCATE_AT: usize = 16;

impl Renderable for ActionContext {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_action_context_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_action_context_tree(self, ctx),
            OutputFormat::Table => render_action_context_table(self, ctx),
        }
    }
}

impl Renderable for ActionLifecycleState {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                Ok(renderer.render_kv("status", &styled_lifecycle_state(*self, ctx)))
            }
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "ActionLifecycleState".to_owned(),
                    children: vec![TreeNode {
                        label: format!("status: {}", styled_lifecycle_state(*self, ctx)),
                        children: Vec::new(),
                    }],
                };

                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                render_status_table("-", lifecycle_state_label(*self), "-", "-", ctx)
            }
        }
    }
}

impl Renderable for RollbackOutcome {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                Ok(renderer.render_kv("rollback_outcome", rollback_outcome_label(*self)))
            }
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "RollbackOutcome".to_owned(),
                    children: vec![TreeNode {
                        label: format!("rollback_outcome: {}", rollback_outcome_label(*self)),
                        children: Vec::new(),
                    }],
                };

                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                render_status_table("-", rollback_outcome_label(*self), "-", "-", ctx)
            }
        }
    }
}

impl Renderable for ExecutionFailureReason {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                Ok(renderer.render_kv("failure_reason", failure_reason_label(*self)))
            }
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "ExecutionFailureReason".to_owned(),
                    children: vec![TreeNode {
                        label: format!("failure_reason: {}", failure_reason_label(*self)),
                        children: Vec::new(),
                    }],
                };

                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                render_status_table("-", failure_reason_label(*self), "-", "-", ctx)
            }
        }
    }
}

impl Renderable for ActionEnvelope {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => render_action_envelope_text(self, ctx),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_action_envelope_tree(self, ctx),
            OutputFormat::Table => render_action_envelope_table(self, ctx),
        }
    }
}

fn render_action_context_text(context: &ActionContext, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let mut lines = vec![
        renderer.render_kv("action_id", context.action_id.as_str()),
        renderer.render_kv("status", &styled_lifecycle_state(context.status, ctx)),
        renderer.render_kv("dispatch_kind", dispatch_kind_label(context.dispatch_kind)),
        renderer.render_kv("queue_class", queue_class_label(context.queue_class)),
        renderer.render_kv("created_at", &context.created_at.to_rfc3339()),
        renderer.render_kv("last_updated_at", &context.last_updated_at.to_rfc3339()),
        renderer.render_kv("duration_ms", &duration_ms(context).to_string()),
    ];

    if let Some(error) = context.error {
        lines.push(renderer.render_kv("failure_reason", failure_reason_label(error)));
    }

    if let Some(outcome) = context.rollback_outcome {
        lines.push(renderer.render_kv("rollback_outcome", rollback_outcome_label(outcome)));
    }

    lines.extend(evidence_text_lines(&renderer, &context.evidence_chain));

    renderer.render_section("Action", &lines)
}

fn render_action_context_tree(
    context: &ActionContext,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("Action {}", context.action_id.as_str()),
        children: vec![
            leaf(format!(
                "status: {}",
                styled_lifecycle_state(context.status, ctx)
            )),
            leaf(format!(
                "dispatch_kind: {}",
                dispatch_kind_label(context.dispatch_kind)
            )),
            leaf(format!(
                "queue_class: {}",
                queue_class_label(context.queue_class)
            )),
            evidence_tree(&context.evidence_chain),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_action_context_table(
    context: &ActionContext,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    render_status_table(
        context.action_id.as_str(),
        lifecycle_state_label(context.status),
        dispatch_kind_label(context.dispatch_kind),
        &duration_ms(context).to_string(),
        ctx,
    )
}

fn render_action_envelope_text(
    envelope: &ActionEnvelope,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let renderer = TextRenderer::new(ctx.clone());
    let parent_action_id = envelope
        .request
        .parent_action_id
        .as_ref()
        .map_or("-", aios_action::ActionId::as_str);
    let session_id = envelope.identity.session_id.as_deref().unwrap_or("-");
    let target = render_json(&envelope.request.target)?;

    let lines = vec![
        renderer.render_kv("schema_version", &envelope.schema_version),
        renderer.render_kv(
            "subject_canonical_id",
            &envelope.identity.subject_canonical_id,
        ),
        renderer.render_kv("is_ai", &envelope.identity.is_ai.to_string()),
        renderer.render_kv("session_id", session_id),
        renderer.render_kv("action", &envelope.request.action),
        renderer.render_kv("target", &target),
        renderer.render_kv("dry_run", dry_run_label(envelope.request.dry_run)),
        renderer.render_kv("parent_action_id", parent_action_id),
        renderer.render_kv("phase", action_phase_label(envelope.execution.phase)),
        renderer.render_kv(
            "sandbox_profile_id",
            envelope
                .execution
                .sandbox_profile_id
                .as_deref()
                .unwrap_or("-"),
        ),
    ];

    Ok(renderer.render_section("ActionEnvelope", &lines))
}

fn render_action_envelope_tree(
    envelope: &ActionEnvelope,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let parent_action_id = envelope
        .request
        .parent_action_id
        .as_ref()
        .map_or("-", aios_action::ActionId::as_str);
    let session_id = envelope.identity.session_id.as_deref().unwrap_or("-");
    let target = render_json(&envelope.request.target)?;
    let root = TreeNode {
        label: "ActionEnvelope".to_owned(),
        children: vec![
            TreeNode {
                label: "identity".to_owned(),
                children: vec![
                    leaf(format!(
                        "subject_canonical_id: {}",
                        envelope.identity.subject_canonical_id
                    )),
                    leaf(format!("is_ai: {}", envelope.identity.is_ai)),
                    leaf(format!("session_id: {session_id}")),
                ],
            },
            TreeNode {
                label: "request".to_owned(),
                children: vec![
                    leaf(format!("action: {}", envelope.request.action)),
                    leaf(format!("target: {target}")),
                    leaf(format!(
                        "dry_run: {}",
                        dry_run_label(envelope.request.dry_run)
                    )),
                    leaf(format!("parent_action_id: {parent_action_id}")),
                ],
            },
            TreeNode {
                label: "execution".to_owned(),
                children: vec![
                    leaf(format!(
                        "phase: {}",
                        action_phase_label(envelope.execution.phase)
                    )),
                    leaf(format!(
                        "sandbox_profile_id: {}",
                        envelope
                            .execution
                            .sandbox_profile_id
                            .as_deref()
                            .unwrap_or("-")
                    )),
                ],
            },
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_action_envelope_table(
    envelope: &ActionEnvelope,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let action_id = envelope
        .request
        .parent_action_id
        .as_ref()
        .map_or("-", aios_action::ActionId::as_str);
    let duration = envelope_duration_ms(envelope)
        .map_or_else(|| "-".to_owned(), |duration| duration.to_string());

    render_status_table(
        action_id,
        action_phase_label(envelope.execution.phase),
        &envelope.request.action,
        &duration,
        ctx,
    )
}

fn evidence_text_lines(renderer: &TextRenderer, evidence_chain: &[String]) -> Vec<String> {
    if evidence_chain.is_empty() {
        return vec![renderer.render_kv("evidence_chain", "(no evidence)")];
    }

    vec![
        renderer.render_kv("evidence_chain", &format!("count={}", evidence_chain.len())),
        renderer.render_kv(
            "evidence_receipts",
            &evidence_head(evidence_chain).join(", "),
        ),
    ]
}

fn evidence_tree(evidence_chain: &[String]) -> TreeNode {
    if evidence_chain.is_empty() {
        return TreeNode {
            label: "evidence_chain: count=0".to_owned(),
            children: vec![leaf("(no evidence)")],
        };
    }

    TreeNode {
        label: format!("evidence_chain: count={}", evidence_chain.len()),
        children: evidence_head(evidence_chain)
            .into_iter()
            .map(leaf)
            .collect(),
    }
}

fn evidence_head(evidence_chain: &[String]) -> Vec<String> {
    evidence_chain
        .iter()
        .take(EVIDENCE_HEAD_LIMIT)
        .map(|receipt_id| truncate_receipt_id(receipt_id))
        .collect()
}

fn truncate_receipt_id(receipt_id: &str) -> String {
    if receipt_id.chars().count() <= RECEIPT_TRUNCATE_AT {
        return receipt_id.to_owned();
    }

    let head = receipt_id
        .chars()
        .take(RECEIPT_TRUNCATE_AT)
        .collect::<String>();

    format!("{head}...")
}

fn render_status_table(
    action_id: &str,
    status: &str,
    adapter: &str,
    duration_ms: &str,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "action_id".to_owned(),
            "status".to_owned(),
            "adapter".to_owned(),
            "duration_ms".to_owned(),
        ],
        rows: vec![vec![
            action_id.to_owned(),
            status.to_owned(),
            adapter.to_owned(),
            duration_ms.to_owned(),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Right,
        ],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn styled_lifecycle_state(state: ActionLifecycleState, ctx: &RenderContext) -> String {
    let label = lifecycle_state_label(state);

    if !ctx.color {
        return label.to_owned();
    }

    let color = match state {
        ActionLifecycleState::Succeeded => "32",
        ActionLifecycleState::Failed
        | ActionLifecycleState::RollbackFailed
        | ActionLifecycleState::PolicyDenied => "31",
        ActionLifecycleState::OverrideDenied => "33",
        ActionLifecycleState::RolledBack => "36",
        ActionLifecycleState::Created
        | ActionLifecycleState::PolicyPending
        | ActionLifecycleState::ApprovalPending
        | ActionLifecycleState::OverridePending
        | ActionLifecycleState::Approved
        | ActionLifecycleState::Queued
        | ActionLifecycleState::Executing
        | ActionLifecycleState::Verifying => "34",
    };

    let colored = format!("\u{1b}[{color}m{label}\u{1b}[0m");

    if state.is_terminal() {
        format!("\u{1b}[1m{colored}\u{1b}[0m")
    } else {
        colored
    }
}

const fn lifecycle_state_label(state: ActionLifecycleState) -> &'static str {
    match state {
        ActionLifecycleState::Created => "Created",
        ActionLifecycleState::PolicyPending => "PolicyPending",
        ActionLifecycleState::ApprovalPending => "ApprovalPending",
        ActionLifecycleState::OverridePending => "OverridePending",
        ActionLifecycleState::Approved => "Approved",
        ActionLifecycleState::PolicyDenied => "PolicyDenied",
        ActionLifecycleState::OverrideDenied => "OverrideDenied",
        ActionLifecycleState::Queued => "Queued",
        ActionLifecycleState::Executing => "Executing",
        ActionLifecycleState::Verifying => "Verifying",
        ActionLifecycleState::Succeeded => "Succeeded",
        ActionLifecycleState::Failed => "Failed",
        ActionLifecycleState::RolledBack => "RolledBack",
        ActionLifecycleState::RollbackFailed => "RollbackFailed",
    }
}

const fn dispatch_kind_label(kind: aios_capability_runtime::ActionDispatchKind) -> &'static str {
    match kind {
        aios_capability_runtime::ActionDispatchKind::InProcessRpc => "InProcessRpc",
        aios_capability_runtime::ActionDispatchKind::SubprocessFork => "SubprocessFork",
        aios_capability_runtime::ActionDispatchKind::IsolatedSandbox => "IsolatedSandbox",
        aios_capability_runtime::ActionDispatchKind::DryRun => "DryRun",
    }
}

const fn queue_class_label(queue_class: aios_capability_runtime::QueueClass) -> &'static str {
    match queue_class {
        aios_capability_runtime::QueueClass::Interactive => "Interactive",
        aios_capability_runtime::QueueClass::AgentProposal => "AgentProposal",
        aios_capability_runtime::QueueClass::Background => "Background",
        aios_capability_runtime::QueueClass::RecoveryPriority => "RecoveryPriority",
    }
}

const fn rollback_outcome_label(outcome: RollbackOutcome) -> &'static str {
    match outcome {
        RollbackOutcome::NotAttempted => "NotAttempted",
        RollbackOutcome::Succeeded => "Succeeded",
        RollbackOutcome::Failed => "Failed",
        RollbackOutcome::NotApplicable => "NotApplicable",
    }
}

const fn failure_reason_label(reason: ExecutionFailureReason) -> &'static str {
    match reason {
        ExecutionFailureReason::SandboxApplicationFailed => "SandboxApplicationFailed",
        ExecutionFailureReason::AdapterTimeout => "AdapterTimeout",
        ExecutionFailureReason::AdapterPanic => "AdapterPanic",
        ExecutionFailureReason::ResourceBudgetExceeded => "ResourceBudgetExceeded",
        ExecutionFailureReason::DependencyUnready => "DependencyUnready",
        ExecutionFailureReason::BackendUnavailable => "BackendUnavailable",
        ExecutionFailureReason::IdempotencyKeyReplayDetected => "IdempotencyKeyReplayDetected",
        ExecutionFailureReason::EnvelopeValidationFailed => "EnvelopeValidationFailed",
        ExecutionFailureReason::RollbackPreconditionFailed => "RollbackPreconditionFailed",
        ExecutionFailureReason::BindingExpired => "BindingExpired",
        ExecutionFailureReason::BindingVoidedActionRevised => "BindingVoidedActionRevised",
        ExecutionFailureReason::AdapterRefused => "AdapterRefused",
    }
}

const fn dry_run_label(mode: aios_action::DryRunMode) -> &'static str {
    match mode {
        aios_action::DryRunMode::Live => "Live",
        aios_action::DryRunMode::Validate => "Validate",
        aios_action::DryRunMode::Simulate => "Simulate",
    }
}

const fn action_phase_label(phase: aios_action::ActionPhase) -> &'static str {
    match phase {
        aios_action::ActionPhase::Pending => "Pending",
        aios_action::ActionPhase::Running => "Running",
        aios_action::ActionPhase::Succeeded => "Succeeded",
        aios_action::ActionPhase::Failed => "Failed",
        aios_action::ActionPhase::RolledBack => "RolledBack",
    }
}

fn duration_ms(context: &ActionContext) -> i64 {
    context
        .last_updated_at
        .signed_duration_since(context.created_at)
        .num_milliseconds()
}

fn envelope_duration_ms(envelope: &ActionEnvelope) -> Option<i64> {
    let started_at = envelope.execution.started_at?;
    let ended_at = envelope.execution.ended_at?;

    Some(
        ended_at
            .signed_duration_since(started_at)
            .num_milliseconds(),
    )
}

fn render_json<T: Serialize>(value: &T) -> Result<String, RenderError> {
    serde_json::to_string(value).map_err(|err| RenderError::SerializationFailed(err.to_string()))
}

fn leaf(label: impl Into<String>) -> TreeNode {
    TreeNode {
        label: label.into(),
        children: Vec::new(),
    }
}
