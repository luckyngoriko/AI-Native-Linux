//! Cross-crate renderers for L6 apps types (T-125).
//!
//! Implements [`Renderable`] for the five domain types exported by `aios-apps`:
//! [`AppPackage`], [`AppProfile`], [`SessionDescriptor`], [`UpdatePlan`],
//! [`RollbackReceipt`]. Each type renders in all four [`OutputFormat`] variants.

use aios_apps::{
    AppPackage, AppProfile, CompatibilityRating, EcosystemRuntime, EvidenceLevel, RatingDimension,
    RecipeTrustClass, RollbackExitState, RollbackReceipt, SessionDescriptor, SessionState,
    UpdatePlan, UpdatePlanId, UpdateState,
};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

// ---------------------------------------------------------------------------
// AppPackage
// ---------------------------------------------------------------------------

impl Renderable for AppPackage {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_app_package_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_app_package_tree(self, ctx),
            OutputFormat::Table => render_app_package_table(self, ctx),
        }
    }
}

fn render_app_package_text(pkg: &AppPackage, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("package_id", &pkg.package_id.0),
        renderer.render_kv("name", &pkg.name),
        renderer.render_kv("version", &pkg.version),
        renderer.render_kv("manifest_bytes_len", &pkg.manifest_bytes.len().to_string()),
        renderer.render_kv("content_hash_blake3", &pkg.content_hash_blake3),
        renderer.render_kv(
            "ed25519_signature",
            &hex_encode(&pkg.ed25519_signature),
        ),
        renderer.render_kv(
            "signer_public_key",
            &hex_encode(&pkg.signer_public_key),
        ),
        renderer.render_kv("registered_at", &pkg.registered_at.to_rfc3339()),
    ];
    renderer.render_section("AppPackage", &lines)
}

fn render_app_package_tree(pkg: &AppPackage, ctx: &RenderContext) -> Result<String, RenderError> {
    let root = TreeNode {
        label: "AppPackage".to_owned(),
        children: vec![
            leaf(format!("package_id: {}", pkg.package_id)),
            leaf(format!("name: {}", pkg.name)),
            leaf(format!("version: {}", pkg.version)),
            leaf(format!("manifest_bytes_len: {}", pkg.manifest_bytes.len())),
            leaf(format!("content_hash_blake3: {}", pkg.content_hash_blake3)),
            leaf(format!(
                "ed25519_signature: {}",
                hex_encode(&pkg.ed25519_signature)
            )),
            leaf(format!(
                "signer_public_key: {}",
                hex_encode(&pkg.signer_public_key)
            )),
            leaf(format!("registered_at: {}", pkg.registered_at.to_rfc3339())),
        ],
    };
    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_app_package_table(pkg: &AppPackage, ctx: &RenderContext) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "package_id".to_owned(),
            "name".to_owned(),
            "version".to_owned(),
            "manifest_bytes_len".to_owned(),
            "content_hash_blake3".to_owned(),
            "ed25519_signature".to_owned(),
            "signer_public_key".to_owned(),
            "registered_at".to_owned(),
        ],
        rows: vec![vec![
            pkg.package_id.0.clone(),
            pkg.name.clone(),
            pkg.version.clone(),
            pkg.manifest_bytes.len().to_string(),
            pkg.content_hash_blake3.clone(),
            hex_encode(&pkg.ed25519_signature),
            hex_encode(&pkg.signer_public_key),
            pkg.registered_at.to_rfc3339(),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Right,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
        ],
    };
    TableRenderer::new(ctx.clone()).render(&spec)
}

// ---------------------------------------------------------------------------
// AppProfile
// ---------------------------------------------------------------------------

impl Renderable for AppProfile {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_app_profile_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_app_profile_tree(self, ctx),
            OutputFormat::Table => render_app_profile_table(self, ctx),
        }
    }
}

fn render_app_profile_text(profile: &AppProfile, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("app_id", &profile.app_id),
        renderer.render_kv(
            "ecosystem_runtime",
            &profile.ecosystem_runtime.to_string(),
        ),
        renderer.render_kv(
            "current_recipe_trust_class",
            &profile.current_recipe_trust_class.to_string(),
        ),
        renderer.render_kv(
            "headline_rating",
            &styled_compat_rating(profile.headline_rating, ctx),
        ),
        renderer.render_kv(
            "headline_evidence_level",
            &profile.headline_evidence_level.to_string(),
        ),
        renderer.render_kv("worst_dimension", &profile.worst_dimension.to_string()),
        renderer.render_kv(
            "ecosystem_honesty_class",
            &profile.ecosystem_honesty_class.to_string(),
        ),
    ];
    renderer.render_section("AppProfile", &lines)
}

