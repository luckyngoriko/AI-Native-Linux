//! Cross-crate renderers for AIOS-FS objects, versions, pointers, and paths.

use serde_json::{json, Value};

use aios_fs::{
    AiosPath, LifecycleState, NamespaceClass, Object, ObjectKind, ObjectMetadata, Pointer,
    PointerKind, PrivacyClass, Version, VersionState,
};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

const HASH_TRUNCATE_AT: usize = 12;

impl Renderable for Object {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => render_object_text(self, ctx),
            OutputFormat::Json => render_value(&object_value(self)),
            OutputFormat::Tree => render_object_tree(self, ctx),
            OutputFormat::Table => render_object_table(self, ctx),
        }
    }
}

impl Renderable for Version {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_version_text(self, ctx)),
            OutputFormat::Json => render_value(&version_value(self)),
            OutputFormat::Tree => render_version_tree(self, ctx),
            OutputFormat::Table => render_version_table(self, ctx),
        }
    }
}

impl Renderable for Pointer {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_pointer_text(self, ctx)),
            OutputFormat::Json => render_value(&pointer_value(self)),
            OutputFormat::Tree => render_pointer_tree(self, ctx),
            OutputFormat::Table => render_pointer_table(self, ctx),
        }
    }
}

impl Renderable for AiosPath {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_path_text(self, ctx)),
            OutputFormat::Json => render_value(&path_value(self)),
            OutputFormat::Tree => render_path_tree(self, ctx),
            OutputFormat::Table => render_path_table(self, ctx),
        }
    }
}

impl Renderable for NamespaceClass {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_namespace_text(*self, ctx)),
            OutputFormat::Json => render_value(&namespace_value(*self)),
            OutputFormat::Tree => render_namespace_tree(*self, ctx),
            OutputFormat::Table => render_namespace_table(*self, ctx),
        }
    }
}

fn render_object_text(object: &Object, ctx: &RenderContext) -> Result<String, RenderError> {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("object_id", object.object_id.as_str()),
        renderer.render_kv("kind", object_kind_label(object.kind)),
        renderer.render_kv("privacy_class", privacy_class_label(object.privacy_class)),
        renderer.render_kv(
            "lifecycle_state",
            &styled_lifecycle_state(object.lifecycle_state, ctx),
        ),
        renderer.render_kv("current_pointer_id", object.current_pointer_id.as_str()),
        renderer.render_kv("metadata", &metadata_json(&object.metadata)?),
    ];

    Ok(renderer.render_section("Object", &lines))
}

