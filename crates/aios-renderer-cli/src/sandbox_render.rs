//! Cross-crate renderers for S3.2 sandbox types (T-113).

use serde::Serialize;

use aios_sandbox::{
    GpuCapabilityBinding, GpuCapabilityClass, GpuPolicy, IommuStatus, IsolationKind,
    NetworkPosture, ResourceLimits, SandboxProfile,
};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

// ---------------------------------------------------------------------------
// IsolationKind
// ---------------------------------------------------------------------------

impl Renderable for IsolationKind {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "isolation_kind",
            &styled_isolation_kind(*self, ctx),
            self,
            format,
            ctx,
        )
    }
}

const fn isolation_kind_label(kind: IsolationKind) -> &'static str {
    match kind {
        IsolationKind::NamespaceLocal => "NAMESPACE_LOCAL",
        IsolationKind::ProcessContainer => "PROCESS_CONTAINER",
        IsolationKind::VmGuest => "VM_GUEST",
        IsolationKind::BrowserOriginIsolated => "BROWSER_ORIGIN_ISOLATED",
        IsolationKind::NoIsolation => "NO_ISOLATION",
    }
}

const fn isolation_kind_color(kind: IsolationKind) -> &'static str {
    match kind {
        IsolationKind::VmGuest | IsolationKind::BrowserOriginIsolated => "32",
        IsolationKind::ProcessContainer => "34",
        IsolationKind::NamespaceLocal => "33",
        IsolationKind::NoIsolation => "31",
    }
}

fn styled_isolation_kind(kind: IsolationKind, ctx: &RenderContext) -> String {
    styled_label(isolation_kind_label(kind), isolation_kind_color(kind), ctx)
}

// ---------------------------------------------------------------------------
// GpuCapabilityClass
// ---------------------------------------------------------------------------

impl Renderable for GpuCapabilityClass {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "gpu_capability_class",
            &styled_gpu_capability_class(*self, ctx),
            self,
            format,
            ctx,
        )
    }
}

const fn gpu_capability_class_label(class: GpuCapabilityClass) -> &'static str {
    match class {
        GpuCapabilityClass::GpuPassiveDisplay => "GPU_PASSIVE_DISPLAY",
        GpuCapabilityClass::GpuBasic2d => "GPU_BASIC_2D",
        GpuCapabilityClass::GpuRich2d => "GPU_RICH_2D",
        GpuCapabilityClass::GpuFull3d => "GPU_FULL_3D",
        GpuCapabilityClass::GpuComputeHeavy => "GPU_COMPUTE_HEAVY",
    }
}

const fn gpu_capability_class_color(class: GpuCapabilityClass) -> &'static str {
    match class {
        GpuCapabilityClass::GpuPassiveDisplay => "31",
        GpuCapabilityClass::GpuBasic2d => "33",
        GpuCapabilityClass::GpuRich2d => "34",
        GpuCapabilityClass::GpuFull3d | GpuCapabilityClass::GpuComputeHeavy => "32",
    }
}

fn styled_gpu_capability_class(class: GpuCapabilityClass, ctx: &RenderContext) -> String {
    styled_label(
        gpu_capability_class_label(class),
        gpu_capability_class_color(class),
        ctx,
    )
}

// ---------------------------------------------------------------------------
// NetworkPosture
// ---------------------------------------------------------------------------

impl Renderable for NetworkPosture {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "network_posture",
            &styled_network_posture(*self, ctx),
            self,
            format,
            ctx,
        )
    }
}

const fn network_posture_label(posture: NetworkPosture) -> &'static str {
    match posture {
        NetworkPosture::DenyAll => "DENY_ALL",
        NetworkPosture::LoopbackOnly => "LOOPBACK_ONLY",
        NetworkPosture::HostLimited => "HOST_LIMITED",
        NetworkPosture::ExplicitAllowlist => "EXPLICIT_ALLOWLIST",
        NetworkPosture::Full => "FULL",
    }
}

const fn network_posture_color(posture: NetworkPosture) -> &'static str {
    match posture {
        NetworkPosture::DenyAll => "31",
        NetworkPosture::LoopbackOnly => "34",
        NetworkPosture::HostLimited => "33",
        NetworkPosture::ExplicitAllowlist => "32",
        NetworkPosture::Full => "35",
    }
}