fn render_app_profile_tree(
    profile: &AppProfile,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: "AppProfile".to_owned(),
        children: vec![
            leaf(format!("app_id: {}", profile.app_id)),
            leaf(format!(
                "ecosystem_runtime: {}",
                profile.ecosystem_runtime
            )),
            leaf(format!(
                "current_recipe_trust_class: {}",
                profile.current_recipe_trust_class
            )),
            leaf(format!(
                "headline_rating: {}",
                styled_compat_rating(profile.headline_rating, ctx)
            )),
            leaf(format!(
                "headline_evidence_level: {}",
                profile.headline_evidence_level
            )),
            leaf(format!("worst_dimension: {}", profile.worst_dimension)),
            leaf(format!(
                "ecosystem_honesty_class: {}",
                profile.ecosystem_honesty_class
            )),
        ],
    };
    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_app_profile_table(
    profile: &AppProfile,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "app_id".to_owned(),
            "ecosystem_runtime".to_owned(),
            "recipe_trust_class".to_owned(),
            "headline_rating".to_owned(),
            "headline_evidence_level".to_owned(),
            "worst_dimension".to_owned(),
            "honesty_class".to_owned(),
        ],
        rows: vec![vec![
            profile.app_id.clone(),
            profile.ecosystem_runtime.to_string(),
            profile.current_recipe_trust_class.to_string(),
            styled_compat_rating(profile.headline_rating, ctx),
            profile.headline_evidence_level.to_string(),
            profile.worst_dimension.to_string(),
            profile.ecosystem_honesty_class.to_string(),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
        ],
    };
    TableRenderer::new(ctx.clone()).render(&spec)
}

// ---------------------------------------------------------------------------
// SessionDescriptor
// ---------------------------------------------------------------------------

impl Renderable for SessionDescriptor {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_session_descriptor_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_session_descriptor_tree(self, ctx),
            OutputFormat::Table => render_session_descriptor_table(self, ctx),
        }
    }
}

fn render_session_descriptor_text(sess: &SessionDescriptor, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("session_id", &sess.session_id.0.clone()),
        renderer.render_kv("package_id", &sess.package_id.0.clone()),
        renderer.render_kv("ecosystem", &sess.ecosystem.to_string()),
        renderer.render_kv("state", &styled_session_state(sess.state, ctx)),
        renderer.render_kv("requester", &sess.requester.canonical_id),
        renderer.render_kv("created_at", &sess.created_at.to_rfc3339()),
        renderer.render_kv("last_heartbeat", &sess.last_heartbeat.to_rfc3339()),
        renderer.render_kv(
            "timeout_seconds",
            &sess.timeout_seconds.to_string(),
        ),
    ];
    renderer.render_section("SessionDescriptor", &lines)
}

fn render_session_descriptor_tree(
    sess: &SessionDescriptor,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: "SessionDescriptor".to_owned(),
        children: vec![
            leaf(format!("session_id: {}", sess.session_id)),
            leaf(format!("package_id: {}", sess.package_id)),
            leaf(format!("ecosystem: {}", sess.ecosystem)),
            leaf(format!(
                "state: {}",
                styled_session_state(sess.state, ctx)
            )),
            leaf(format!("requester: {}", sess.requester.canonical_id)),
            leaf(format!("created_at: {}", sess.created_at.to_rfc3339())),
            leaf(format!(
                "last_heartbeat: {}",
                sess.last_heartbeat.to_rfc3339()
            )),
            leaf(format!("timeout_seconds: {}", sess.timeout_seconds)),
        ],
    };
    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_session_descriptor_table(
    sess: &SessionDescriptor,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "session_id".to_owned(),
            "package_id".to_owned(),
            "ecosystem".to_owned(),
            "state".to_owned(),
            "requester".to_owned(),
            "created_at".to_owned(),
            "last_heartbeat".to_owned(),
            "timeout_seconds".to_owned(),
        ],
        rows: vec![vec![
            sess.session_id.0.clone(),
            sess.package_id.0.clone(),
            sess.ecosystem.to_string(),
            styled_session_state(sess.state, ctx),
            sess.requester.canonical_id.clone(),
            sess.created_at.to_rfc3339(),
            sess.last_heartbeat.to_rfc3339(),
            sess.timeout_seconds.to_string(),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Right,
        ],
    };
    TableRenderer::new(ctx.clone()).render(&spec)
}