fn render_object_tree(object: &Object, ctx: &RenderContext) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("Object {}", object.object_id.as_str()),
        children: vec![
            leaf(format!("kind: {}", object_kind_label(object.kind))),
            leaf(format!(
                "privacy_class: {}",
                privacy_class_label(object.privacy_class)
            )),
            leaf(format!(
                "lifecycle_state: {}",
                styled_lifecycle_state(object.lifecycle_state, ctx)
            )),
            leaf(format!(
                "current_pointer_id: {}",
                object.current_pointer_id.as_str()
            )),
            leaf(format!("metadata: {}", metadata_json(&object.metadata)?)),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_object_table(object: &Object, ctx: &RenderContext) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "object_id".to_owned(),
            "kind".to_owned(),
            "privacy_class".to_owned(),
            "lifecycle_state".to_owned(),
            "current_pointer_id".to_owned(),
            "metadata".to_owned(),
        ],
        rows: vec![vec![
            object.object_id.as_str().to_owned(),
            object_kind_label(object.kind).to_owned(),
            privacy_class_label(object.privacy_class).to_owned(),
            styled_lifecycle_state(object.lifecycle_state, ctx),
            object.current_pointer_id.as_str().to_owned(),
            metadata_json(&object.metadata)?,
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

fn render_version_text(version: &Version, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("version_id", version.version_id.as_str()),
        renderer.render_kv("state", &styled_version_state(version.state, ctx)),
        renderer.render_kv("created_at", &version.created_at.to_rfc3339()),
        renderer.render_kv("content_hash", &truncate_hash(&version.content_hash)),
    ];

    renderer.render_section("Version", &lines)
}

fn render_version_tree(version: &Version, ctx: &RenderContext) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("Version {}", version.version_id.as_str()),
        children: vec![
            leaf(format!(
                "state: {}",
                styled_version_state(version.state, ctx)
            )),
            leaf(format!("created_at: {}", version.created_at.to_rfc3339())),
            leaf(format!(
                "content_hash: {}",
                truncate_hash(&version.content_hash)
            )),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_version_table(version: &Version, ctx: &RenderContext) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "version_id".to_owned(),
            "state".to_owned(),
            "created_at".to_owned(),
            "content_hash".to_owned(),
        ],
        rows: vec![vec![
            version.version_id.as_str().to_owned(),
            styled_version_state(version.state, ctx),
            version.created_at.to_rfc3339(),
            truncate_hash(&version.content_hash),
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

fn render_pointer_text(pointer: &Pointer, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("pointer_id", pointer.pointer_id.as_str()),
        renderer.render_kv("kind", pointer_kind_label(pointer.kind)),
        renderer.render_kv("current_version_id", pointer.current_version_id.as_str()),
    ];

    renderer.render_section("Pointer", &lines)
}

fn render_pointer_tree(pointer: &Pointer, ctx: &RenderContext) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("Pointer {}", pointer.pointer_id.as_str()),
        children: vec![
            leaf(format!("kind: {}", pointer_kind_label(pointer.kind))),
            leaf(format!(
                "current_version_id: {}",
                pointer.current_version_id.as_str()
            )),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_pointer_table(pointer: &Pointer, ctx: &RenderContext) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "pointer_id".to_owned(),
            "kind".to_owned(),
            "current_version_id".to_owned(),
        ],
        rows: vec![vec![
            pointer.pointer_id.as_str().to_owned(),
            pointer_kind_label(pointer.kind).to_owned(),
            pointer.current_version_id.as_str().to_owned(),
        ]],
        align: vec![TableAlign::Left, TableAlign::Left, TableAlign::Left],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn render_path_text(path: &AiosPath, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("path", path.as_str()),
        renderer.render_kv("namespace_class", &namespace_class_text(path)),
    ];

    renderer.render_section("AiosPath", &lines)
}

fn render_path_tree(path: &AiosPath, ctx: &RenderContext) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("AiosPath {}", path.as_str()),
        children: vec![leaf(format!(
            "namespace_class: {}",
            namespace_class_text(path)
        ))],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_path_table(path: &AiosPath, ctx: &RenderContext) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec!["path".to_owned(), "namespace_class".to_owned()],
        rows: vec![vec![path.as_str().to_owned(), namespace_class_text(path)]],
        align: vec![TableAlign::Left, TableAlign::Left],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn render_namespace_text(class: NamespaceClass, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = namespace_rows(class)
        .into_iter()
        .map(|(key, value)| renderer.render_kv(&key, &value))
        .collect::<Vec<_>>();

    renderer.render_section("NamespaceClass", &lines)
}

fn render_namespace_tree(
    class: NamespaceClass,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("NamespaceClass {}", namespace_class_label(class)),
        children: namespace_rows(class)
            .into_iter()
            .map(|(key, value)| leaf(format!("{key}: {value}")))
            .collect(),
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_namespace_table(
    class: NamespaceClass,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec!["field".to_owned(), "value".to_owned()],
        rows: namespace_rows(class)
            .into_iter()
            .map(|(key, value)| vec![key, value])
            .collect(),
        align: vec![TableAlign::Left, TableAlign::Left],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn object_value(object: &Object) -> Value {
    json!({
        "object_id": object.object_id.as_str(),
        "kind": object_kind_label(object.kind),
        "privacy_class": privacy_class_label(object.privacy_class),
        "lifecycle_state": lifecycle_state_label(object.lifecycle_state),
        "current_pointer_id": object.current_pointer_id.as_str(),
        "metadata": metadata_value(&object.metadata),
    })
}

fn version_value(version: &Version) -> Value {
    json!({
        "version_id": version.version_id.as_str(),
        "state": version_state_label(version.state),
        "created_at": version.created_at.to_rfc3339(),
        "content_hash": truncate_hash(&version.content_hash),
    })
}

fn pointer_value(pointer: &Pointer) -> Value {
    json!({
        "pointer_id": pointer.pointer_id.as_str(),
        "kind": pointer_kind_label(pointer.kind),
        "current_version_id": pointer.current_version_id.as_str(),
    })
}

fn path_value(path: &AiosPath) -> Value {
    json!({
        "path": path.as_str(),
        "namespace_class": namespace_class_text(path),
    })
}

fn namespace_value(class: NamespaceClass) -> Value {
    json!({
        "namespace_class": namespace_class_label(class),
        "recovery_only_mutation": class.is_recovery_only_mutation(),
        "read_only_for_ai": class.is_read_only_for_ai(),
        "evidence_grade_floor": class.evidence_grade_floor().as_str(),
    })
}

fn metadata_value(metadata: &ObjectMetadata) -> Value {
    json!({
        "name": metadata.name,
        "labels": metadata.labels,
        "mime": metadata.mime,
        "extra": metadata.extra,
    })
}

fn metadata_json(metadata: &ObjectMetadata) -> Result<String, RenderError> {
    render_value(&metadata_value(metadata))
}

fn namespace_rows(class: NamespaceClass) -> Vec<(String, String)> {
    vec![
        (
            "namespace_class".to_owned(),
            namespace_class_label(class).to_owned(),
        ),
        (
            "recovery_only_mutation".to_owned(),
            class.is_recovery_only_mutation().to_string(),
        ),
        (
            "read_only_for_ai".to_owned(),
            class.is_read_only_for_ai().to_string(),
        ),
        (
            "evidence_grade_floor".to_owned(),
            class.evidence_grade_floor().as_str().to_owned(),
        ),
    ]
}

fn namespace_class_text(path: &AiosPath) -> String {
    path.namespace_class().map_or_else(
        || "<unclassified>".to_owned(),
        |class| namespace_class_label(class).to_owned(),
    )
}

fn styled_lifecycle_state(state: LifecycleState, ctx: &RenderContext) -> String {
    let label = lifecycle_state_label(state);

    if !ctx.color {
        return label.to_owned();
    }

    let color = match state {
        LifecycleState::Active => "32",
        LifecycleState::Retired => "33",
        LifecycleState::Purged => "31",
    };

    format!("\u{1b}[{color}m{label}\u{1b}[0m")
}

fn styled_version_state(state: VersionState, ctx: &RenderContext) -> String {
    let label = version_state_label(state);

    if !ctx.color {
        return label.to_owned();
    }

    let color = match state {
        VersionState::Verified => "32",
        VersionState::Staged | VersionState::Quarantined => "33",
        VersionState::RetiredVersion => "31",
    };

    format!("\u{1b}[{color}m{label}\u{1b}[0m")
}

const fn lifecycle_state_label(state: LifecycleState) -> &'static str {
    match state {
        LifecycleState::Active => "Active",
        LifecycleState::Retired => "Retired",
        LifecycleState::Purged => "Purged",
    }
}

const fn version_state_label(state: VersionState) -> &'static str {
    match state {
        VersionState::Staged => "Staged",
        VersionState::Verified => "Verified",
        VersionState::Quarantined => "Quarantined",
        VersionState::RetiredVersion => "RetiredVersion",
    }
}

const fn object_kind_label(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Project => "Project",
        ObjectKind::Application => "Application",
        ObjectKind::File => "File",
        ObjectKind::Memory => "Memory",
        ObjectKind::Policy => "Policy",
        ObjectKind::Model => "Model",
        ObjectKind::Package => "Package",
        ObjectKind::EvidenceRef => "EvidenceRef",
        ObjectKind::Workspace => "Workspace",
        ObjectKind::Config => "Config",
    }
}