fn styled_network_posture(posture: NetworkPosture, ctx: &RenderContext) -> String {
    styled_label(
        network_posture_label(posture),
        network_posture_color(posture),
        ctx,
    )
}

// ---------------------------------------------------------------------------
// IommuStatus
// ---------------------------------------------------------------------------

impl Renderable for IommuStatus {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "iommu_status",
            &styled_iommu_status(*self, ctx),
            self,
            format,
            ctx,
        )
    }
}

const fn iommu_status_label(status: IommuStatus) -> &'static str {
    match status {
        IommuStatus::Available => "AVAILABLE",
        IommuStatus::Unavailable => "UNAVAILABLE",
        IommuStatus::Unknown => "UNKNOWN",
    }
}

const fn iommu_status_color(status: IommuStatus) -> &'static str {
    match status {
        IommuStatus::Available => "32",
        IommuStatus::Unavailable => "31",
        IommuStatus::Unknown => "33",
    }
}

fn styled_iommu_status(status: IommuStatus, ctx: &RenderContext) -> String {
    styled_label(iommu_status_label(status), iommu_status_color(status), ctx)
}

// ---------------------------------------------------------------------------
// ResourceLimits
// ---------------------------------------------------------------------------

impl Renderable for ResourceLimits {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_resource_limits_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_resource_limits_tree(self, ctx),
            OutputFormat::Table => render_resource_limits_table(self, ctx),
        }
    }
}

fn render_resource_limits_text(limits: &ResourceLimits, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("cpu_quota_percent", &limits.cpu_quota_percent.to_string()),
        renderer.render_kv("memory_max_bytes", &limits.memory_max_bytes.to_string()),
        renderer.render_kv(
            "io_max_bytes_per_sec",
            &optional_u64(limits.io_max_bytes_per_sec),
        ),
        renderer.render_kv(
            "network_max_bytes_per_sec",
            &optional_u64(limits.network_max_bytes_per_sec),
        ),
        renderer.render_kv("process_max_count", &optional_u32(limits.process_max_count)),
        renderer.render_kv(
            "file_descriptor_max",
            &optional_u32(limits.file_descriptor_max),
        ),
        renderer.render_kv("expires_at", &optional_datetime(limits.expires_at)),
    ];
    renderer.render_section("ResourceLimits", &lines)
}

fn render_resource_limits_tree(
    limits: &ResourceLimits,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: "ResourceLimits".to_owned(),
        children: vec![
            leaf(format!("cpu_quota_percent: {}", limits.cpu_quota_percent)),
            leaf(format!("memory_max_bytes: {}", limits.memory_max_bytes)),
            leaf(format!(
                "io_max_bytes_per_sec: {}",
                optional_u64(limits.io_max_bytes_per_sec)
            )),
            leaf(format!(
                "network_max_bytes_per_sec: {}",
                optional_u64(limits.network_max_bytes_per_sec)
            )),
            leaf(format!(
                "process_max_count: {}",
                optional_u32(limits.process_max_count)
            )),
            leaf(format!(
                "file_descriptor_max: {}",
                optional_u32(limits.file_descriptor_max)
            )),
            leaf(format!(
                "expires_at: {}",
                optional_datetime(limits.expires_at)
            )),
        ],
    };
    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_resource_limits_table(
    limits: &ResourceLimits,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "cpu_quota_percent".to_owned(),
            "memory_max_bytes".to_owned(),
            "io_max_bytes_per_sec".to_owned(),
            "network_max_bytes_per_sec".to_owned(),
            "process_max_count".to_owned(),
            "file_descriptor_max".to_owned(),
            "expires_at".to_owned(),
        ],
        rows: vec![vec![
            limits.cpu_quota_percent.to_string(),
            limits.memory_max_bytes.to_string(),
            optional_u64(limits.io_max_bytes_per_sec),
            optional_u64(limits.network_max_bytes_per_sec),
            optional_u32(limits.process_max_count),
            optional_u32(limits.file_descriptor_max),
            optional_datetime(limits.expires_at),
        ]],
        align: vec![
            TableAlign::Right,
            TableAlign::Right,
            TableAlign::Right,
            TableAlign::Right,
            TableAlign::Right,
            TableAlign::Right,
            TableAlign::Left,
        ],
    };
    TableRenderer::new(ctx.clone()).render(&spec)
}

// ---------------------------------------------------------------------------
// GpuPolicy
// ---------------------------------------------------------------------------