// ---------------------------------------------------------------------------
// UpdatePlan
// ---------------------------------------------------------------------------

impl Renderable for UpdatePlan {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_update_plan_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_update_plan_tree(self, ctx),
            OutputFormat::Table => render_update_plan_table(self, ctx),
        }
    }
}

fn render_update_plan_text(plan: &UpdatePlan, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let failure = plan
        .failure_class
        .as_ref()
        .map_or_else(|| "-".to_owned(), |fc| fc.to_string());
    let lines = vec![
        renderer.render_kv("id", &plan.id.0.clone()),
        renderer.render_kv("package_id", &plan.package_id.0.clone()),
        renderer.render_kv("from_version", &plan.from_version),
        renderer.render_kv("to_version", &plan.to_version),
        renderer.render_kv("state", &styled_update_state(plan.state, ctx)),
        renderer.render_kv("failure_class", &failure),
        renderer.render_kv("created_at", &plan.created_at.to_rfc3339()),
        renderer.render_kv(
            "state_changed_at",
            &plan.state_changed_at.to_rfc3339(),
        ),
    ];
    renderer.render_section("UpdatePlan", &lines)
}

fn render_update_plan_tree(plan: &UpdatePlan, ctx: &RenderContext) -> Result<String, RenderError> {
    let failure = plan
        .failure_class
        .as_ref()
        .map_or_else(|| "-".to_owned(), |fc| fc.to_string());
    let root = TreeNode {
        label: "UpdatePlan".to_owned(),
        children: vec![
            leaf(format!("id: {}", plan.id)),
            leaf(format!("package_id: {}", plan.package_id)),
            leaf(format!("from_version: {}", plan.from_version)),
            leaf(format!("to_version: {}", plan.to_version)),
            leaf(format!(
                "state: {}",
                styled_update_state(plan.state, ctx)
            )),
            leaf(format!("failure_class: {failure}")),
            leaf(format!("created_at: {}", plan.created_at.to_rfc3339())),
            leaf(format!(
                "state_changed_at: {}",
                plan.state_changed_at.to_rfc3339()
            )),
        ],
    };
    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_update_plan_table(plan: &UpdatePlan, ctx: &RenderContext) -> Result<String, RenderError> {
    let failure = plan
        .failure_class
        .as_ref()
        .map_or_else(|| "-".to_owned(), |fc| fc.to_string());
    let spec = TableSpec {
        headers: vec![
            "id".to_owned(),
            "package_id".to_owned(),
            "from_version".to_owned(),
            "to_version".to_owned(),
            "state".to_owned(),
            "failure_class".to_owned(),
            "created_at".to_owned(),
            "state_changed_at".to_owned(),
        ],
        rows: vec![vec![
            plan.id.0.clone(),
            plan.package_id.0.clone(),
            plan.from_version.clone(),
            plan.to_version.clone(),
            styled_update_state(plan.state, ctx),
            failure,
            plan.created_at.to_rfc3339(),
            plan.state_changed_at.to_rfc3339(),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
        ],
    };
    TableRenderer::new(ctx.clone()).render(&spec)
}

// ---------------------------------------------------------------------------
// RollbackReceipt
// ---------------------------------------------------------------------------

impl Renderable for RollbackReceipt {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_rollback_receipt_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_rollback_receipt_tree(self, ctx),
            OutputFormat::Table => render_rollback_receipt_table(self, ctx),
        }
    }
}

fn render_rollback_receipt_text(receipt: &RollbackReceipt, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("plan_id", &receipt.plan_id.0.clone()),
        renderer.render_kv("reverted_to", &receipt.reverted_to),
        renderer.render_kv("completed_at", &receipt.completed_at.to_rfc3339()),
        renderer.render_kv(
            "exit_state",
            &styled_rollback_exit_state(receipt.exit_state, ctx),
        ),
    ];
    renderer.render_section("RollbackReceipt", &lines)
}

