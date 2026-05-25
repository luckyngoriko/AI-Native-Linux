//! Cross-crate renderers for policy decisions and bindings.

use serde::Serialize;

use aios_policy::{
    ApprovalRequirement, ApprovalScope, ApproverClass, Constraints, Decision, EvidenceGrade,
    NetworkPolicy, PolicyDecision, SessionClass,
};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

impl Renderable for PolicyDecision {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_decision_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_decision_tree(self, ctx),
            OutputFormat::Table => render_decision_table(self, ctx),
        }
    }
}

impl Renderable for Decision {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                Ok(renderer.render_kv("decision", &styled_decision(*self, ctx)))
            }
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "Decision".to_owned(),
                    children: vec![leaf(styled_decision(*self, ctx))],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["decision".to_owned()],
                    rows: vec![vec![styled_decision(*self, ctx)]],
                    align: vec![TableAlign::Left],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

impl Renderable for Constraints {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_constraints_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => {
                let root = constraints_tree(self);
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                render_field_table("field", "value", constraints_rows(self), ctx)
            }
        }
    }
}

impl Renderable for ApprovalRequirement {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_approval_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => {
                let root = approval_tree(self);
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => render_field_table("field", "value", approval_rows(self), ctx),
        }
    }
}

fn render_decision_text(decision: &PolicyDecision, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("policy_decision_id", &decision.policy_decision_id),
        renderer.render_kv("action_id", decision.action_id.as_str()),
        renderer.render_kv("decision", &styled_decision(decision.decision, ctx)),
        renderer.render_kv("reason_code", &decision.reason_code),
        renderer.render_kv("reason_message", &decision.reason_message),
        renderer.render_kv("constraints", &constraints_summary(&decision.constraints)),
        renderer.render_kv("approval", &approval_summary(&decision.approval)),
    ];

    renderer.render_section("PolicyDecision", &lines)
}