const fn privacy_class_label(class: PrivacyClass) -> &'static str {
    match class {
        PrivacyClass::Public => "Public",
        PrivacyClass::Internal => "Internal",
        PrivacyClass::Sensitive => "Sensitive",
        PrivacyClass::SecretBearing => "SecretBearing",
        PrivacyClass::Classified => "Classified",
    }
}

const fn pointer_kind_label(kind: PointerKind) -> &'static str {
    match kind {
        PointerKind::Current => "Current",
        PointerKind::Stable => "Stable",
        PointerKind::Candidate => "Candidate",
        PointerKind::Rollback => "Rollback",
        PointerKind::Quarantine => "Quarantine",
    }
}

const fn namespace_class_label(class: NamespaceClass) -> &'static str {
    match class {
        NamespaceClass::System => "System",
        NamespaceClass::SystemApps => "SystemApps",
        NamespaceClass::SystemAgents => "SystemAgents",
        NamespaceClass::SystemPolicy => "SystemPolicy",
        NamespaceClass::SystemCapabilities => "SystemCapabilities",
        NamespaceClass::SystemEvidence => "SystemEvidence",
        NamespaceClass::SystemVault => "SystemVault",
        NamespaceClass::SystemRuntime => "SystemRuntime",
        NamespaceClass::SystemRecovery => "SystemRecovery",
        NamespaceClass::SystemBoot => "SystemBoot",
        NamespaceClass::SystemFirstboot => "SystemFirstboot",
        NamespaceClass::SystemGovernance => "SystemGovernance",
        NamespaceClass::SystemIdentity => "SystemIdentity",
        NamespaceClass::SystemKernel => "SystemKernel",
        NamespaceClass::SystemHardware => "SystemHardware",
        NamespaceClass::SystemDrivers => "SystemDrivers",
        NamespaceClass::SystemFirmware => "SystemFirmware",
        NamespaceClass::SystemNetwork => "SystemNetwork",
        NamespaceClass::SystemSgr => "SystemSgr",
        NamespaceClass::SystemUnits => "SystemUnits",
        NamespaceClass::SystemRunbooks => "SystemRunbooks",
        NamespaceClass::SystemThemes => "SystemThemes",
        NamespaceClass::SystemRenderers => "SystemRenderers",
        NamespaceClass::SystemWeb => "SystemWeb",
        NamespaceClass::SystemDistribution => "SystemDistribution",
        NamespaceClass::Groups => "Groups",
        NamespaceClass::Group => "Group",
        NamespaceClass::GroupApps => "GroupApps",
        NamespaceClass::GroupAgents => "GroupAgents",
        NamespaceClass::GroupUsers => "GroupUsers",
        NamespaceClass::GroupShared => "GroupShared",
        NamespaceClass::GroupProjects => "GroupProjects",
        NamespaceClass::GroupDatasets => "GroupDatasets",
        NamespaceClass::GroupInbox => "GroupInbox",
        NamespaceClass::GroupPolicy => "GroupPolicy",
        NamespaceClass::GroupEvidence => "GroupEvidence",
        NamespaceClass::GroupVault => "GroupVault",
        NamespaceClass::GroupAudit => "GroupAudit",
        NamespaceClass::GroupServices => "GroupServices",
        NamespaceClass::GroupSystem => "GroupSystem",
        NamespaceClass::User => "User",
        NamespaceClass::UserHome => "UserHome",
        NamespaceClass::UserAgents => "UserAgents",
        NamespaceClass::UserPrefs => "UserPrefs",
        NamespaceClass::UserDesktop => "UserDesktop",
        NamespaceClass::UserInbox => "UserInbox",
        NamespaceClass::UserOutbox => "UserOutbox",
        NamespaceClass::UserDrafts => "UserDrafts",
        NamespaceClass::UserTrust => "UserTrust",
        NamespaceClass::UserApps => "UserApps",
        NamespaceClass::UserRuntime => "UserRuntime",
        NamespaceClass::UserExports => "UserExports",
    }
}

fn truncate_hash(hash: &str) -> String {
    hash.chars().take(HASH_TRUNCATE_AT).collect()
}

fn render_value(value: &Value) -> Result<String, RenderError> {
    serde_json::to_string(value).map_err(|err| RenderError::SerializationFailed(err.to_string()))
}

fn leaf(label: impl Into<String>) -> TreeNode {
    TreeNode {
        label: label.into(),
        children: Vec::new(),
    }
}
