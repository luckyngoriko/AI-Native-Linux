//! Cross-crate renderers for S9 recovery, first-boot, and kernel candidates.

use serde::Serialize;

use aios_recovery::{
    CandidateState, FirstBootContext, FirstBootPhase, FirstBootStatus, KernelCandidate,
    KernelManifest, RecoveryMode, RecoveryState,
};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

const HASH_TRUNCATE_AT: usize = 12;

impl Renderable for RecoveryState {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_recovery_state_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_recovery_state_tree(self, ctx),
            OutputFormat::Table => render_recovery_state_table(self, ctx),
        }
    }
}

impl Renderable for RecoveryMode {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value("mode", &styled_recovery_mode(*self, ctx), self, format, ctx)
    }
}

impl Renderable for FirstBootContext {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_first_boot_context_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_first_boot_context_tree(self, ctx),
            OutputFormat::Table => render_first_boot_context_table(self, ctx),
        }
    }
}

impl Renderable for FirstBootPhase {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "first_boot_phase",
            first_boot_phase_label(*self),
            self,
            format,
            ctx,
        )
    }
}

impl Renderable for FirstBootStatus {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "first_boot_status",
            &styled_first_boot_status(*self, ctx),
            self,
            format,
            ctx,
        )
    }
}

impl Renderable for KernelCandidate {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_kernel_candidate_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_kernel_candidate_tree(self, ctx),
            OutputFormat::Table => render_kernel_candidate_table(self, ctx),
        }
    }
}

impl Renderable for CandidateState {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "candidate_state",
            &styled_candidate_state(*self, ctx),
            self,
            format,
            ctx,
        )
    }
}

impl Renderable for KernelManifest {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_kernel_manifest_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_kernel_manifest_tree(self, ctx),
            OutputFormat::Table => render_kernel_manifest_table(self, ctx),
        }
    }
}

fn render_recovery_state_text(state: &RecoveryState, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("mode", &styled_recovery_mode(state.mode, ctx)),
        renderer.render_kv("entered_at", &optional_datetime(state.entered_at)),
        renderer.render_kv("exit_planned_at", &optional_datetime(state.exit_planned_at)),
        renderer.render_kv("reason", optional_str(state.reason.as_deref())),
    ];

    renderer.render_section("RecoveryState", &lines)
}