fn render_decision_tree(
    decision: &PolicyDecision,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("PolicyDecision {}", decision.policy_decision_id),
        children: vec![
            leaf(format!("action_id: {}", decision.action_id.as_str())),
            leaf(format!(
                "decision: {}",
                styled_decision(decision.decision, ctx)
            )),
            leaf(format!("reason_code: {}", decision.reason_code)),
            leaf(format!("reason_message: {}", decision.reason_message)),
            constraints_tree(&decision.constraints),
            approval_tree(&decision.approval),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_decision_table(
    decision: &PolicyDecision,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    render_field_table(
        "field",
        "value",
        vec![
            (
                "decision".to_owned(),
                styled_decision(decision.decision, ctx),
            ),
            ("reason_code".to_owned(), decision.reason_code.clone()),
            ("reason_message".to_owned(), decision.reason_message.clone()),
            (
                "constraints".to_owned(),
                constraints_summary(&decision.constraints),
            ),
            ("approval".to_owned(), approval_summary(&decision.approval)),
        ],
        ctx,
    )
}

fn render_constraints_text(constraints: &Constraints, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = constraints_rows(constraints)
        .into_iter()
        .map(|(key, value)| renderer.render_kv(&key, &value))
        .collect::<Vec<_>>();

    renderer.render_section("Constraints", &lines)
}

fn render_approval_text(approval: &ApprovalRequirement, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = approval_rows(approval)
        .into_iter()
        .map(|(key, value)| renderer.render_kv(&key, &value))
        .collect::<Vec<_>>();

    renderer.render_section("ApprovalRequirement", &lines)
}

fn constraints_tree(constraints: &Constraints) -> TreeNode {
    TreeNode {
        label: "constraints".to_owned(),
        children: constraints_rows(constraints)
            .into_iter()
            .map(|(key, value)| leaf(format!("{key}: {value}")))
            .collect(),
    }
}

fn approval_tree(approval: &ApprovalRequirement) -> TreeNode {
    TreeNode {
        label: "approval".to_owned(),
        children: approval_rows(approval)
            .into_iter()
            .map(|(key, value)| leaf(format!("{key}: {value}")))
            .collect(),
    }
}

fn constraints_rows(constraints: &Constraints) -> Vec<(String, String)> {
    vec![
        (
            "sandbox_profile_id".to_owned(),
            constraints
                .sandbox_profile_id
                .as_ref()
                .map_or_else(dash, |value| value.0.clone()),
        ),
        (
            "max_runtime_seconds".to_owned(),
            constraints
                .max_runtime_seconds
                .map_or_else(dash, |value| value.to_string()),
        ),
        (
            "verification_required".to_owned(),
            constraints.verification_required.to_string(),
        ),
        (
            "dry_run_only".to_owned(),
            constraints.dry_run_only.to_string(),
        ),
        (
            "require_evidence_grade".to_owned(),
            constraints
                .require_evidence_grade
                .map_or_else(dash, evidence_grade_label)
                .to_owned(),
        ),
        (
            "require_human_co_signer".to_owned(),
            constraints.require_human_co_signer.to_string(),
        ),
        (
            "network_policy".to_owned(),
            constraints
                .network_policy
                .map_or_else(dash, network_policy_label)
                .to_owned(),
        ),
        (
            "max_concurrent_per_subject".to_owned(),
            constraints
                .max_concurrent_per_subject
                .map_or_else(dash, |value| value.to_string()),
        ),
        (
            "min_subject_session_class".to_owned(),
            constraints
                .min_subject_session_class
                .map_or_else(dash, session_class_label)
                .to_owned(),
        ),
        (
            "vault_capability_required".to_owned(),
            constraints
                .vault_capability_required
                .as_ref()
                .map_or_else(dash, |value| value.0.clone()),
        ),
        (
            "ttl_seconds".to_owned(),
            constraints.ttl_seconds.to_string(),
        ),
        (
            "expires_at".to_owned(),
            constraints
                .expires_at
                .map_or_else(dash, |value| value.to_rfc3339()),
        ),
    ]
}

fn approval_rows(approval: &ApprovalRequirement) -> Vec<(String, String)> {
    vec![
        ("required".to_owned(), approval.required.to_string()),
        (
            "approval_scope".to_owned(),
            approval_scope_label(approval.approval_scope).to_owned(),
        ),
        ("ttl_seconds".to_owned(), approval.ttl_seconds.to_string()),
        (
            "approver_classes".to_owned(),
            approver_classes_label(&approval.approver_classes),
        ),
        (
            "require_human_co_signer".to_owned(),
            approval.require_human_co_signer.to_string(),
        ),
    ]
}

fn render_field_table(
    left_header: &str,
    right_header: &str,
    rows: Vec<(String, String)>,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![left_header.to_owned(), right_header.to_owned()],
        rows: rows
            .into_iter()
            .map(|(key, value)| vec![key, value])
            .collect(),
        align: vec![TableAlign::Left, TableAlign::Left],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn constraints_summary(constraints: &Constraints) -> String {
    format!(
        "ttl_seconds={}, sandbox_profile_id={}, vault_capability_required={}",
        constraints.ttl_seconds,
        constraints
            .sandbox_profile_id
            .as_ref()
            .map_or("-", |value| value.0.as_str()),
        constraints
            .vault_capability_required
            .as_ref()
            .map_or("-", |value| value.0.as_str())
    )
}

fn approval_summary(approval: &ApprovalRequirement) -> String {
    format!(
        "required={}, scope={}, ttl_seconds={}, approver_classes={}",
        approval.required,
        approval_scope_label(approval.approval_scope),
        approval.ttl_seconds,
        approver_classes_label(&approval.approver_classes)
    )
}

fn styled_decision(decision: Decision, ctx: &RenderContext) -> String {
    let label = decision_label(decision);

    if !ctx.color {
        return label.to_owned();
    }

    let color = match decision {
        Decision::Allow => "32",
        Decision::Deny => "31",
        Decision::RequireApproval => "33",
        Decision::Unspecified => "36",
    };

    format!("\u{1b}[{color}m{label}\u{1b}[0m")
}

const fn decision_label(decision: Decision) -> &'static str {
    match decision {
        Decision::Unspecified => "Unspecified",
        Decision::Allow => "Allow",
        Decision::RequireApproval => "RequireApproval",
        Decision::Deny => "Deny",
    }
}

const fn evidence_grade_label(grade: EvidenceGrade) -> &'static str {
    match grade {
        EvidenceGrade::E2 => "E2",
        EvidenceGrade::E3 => "E3",
        EvidenceGrade::E4 => "E4",
        EvidenceGrade::E5 => "E5",
    }
}

const fn network_policy_label(policy: NetworkPolicy) -> &'static str {
    match policy {
        NetworkPolicy::LocalhostOnly => "LocalhostOnly",
        NetworkPolicy::LanAllowed => "LanAllowed",
        NetworkPolicy::InternetAllowed => "InternetAllowed",
    }
}

const fn session_class_label(class: SessionClass) -> &'static str {
    match class {
        SessionClass::Public => "Public",
        SessionClass::Internal => "Internal",
        SessionClass::Confidential => "Confidential",
        SessionClass::Restricted => "Restricted",
        SessionClass::Recovery => "Recovery",
    }
}

const fn approval_scope_label(scope: ApprovalScope) -> &'static str {
    match scope {
        ApprovalScope::ExactRequestHash => "ExactRequestHash",
    }
}

const fn approver_class_label(class: ApproverClass) -> &'static str {
    match class {
        ApproverClass::Human => "Human",
        ApproverClass::Operator => "Operator",
        ApproverClass::Agent => "Agent",
        ApproverClass::Application => "Application",
        ApproverClass::Service => "Service",
        ApproverClass::Device => "Device",
        ApproverClass::Workflow => "Workflow",
        ApproverClass::RemoteOperator => "RemoteOperator",
    }
}

fn approver_classes_label(classes: &[ApproverClass]) -> String {
    if classes.is_empty() {
        return "-".to_owned();
    }

    classes
        .iter()
        .map(|class| approver_class_label(*class))
        .collect::<Vec<_>>()
        .join(", ")
}

fn dash<T>() -> T
where
    T: From<&'static str>,
{
    T::from("-")
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