impl Renderable for GpuPolicy {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_gpu_policy_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_gpu_policy_tree(self, ctx),
            OutputFormat::Table => render_gpu_policy_table(self, ctx),
        }
    }
}

fn render_gpu_policy_text(policy: &GpuPolicy, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv(
            "gpu_capability_class",
            &styled_gpu_capability_class(policy.gpu_capability_class, ctx),
        ),
        renderer.render_kv("vk_device_required", &policy.vk_device_required.to_string()),
        renderer.render_kv(
            "dmabuf_passthrough_allowed",
            &policy.dmabuf_passthrough_allowed.to_string(),
        ),
        renderer.render_kv(
            "per_group_partitioning",
            &policy.per_group_partitioning.to_string(),
        ),
        renderer.render_kv("iommu_required", &policy.iommu_required.to_string()),
        renderer.render_kv("expires_at", &optional_datetime(policy.expires_at)),
    ];
    renderer.render_section("GpuPolicy", &lines)
}

fn render_gpu_policy_tree(policy: &GpuPolicy, ctx: &RenderContext) -> Result<String, RenderError> {
    let root = TreeNode {
        label: "GpuPolicy".to_owned(),
        children: vec![
            leaf(format!(
                "gpu_capability_class: {}",
                styled_gpu_capability_class(policy.gpu_capability_class, ctx)
            )),
            leaf(format!("vk_device_required: {}", policy.vk_device_required)),
            leaf(format!(
                "dmabuf_passthrough_allowed: {}",
                policy.dmabuf_passthrough_allowed
            )),
            leaf(format!(
                "per_group_partitioning: {}",
                policy.per_group_partitioning
            )),
            leaf(format!("iommu_required: {}", policy.iommu_required)),
            leaf(format!(
                "expires_at: {}",
                optional_datetime(policy.expires_at)
            )),
        ],
    };
    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_gpu_policy_table(policy: &GpuPolicy, ctx: &RenderContext) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "gpu_capability_class".to_owned(),
            "vk_device_required".to_owned(),
            "dmabuf_passthrough_allowed".to_owned(),
            "per_group_partitioning".to_owned(),
            "iommu_required".to_owned(),
            "expires_at".to_owned(),
        ],
        rows: vec![vec![
            styled_gpu_capability_class(policy.gpu_capability_class, ctx),
            policy.vk_device_required.to_string(),
            policy.dmabuf_passthrough_allowed.to_string(),
            policy.per_group_partitioning.to_string(),
            policy.iommu_required.to_string(),
            optional_datetime(policy.expires_at),
        ]],
        align: vec![
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
// GpuCapabilityBinding
// ---------------------------------------------------------------------------

impl Renderable for GpuCapabilityBinding {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_gpu_binding_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_gpu_binding_tree(self, ctx),
            OutputFormat::Table => render_gpu_binding_table(self, ctx),
        }
    }
}

fn render_gpu_binding_text(binding: &GpuCapabilityBinding, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("binding_id", &binding.binding_id),
        renderer.render_kv(
            "gpu_capability_class",
            &styled_gpu_capability_class(binding.gpu_capability_class, ctx),
        ),
        renderer.render_kv("group_id", &binding.group_id),
        renderer.render_kv("subject", &binding.subject.0),
        renderer.render_kv(
            "vk_device_required",
            &binding.vk_device_required.to_string(),
        ),
        renderer.render_kv(
            "dmabuf_passthrough_allowed",
            &binding.dmabuf_passthrough_allowed.to_string(),
        ),
        renderer.render_kv("iommu_required", &binding.iommu_required.to_string()),
        renderer.render_kv(
            "degraded_isolation",
            &binding.degraded_isolation.to_string(),
        ),
        renderer.render_kv("issued_at", &binding.issued_at.to_rfc3339()),
        renderer.render_kv("expires_at", &optional_datetime(binding.expires_at)),
    ];
    renderer.render_section("GpuCapabilityBinding", &lines)
}