fn render_recovery_state_tree(
    state: &RecoveryState,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: "RecoveryState".to_owned(),
        children: vec![
            leaf(format!("mode: {}", styled_recovery_mode(state.mode, ctx))),
            leaf(format!(
                "entered_at: {}",
                optional_datetime(state.entered_at)
            )),
            leaf(format!(
                "exit_planned_at: {}",
                optional_datetime(state.exit_planned_at)
            )),
            leaf(format!("reason: {}", optional_str(state.reason.as_deref()))),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_recovery_state_table(
    state: &RecoveryState,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "mode".to_owned(),
            "entered_at".to_owned(),
            "exit_planned_at".to_owned(),
            "reason".to_owned(),
        ],
        rows: vec![vec![
            styled_recovery_mode(state.mode, ctx),
            optional_datetime(state.entered_at),
            optional_datetime(state.exit_planned_at),
            optional_str(state.reason.as_deref()).to_owned(),
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

fn render_first_boot_context_text(context: &FirstBootContext, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let mut lines = vec![
        renderer.render_kv("boot_id", context.boot_id.as_str()),
        renderer.render_kv("status", &styled_first_boot_status(context.status, ctx)),
        renderer.render_kv("started_at", &context.started_at.to_rfc3339()),
        renderer.render_kv("completed_at", &optional_datetime(context.completed_at)),
        renderer.render_kv(
            "performed_phases",
            &format!("count={}", context.performed_phases.len()),
        ),
    ];
    lines.extend(
        context
            .performed_phases
            .iter()
            .map(|phase| format!("  {}", first_boot_phase_label(*phase))),
    );

    renderer.render_section("FirstBootContext", &lines)
}

fn render_first_boot_context_tree(
    context: &FirstBootContext,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("FirstBootContext {}", context.boot_id.as_str()),
        children: vec![
            leaf(format!(
                "status: {}",
                styled_first_boot_status(context.status, ctx)
            )),
            leaf(format!("started_at: {}", context.started_at.to_rfc3339())),
            leaf(format!(
                "completed_at: {}",
                optional_datetime(context.completed_at)
            )),
            TreeNode {
                label: format!("performed_phases: count={}", context.performed_phases.len()),
                children: context
                    .performed_phases
                    .iter()
                    .map(|phase| leaf(first_boot_phase_label(*phase)))
                    .collect(),
            },
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_first_boot_context_table(
    context: &FirstBootContext,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "boot_id".to_owned(),
            "status".to_owned(),
            "phases".to_owned(),
            "started_at".to_owned(),
            "completed_at".to_owned(),
        ],
        rows: vec![vec![
            context.boot_id.as_str().to_owned(),
            styled_first_boot_status(context.status, ctx),
            context.performed_phases.len().to_string(),
            context.started_at.to_rfc3339(),
            optional_datetime(context.completed_at),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Right,
            TableAlign::Left,
            TableAlign::Left,
        ],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn render_kernel_candidate_text(candidate: &KernelCandidate, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("candidate_id", candidate.candidate_id.as_str()),
        renderer.render_kv("version", &candidate.version),
        renderer.render_kv("kernel_blake3", &truncate_hash(&candidate.kernel_blake3)),
        renderer.render_kv("signing_authority", &candidate.signing_authority),
        renderer.render_kv("state", &styled_candidate_state(candidate.state, ctx)),
        renderer.render_kv("registered_at", &candidate.registered_at.to_rfc3339()),
        renderer.render_kv("manifest_version", &candidate.manifest.version),
    ];

    renderer.render_section("KernelCandidate", &lines)
}

fn render_kernel_candidate_tree(
    candidate: &KernelCandidate,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("KernelCandidate {}", candidate.candidate_id.as_str()),
        children: vec![
            leaf(format!("version: {}", candidate.version)),
            leaf(format!(
                "kernel_blake3: {}",
                truncate_hash(&candidate.kernel_blake3)
            )),
            leaf(format!(
                "signing_authority: {}",
                candidate.signing_authority
            )),
            leaf(format!(
                "state: {}",
                styled_candidate_state(candidate.state, ctx)
            )),
            leaf(format!(
                "registered_at: {}",
                candidate.registered_at.to_rfc3339()
            )),
            kernel_manifest_tree_node(&candidate.manifest),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_kernel_candidate_table(
    candidate: &KernelCandidate,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "candidate_id".to_owned(),
            "version".to_owned(),
            "kernel_blake3".to_owned(),
            "signing_authority".to_owned(),
            "state".to_owned(),
        ],
        rows: vec![vec![
            candidate.candidate_id.as_str().to_owned(),
            candidate.version.clone(),
            truncate_hash(&candidate.kernel_blake3),
            candidate.signing_authority.clone(),
            styled_candidate_state(candidate.state, ctx),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
        ],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn render_kernel_manifest_text(manifest: &KernelManifest, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("version", &manifest.version),
        renderer.render_kv("min_aios_version", &manifest.min_aios_version),
        renderer.render_kv(
            "requires_recovery_install",
            &manifest.requires_recovery_install.to_string(),
        ),
        renderer.render_kv(
            "verification_intent",
            optional_str(manifest.verification_intent.as_deref()),
        ),
        renderer.render_kv("tags", &tag_label(&manifest.tags)),
    ];

    renderer.render_section("KernelManifest", &lines)
}

fn render_kernel_manifest_tree(
    manifest: &KernelManifest,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    TreeRenderer::new(ctx.clone()).render(&kernel_manifest_tree_node(manifest))
}

fn render_kernel_manifest_table(
    manifest: &KernelManifest,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "version".to_owned(),
            "min_aios_version".to_owned(),
            "requires_recovery_install".to_owned(),
            "verification_intent".to_owned(),
            "tags".to_owned(),
        ],
        rows: vec![vec![
            manifest.version.clone(),
            manifest.min_aios_version.clone(),
            manifest.requires_recovery_install.to_string(),
            optional_str(manifest.verification_intent.as_deref()).to_owned(),
            tag_label(&manifest.tags),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
        ],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn kernel_manifest_tree_node(manifest: &KernelManifest) -> TreeNode {
    TreeNode {
        label: "KernelManifest".to_owned(),
        children: vec![
            leaf(format!("version: {}", manifest.version)),
            leaf(format!("min_aios_version: {}", manifest.min_aios_version)),
            leaf(format!(
                "requires_recovery_install: {}",
                manifest.requires_recovery_install
            )),
            leaf(format!(
                "verification_intent: {}",
                optional_str(manifest.verification_intent.as_deref())
            )),
            TreeNode {
                label: format!("tags: count={}", manifest.tags.len()),
                children: manifest.tags.iter().cloned().map(leaf).collect(),
            },
        ],
    }
}

fn render_enum_value<T: Serialize>(
    field: &str,
    label: &str,
    value: &T,
    format: OutputFormat,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    match format {
        OutputFormat::Text => {
            let renderer = TextRenderer::new(ctx.clone());
            Ok(renderer.render_kv(field, label))
        }
        OutputFormat::Json => render_json(value),
        OutputFormat::Tree => {
            let root = TreeNode {
                label: field.to_owned(),
                children: vec![leaf(label)],
            };
            TreeRenderer::new(ctx.clone()).render(&root)
        }
        OutputFormat::Table => {
            let spec = TableSpec {
                headers: vec![field.to_owned()],
                rows: vec![vec![label.to_owned()]],
                align: vec![TableAlign::Left],
            };
            TableRenderer::new(ctx.clone()).render(&spec)
        }
    }
}

fn styled_recovery_mode(mode: RecoveryMode, ctx: &RenderContext) -> String {
    styled_label(recovery_mode_label(mode), recovery_mode_color(mode), ctx)
}

fn styled_first_boot_status(status: FirstBootStatus, ctx: &RenderContext) -> String {
    styled_label(
        first_boot_status_label(status),
        first_boot_status_color(status),
        ctx,
    )
}

fn styled_candidate_state(state: CandidateState, ctx: &RenderContext) -> String {
    styled_label(state.as_wire_str(), candidate_state_color(state), ctx)
}

fn styled_label(label: &str, color: &str, ctx: &RenderContext) -> String {
    if ctx.color {
        format!("\u{1b}[{color}m{label}\u{1b}[0m")
    } else {
        label.to_owned()
    }
}

const fn recovery_mode_color(mode: RecoveryMode) -> &'static str {
    match mode {
        RecoveryMode::Normal => "32",
        RecoveryMode::Recovery => "31",
        RecoveryMode::Degraded => "33",
        RecoveryMode::FirstBoot => "34",
    }
}

const fn first_boot_status_color(status: FirstBootStatus) -> &'static str {
    match status {
        FirstBootStatus::Completed => "32",
        FirstBootStatus::Failed => "31",
        FirstBootStatus::InProgress => "34",
        FirstBootStatus::Skipped => "33",
        FirstBootStatus::NotStarted => "90",
    }
}

const fn candidate_state_color(state: CandidateState) -> &'static str {
    match state {
        CandidateState::GatePassed | CandidateState::APromoted => "32",
        CandidateState::GateFailed | CandidateState::Rollback => "31",
        CandidateState::Building | CandidateState::Gating => "34",
        CandidateState::BDemotedToA => "33",
        CandidateState::Built => "36",
        CandidateState::Retired => "90",
    }
}

const fn recovery_mode_label(mode: RecoveryMode) -> &'static str {
    match mode {
        RecoveryMode::Normal => "NORMAL",
        RecoveryMode::Recovery => "RECOVERY",
        RecoveryMode::Degraded => "DEGRADED",
        RecoveryMode::FirstBoot => "FIRST_BOOT",
    }
}

const fn first_boot_status_label(status: FirstBootStatus) -> &'static str {
    match status {
        FirstBootStatus::NotStarted => "NOT_STARTED",
        FirstBootStatus::InProgress => "IN_PROGRESS",
        FirstBootStatus::Completed => "COMPLETED",
        FirstBootStatus::Failed => "FAILED",
        FirstBootStatus::Skipped => "SKIPPED",
    }
}

const fn first_boot_phase_label(phase: FirstBootPhase) -> &'static str {
    match phase {
        FirstBootPhase::StageInstallerMediaVerified => "STAGE_INSTALLER_MEDIA_VERIFIED",
        FirstBootPhase::StageDiskPartitioned => "STAGE_DISK_PARTITIONED",
        FirstBootPhase::StageKernelInstalled => "STAGE_KERNEL_INSTALLED",
        FirstBootPhase::StageAiosFsInitialized => "STAGE_AIOS_FS_INITIALIZED",
        FirstBootPhase::StageVaultRootGenerated => "STAGE_VAULT_ROOT_GENERATED",
        FirstBootPhase::StageInvariantBundleLoaded => "STAGE_INVARIANT_BUNDLE_LOADED",
        FirstBootPhase::StagePolicyBundleLoaded => "STAGE_POLICY_BUNDLE_LOADED",
        FirstBootPhase::StageIdentityBundleLoaded => "STAGE_IDENTITY_BUNDLE_LOADED",
        FirstBootPhase::StageRecoveryOperatorRegistration => "STAGE_RECOVERY_OPERATOR_REGISTRATION",
        FirstBootPhase::StageAiProviderConfiguration => "STAGE_AI_PROVIDER_CONFIGURATION",
        FirstBootPhase::StageFirstGroupRegistration => "STAGE_FIRST_GROUP_REGISTRATION",
        FirstBootPhase::StageFirstUserRegistration => "STAGE_FIRST_USER_REGISTRATION",
        FirstBootPhase::StageRuntimeServicesStarted => "STAGE_RUNTIME_SERVICES_STARTED",
        FirstBootPhase::StageFirstBootComplete => "STAGE_FIRST_BOOT_COMPLETE",
        FirstBootPhase::StageFailedRequiresRecovery => "STAGE_FAILED_REQUIRES_RECOVERY",
    }
}

fn truncate_hash(hash: &str) -> String {
    hash.chars().take(HASH_TRUNCATE_AT).collect()
}

fn optional_datetime(value: Option<chrono::DateTime<chrono::Utc>>) -> String {
    value.map_or_else(|| "-".to_owned(), |dt| dt.to_rfc3339())
}

fn optional_str(value: Option<&str>) -> &str {
    value.unwrap_or("-")
}

fn tag_label(tags: &[String]) -> String {
    if tags.is_empty() {
        "-".to_owned()
    } else {
        tags.join(", ")
    }
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