fn render_rollback_receipt_tree(
    receipt: &RollbackReceipt,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: "RollbackReceipt".to_owned(),
        children: vec![
            leaf(format!("plan_id: {}", receipt.plan_id)),
            leaf(format!("reverted_to: {}", receipt.reverted_to)),
            leaf(format!(
                "completed_at: {}",
                receipt.completed_at.to_rfc3339()
            )),
            leaf(format!(
                "exit_state: {}",
                styled_rollback_exit_state(receipt.exit_state, ctx)
            )),
        ],
    };
    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_rollback_receipt_table(
    receipt: &RollbackReceipt,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "plan_id".to_owned(),
            "reverted_to".to_owned(),
            "completed_at".to_owned(),
            "exit_state".to_owned(),
        ],
        rows: vec![vec![
            receipt.plan_id.0.clone(),
            receipt.reverted_to.clone(),
            receipt.completed_at.to_rfc3339(),
            styled_rollback_exit_state(receipt.exit_state, ctx),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
        ],
    };
    TableRenderer::new(ctx.clone()).render(&spec)
}

// ---------------------------------------------------------------------------
// Styled enum helpers
// ---------------------------------------------------------------------------

fn styled_compat_rating(rating: CompatibilityRating, ctx: &RenderContext) -> String {
    let (label, color) = compat_rating_label_color(rating);
    styled_label(label, color, ctx)
}

const fn compat_rating_label_color(rating: CompatibilityRating) -> (&'static str, &'static str) {
    match rating {
        CompatibilityRating::Platinum => ("PLATINUM", "36"),
        CompatibilityRating::Gold => ("GOLD", "33"),
        CompatibilityRating::Silver => ("SILVER", "37"),
        CompatibilityRating::Bronze => ("BRONZE", "33"),
        CompatibilityRating::Borked => ("BORKED", "31"),
    }
}

fn styled_session_state(state: SessionState, ctx: &RenderContext) -> String {
    let (label, color) = session_state_label_color(state);
    styled_label(label, color, ctx)
}

const fn session_state_label_color(state: SessionState) -> (&'static str, &'static str) {
    match state {
        SessionState::Active => ("ACTIVE", "32"),
        SessionState::Allocating => ("ALLOCATING", "33"),
        SessionState::Suspended => ("SUSPENDED", "33"),
        SessionState::Terminating => ("TERMINATING", "31"),
        SessionState::Terminated => ("TERMINATED", "31"),
    }
}

fn styled_update_state(state: UpdateState, ctx: &RenderContext) -> String {
    let (label, color) = update_state_label_color(state);
    styled_label(label, color, ctx)
}

const fn update_state_label_color(state: UpdateState) -> (&'static str, &'static str) {
    match state {
        UpdateState::Planned => ("PLANNED", "34"),
        UpdateState::Executing => ("EXECUTING", "33"),
        UpdateState::Executed => ("EXECUTED", "36"),
        UpdateState::Verifying => ("VERIFYING", "33"),
        UpdateState::Verified => ("VERIFIED", "32"),
        UpdateState::Activating => ("ACTIVATING", "33"),
        UpdateState::Active => ("ACTIVE", "32"),
        UpdateState::Failed => ("FAILED", "31"),
        UpdateState::RollingBack => ("ROLLING_BACK", "33"),
        UpdateState::RolledBack => ("ROLLED_BACK", "32"),
        UpdateState::RollbackFailed => ("ROLLBACK_FAILED", "31"),
    }
}

fn styled_rollback_exit_state(state: RollbackExitState, ctx: &RenderContext) -> String {
    let (label, color) = rollback_exit_state_label_color(state);
    styled_label(label, color, ctx)
}

const fn rollback_exit_state_label_color(
    state: RollbackExitState,
) -> (&'static str, &'static str) {
    match state {
        RollbackExitState::Reverted => ("REVERTED", "32"),
        RollbackExitState::PartialRevert => ("PARTIAL_REVERT", "33"),
        RollbackExitState::RollbackFailed => ("ROLLBACK_FAILED", "31"),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn render_json<T: serde::Serialize>(value: &T) -> Result<String, RenderError> {
    serde_json::to_string(value).map_err(|err| RenderError::SerializationFailed(err.to_string()))
}

fn leaf(label: impl Into<String>) -> TreeNode {
    TreeNode {
        label: label.into(),
        children: Vec::new(),
    }
}

fn styled_label(label: &str, color: &str, ctx: &RenderContext) -> String {
    if ctx.color {
        format!("\u{1b}[{color}m{label}\u{1b}[0m")
    } else {
        label.to_owned()
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