fn render_gpu_binding_tree(
    binding: &GpuCapabilityBinding,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("GpuCapabilityBinding {}", binding.binding_id),
        children: vec![
            leaf(format!(
                "gpu_capability_class: {}",
                styled_gpu_capability_class(binding.gpu_capability_class, ctx)
            )),
            leaf(format!("group_id: {}", binding.group_id)),
            leaf(format!("subject: {}", &binding.subject.0)),
            leaf(format!(
                "vk_device_required: {}",
                binding.vk_device_required
            )),
            leaf(format!(
                "dmabuf_passthrough_allowed: {}",
                binding.dmabuf_passthrough_allowed
            )),
            leaf(format!("iommu_required: {}", binding.iommu_required)),
            leaf(format!(
                "degraded_isolation: {}",
                binding.degraded_isolation
            )),
            leaf(format!("issued_at: {}", binding.issued_at.to_rfc3339())),
            leaf(format!(
                "expires_at: {}",
                optional_datetime(binding.expires_at)
            )),
        ],
    };
    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_gpu_binding_table(
    binding: &GpuCapabilityBinding,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "binding_id".to_owned(),
            "gpu_capability_class".to_owned(),
            "group_id".to_owned(),
            "subject".to_owned(),
            "vk_device_required".to_owned(),
            "dmabuf_passthrough_allowed".to_owned(),
            "iommu_required".to_owned(),
            "degraded_isolation".to_owned(),
            "issued_at".to_owned(),
            "expires_at".to_owned(),
        ],
        rows: vec![vec![
            binding.binding_id.clone(),
            styled_gpu_capability_class(binding.gpu_capability_class, ctx),
            binding.group_id.clone(),
            binding.subject.to_string(),
            binding.vk_device_required.to_string(),
            binding.dmabuf_passthrough_allowed.to_string(),
            binding.iommu_required.to_string(),
            binding.degraded_isolation.to_string(),
            binding.issued_at.to_rfc3339(),
            optional_datetime(binding.expires_at),
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
            TableAlign::Left,
            TableAlign::Left,
        ],
    };
    TableRenderer::new(ctx.clone()).render(&spec)
}

// ---------------------------------------------------------------------------
// SandboxProfile
// ---------------------------------------------------------------------------

impl Renderable for SandboxProfile {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_sandbox_profile_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_sandbox_profile_tree(self, ctx),
            OutputFormat::Table => render_sandbox_profile_table(self, ctx),
        }
    }
}

fn render_sandbox_profile_text(profile: &SandboxProfile, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let syscalls_label = profile
        .syscall_allowlist
        .as_ref()
        .map_or_else(|| "-".to_owned(), |list| format!("count={}", list.len()));
    let lines = vec![
        renderer.render_kv("profile_id", profile.profile_id.to_string().as_str()),
        renderer.render_kv("name", &profile.name),
        renderer.render_kv("description", &profile.description),
        renderer.render_kv(
            "isolation_kind",
            &styled_isolation_kind(profile.isolation_kind, ctx),
        ),
        renderer.render_kv(
            "network_posture",
            &styled_network_posture(profile.network_posture, ctx),
        ),
        renderer.render_kv(
            "cpu_quota_percent",
            &profile.resource_limits.cpu_quota_percent.to_string(),
        ),
        renderer.render_kv(
            "memory_max_bytes",
            &profile.resource_limits.memory_max_bytes.to_string(),
        ),
        renderer.render_kv(
            "gpu_capability_class",
            &styled_gpu_capability_class(profile.gpu_policy.gpu_capability_class, ctx),
        ),
        renderer.render_kv("syscall_allowlist", &syscalls_label),
        renderer.render_kv("signing_authority", &profile.signing_authority),
    ];
    renderer.render_section("SandboxProfile", &lines)
}

