//! Cross-crate renderers for vault capability and override records.

use serde::Serialize;
use serde_json::json;

use aios_vault::{
    CapabilityClass, CapabilityState, KeyMaterialHandle, OverrideBinding, OverrideClass,
    VaultCapability,
};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

const VAULT_HANDLE_MARKER: &str = "<vault-handle>";

impl Renderable for VaultCapability {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_capability_text(self, ctx)),
            OutputFormat::Json => render_capability_json(self),
            OutputFormat::Tree => render_capability_tree(self, ctx),
            OutputFormat::Table => render_capability_table(self, ctx),
        }
    }
}

impl Renderable for CapabilityClass {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "capability_class",
            capability_class_label(*self),
            self,
            format,
            ctx,
        )
    }
}

impl Renderable for CapabilityState {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "capability_state",
            &styled_capability_state(*self, ctx),
            self,
            format,
            ctx,
        )
    }
}

impl Renderable for OverrideBinding {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_override_binding_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_override_binding_tree(self, ctx),
            OutputFormat::Table => render_override_binding_table(self, ctx),
        }
    }
}

impl Renderable for OverrideClass {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "override_class",
            override_class_label(*self),
            self,
            format,
            ctx,
        )
    }
}

impl Renderable for KeyMaterialHandle {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text | OutputFormat::Tree | OutputFormat::Table => {
                Ok(VAULT_HANDLE_MARKER.to_owned())
            }
            OutputFormat::Json => serde_json::to_string(VAULT_HANDLE_MARKER)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
        }
        .map(|rendered| {
            if ctx.redact_secrets {
                rendered
            } else {
                rendered.replace(&self.0, VAULT_HANDLE_MARKER)
            }
        })
    }
}

fn render_capability_text(capability: &VaultCapability, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("capability_id", capability.capability_id.as_str()),
        renderer.render_kv("class", capability_class_label(capability.class)),
        renderer.render_kv("issued_to", &capability.issued_to.to_string()),
        renderer.render_kv("state", &styled_capability_state(capability.state, ctx)),
        format!("key_material_handle: {VAULT_HANDLE_MARKER}"),
    ];

    renderer.render_section("VaultCapability", &lines)
}

fn render_capability_json(capability: &VaultCapability) -> Result<String, RenderError> {
    let value = json!({
        "capability_id": capability.capability_id.as_str(),
        "class": capability_class_label(capability.class),
        "issued_to": capability.issued_to.to_string(),
        "state": capability_state_label(capability.state),
        "key_material_handle": VAULT_HANDLE_MARKER,
    });

    serde_json::to_string(&value).map_err(|err| RenderError::SerializationFailed(err.to_string()))
}

fn render_capability_tree(
    capability: &VaultCapability,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("VaultCapability {}", capability.capability_id.as_str()),
        children: vec![
            leaf(format!(
                "class: {}",
                capability_class_label(capability.class)
            )),
            leaf(format!("issued_to: {}", capability.issued_to)),
            leaf(format!(
                "state: {}",
                styled_capability_state(capability.state, ctx)
            )),
            leaf(format!("key_material_handle: {VAULT_HANDLE_MARKER}")),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_capability_table(
    capability: &VaultCapability,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "capability_id".to_owned(),
            "class".to_owned(),
            "issued_to".to_owned(),
            "state".to_owned(),
            "key_material_handle".to_owned(),
        ],
        rows: vec![vec![
            capability.capability_id.as_str().to_owned(),
            capability_class_label(capability.class).to_owned(),
            capability.issued_to.to_string(),
            styled_capability_state(capability.state, ctx),
            VAULT_HANDLE_MARKER.to_owned(),
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

fn render_override_binding_text(binding: &OverrideBinding, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = override_binding_rows(binding)
        .into_iter()
        .map(|(key, value)| renderer.render_kv(&key, &value))
        .collect::<Vec<_>>();

    renderer.render_section("OverrideBinding", &lines)
}

fn render_override_binding_tree(
    binding: &OverrideBinding,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("OverrideBinding {}", binding.binding_id),
        children: override_binding_rows(binding)
            .into_iter()
            .map(|(key, value)| leaf(format!("{key}: {value}")))
            .collect(),
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_override_binding_table(
    binding: &OverrideBinding,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec!["field".to_owned(), "value".to_owned()],
        rows: override_binding_rows(binding)
            .into_iter()
            .map(|(key, value)| vec![key, value])
            .collect(),
        align: vec![TableAlign::Left, TableAlign::Left],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn override_binding_rows(binding: &OverrideBinding) -> Vec<(String, String)> {
    vec![
        ("binding_id".to_owned(), binding.binding_id.clone()),
        (
            "class".to_owned(),
            override_class_label(binding.class).to_owned(),
        ),
        (
            "granted_by".to_owned(),
            subject_refs_label(&binding.granted_by),
        ),
        ("granted_at".to_owned(), binding.granted_at.to_rfc3339()),
        ("expires_at".to_owned(), binding.expires_at.to_rfc3339()),
        (
            "target_action_id".to_owned(),
            binding
                .target_action_id
                .as_ref()
                .map_or_else(|| "-".to_owned(), |action_id| action_id.as_str().to_owned()),
        ),
        (
            "state".to_owned(),
            override_binding_state_label(binding.state).to_owned(),
        ),
    ]
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

fn styled_capability_state(state: CapabilityState, ctx: &RenderContext) -> String {
    let label = capability_state_label(state);

    if !ctx.color {
        return label.to_owned();
    }

    let color = match state {
        CapabilityState::Active => "32",
        CapabilityState::Draft | CapabilityState::Rotated => "33",
        CapabilityState::Expired | CapabilityState::Revoked | CapabilityState::Discarded => "31",
    };

    format!("\u{1b}[{color}m{label}\u{1b}[0m")
}

const fn capability_class_label(class: CapabilityClass) -> &'static str {
    match class {
        CapabilityClass::KeySign => "KeySign",
        CapabilityClass::KeyVerify => "KeyVerify",
        CapabilityClass::KeyEncrypt => "KeyEncrypt",
        CapabilityClass::KeyDecrypt => "KeyDecrypt",
        CapabilityClass::MacGenerate => "MacGenerate",
        CapabilityClass::MacVerify => "MacVerify",
        CapabilityClass::RandomGenerate => "RandomGenerate",
        CapabilityClass::SecretGet => "SecretGet",
        CapabilityClass::BootstrapKeySign => "BootstrapKeySign",
    }
}

const fn capability_state_label(state: CapabilityState) -> &'static str {
    match state {
        CapabilityState::Draft => "Draft",
        CapabilityState::Active => "Active",
        CapabilityState::Expired => "Expired",
        CapabilityState::Revoked => "Revoked",
        CapabilityState::Rotated => "Rotated",
        CapabilityState::Discarded => "Discarded",
    }
}

const fn override_class_label(class: OverrideClass) -> &'static str {
    match class {
        OverrideClass::StrongSolo => "StrongSolo",
        OverrideClass::DualHuman => "DualHuman",
        OverrideClass::TripleHuman => "TripleHuman",
    }
}

const fn override_binding_state_label(state: aios_vault::OverrideBindingState) -> &'static str {
    match state {
        aios_vault::OverrideBindingState::Granted => "Granted",
        aios_vault::OverrideBindingState::Consumed => "Consumed",
        aios_vault::OverrideBindingState::Revoked => "Revoked",
        aios_vault::OverrideBindingState::Expired => "Expired",
    }
}

fn subject_refs_label(subjects: &[aios_vault::SubjectRef]) -> String {
    if subjects.is_empty() {
        return "-".to_owned();
    }

    subjects
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
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