fn render_sandbox_profile_tree(
    profile: &SandboxProfile,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let syscall_children: Vec<TreeNode> = profile
        .syscall_allowlist
        .as_ref()
        .map(|list| list.iter().map(|syscall| leaf(syscall.clone())).collect())
        .unwrap_or_default();
    let syscall_label = format!(
        "syscall_allowlist: count={}",
        profile.syscall_allowlist.as_ref().map_or(0, Vec::len)
    );
    let root = TreeNode {
        label: format!("SandboxProfile {}", profile.profile_id),
        children: vec![
            leaf(format!("name: {}", profile.name)),
            leaf(format!("description: {}", profile.description)),
            leaf(format!(
                "isolation_kind: {}",
                styled_isolation_kind(profile.isolation_kind, ctx)
            )),
            leaf(format!(
                "network_posture: {}",
                styled_network_posture(profile.network_posture, ctx)
            )),
            leaf(format!(
                "cpu_quota_percent: {}",
                profile.resource_limits.cpu_quota_percent
            )),
            leaf(format!(
                "memory_max_bytes: {}",
                profile.resource_limits.memory_max_bytes
            )),
            leaf(format!(
                "gpu_capability_class: {}",
                styled_gpu_capability_class(profile.gpu_policy.gpu_capability_class, ctx)
            )),
            TreeNode {
                label: syscall_label,
                children: syscall_children,
            },
            leaf(format!("signing_authority: {}", profile.signing_authority)),
        ],
    };
    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_sandbox_profile_table(
    profile: &SandboxProfile,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "profile_id".to_owned(),
            "name".to_owned(),
            "description".to_owned(),
            "isolation_kind".to_owned(),
            "network_posture".to_owned(),
            "cpu_pct".to_owned(),
            "mem_bytes".to_owned(),
            "gpu_class".to_owned(),
            "syscalls".to_owned(),
            "signing_authority".to_owned(),
        ],
        rows: vec![vec![
            profile.profile_id.to_string(),
            profile.name.clone(),
            profile.description.clone(),
            styled_isolation_kind(profile.isolation_kind, ctx),
            styled_network_posture(profile.network_posture, ctx),
            profile.resource_limits.cpu_quota_percent.to_string(),
            profile.resource_limits.memory_max_bytes.to_string(),
            styled_gpu_capability_class(profile.gpu_policy.gpu_capability_class, ctx),
            profile
                .syscall_allowlist
                .as_ref()
                .map_or_else(|| "-".to_owned(), |l| l.len().to_string()),
            profile.signing_authority.clone(),
        ]],
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Right,
            TableAlign::Right,
            TableAlign::Left,
            TableAlign::Right,
            TableAlign::Left,
        ],
    };
    TableRenderer::new(ctx.clone()).render(&spec)
}

// ---------------------------------------------------------------------------
// SandboxProfileListView — wrapper for Vec<SandboxProfile>
// ---------------------------------------------------------------------------

/// Wrapper for rendering a list of sandbox profiles.
pub struct SandboxProfileListView(pub Vec<SandboxProfile>);

impl Renderable for SandboxProfileListView {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                let mut lines = vec![renderer.render_kv("profiles", &self.0.len().to_string())];
                lines.extend(
                    self.0
                        .iter()
                        .map(|profile| format!("{} {}", profile.profile_id, profile.name)),
                );
                Ok(renderer.render_section("SandboxProfiles", &lines))
            }
            OutputFormat::Json => render_json(&self.0),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: format!("sandbox_profiles count={}", self.0.len()),
                    children: self
                        .0
                        .iter()
                        .map(|profile| TreeNode {
                            label: profile.profile_id.to_string(),
                            children: vec![
                                leaf(format!("name: {}", profile.name)),
                                leaf(format!(
                                    "isolation_kind: {}",
                                    styled_isolation_kind(profile.isolation_kind, ctx)
                                )),
                                leaf(format!(
                                    "network_posture: {}",
                                    styled_network_posture(profile.network_posture, ctx)
                                )),
                            ],
                        })
                        .collect(),
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec![
                        "profile_id".to_owned(),
                        "name".to_owned(),
                        "isolation_kind".to_owned(),
                        "network_posture".to_owned(),
                        "gpu_class".to_owned(),
                    ],
                    rows: self
                        .0
                        .iter()
                        .map(|profile| {
                            vec![
                                profile.profile_id.to_string(),
                                profile.name.clone(),
                                styled_isolation_kind(profile.isolation_kind, ctx),
                                styled_network_posture(profile.network_posture, ctx),
                                styled_gpu_capability_class(
                                    profile.gpu_policy.gpu_capability_class,
                                    ctx,
                                ),
                            ]
                        })
                        .collect(),
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
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

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

fn styled_label(label: &str, color: &str, ctx: &RenderContext) -> String {
    if ctx.color {
        format!("\u{1b}[{color}m{label}\u{1b}[0m")
    } else {
        label.to_owned()
    }
}

fn optional_u64(value: Option<u64>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

fn optional_u32(value: Option<u32>) -> String {
    value.map_or_else(|| "-".to_owned(), |v| v.to_string())
}

fn optional_datetime(value: Option<chrono::DateTime<chrono::Utc>>) -> String {
    value.map_or_else(|| "-".to_owned(), |dt| dt.to_rfc3339())
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
