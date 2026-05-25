//! Cross-crate renderers for S15.x Service Graph Runtime types.

use serde::Serialize;
use serde_json::{json, Value};

use aios_sgr::{
    AdapterDeclaration, AdapterRegistrationState, DependencyKind, GraphState, RegisteredAdapter,
    ServiceUnit, UnitId, UnitKind, UnitState,
};

use crate::{
    OutputFormat, RenderContext, RenderError, Renderable, TableAlign, TableRenderer, TableSpec,
    TextRenderer, TreeNode, TreeRenderer,
};

/// Renderable view over SGR unit lists.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SgrUnitListView {
    units: Vec<ServiceUnit>,
}

impl SgrUnitListView {
    /// Builds a unit-list view.
    #[must_use]
    pub const fn new(units: Vec<ServiceUnit>) -> Self {
        Self { units }
    }

    /// Read-only access to the unit list.
    #[must_use]
    pub fn units(&self) -> &[ServiceUnit] {
        &self.units
    }

    /// Consumes the view and returns the units.
    #[must_use]
    pub fn into_units(self) -> Vec<ServiceUnit> {
        self.units
    }
}

impl From<Vec<ServiceUnit>> for SgrUnitListView {
    fn from(units: Vec<ServiceUnit>) -> Self {
        Self::new(units)
    }
}

/// Renderable view over a traversed SGR graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SgrGraphView {
    ordered_unit_ids: Vec<UnitId>,
    state: GraphState,
}

impl SgrGraphView {
    /// Builds a graph traversal view.
    #[must_use]
    pub const fn new(ordered_unit_ids: Vec<UnitId>, state: GraphState) -> Self {
        Self {
            ordered_unit_ids,
            state,
        }
    }

    /// Read-only ordered unit ids.
    #[must_use]
    pub fn ordered_unit_ids(&self) -> &[UnitId] {
        &self.ordered_unit_ids
    }

    /// Evaluated graph state.
    #[must_use]
    pub const fn state(&self) -> GraphState {
        self.state
    }
}

impl Renderable for ServiceUnit {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_service_unit_text(self, ctx)),
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => render_service_unit_tree(self, ctx),
            OutputFormat::Table => render_service_unit_table(self, ctx),
        }
    }
}

impl Renderable for UnitId {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                Ok(TextRenderer::new(ctx.clone()).render_kv("unit_id", self.as_str()))
            }
            OutputFormat::Json => render_json(self),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "UnitId".to_owned(),
                    children: vec![leaf(self.as_str())],
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["unit_id".to_owned()],
                    rows: vec![vec![self.as_str().to_owned()]],
                    align: vec![TableAlign::Left],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

impl Renderable for UnitState {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "unit_state",
            &styled_unit_state(*self, ctx),
            self,
            format,
            ctx,
        )
    }
}

impl Renderable for UnitKind {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value("unit_kind", unit_kind_label(*self), self, format, ctx)
    }
}

impl Renderable for DependencyKind {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "dependency_kind",
            dependency_kind_label(*self),
            self,
            format,
            ctx,
        )
    }
}

impl Renderable for GraphState {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "graph_state",
            &styled_graph_state(*self, ctx),
            self,
            format,
            ctx,
        )
    }
}

impl Renderable for RegisteredAdapter {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_registered_adapter_text(self, ctx)),
            OutputFormat::Json => render_value(&registered_adapter_value(self)),
            OutputFormat::Tree => render_registered_adapter_tree(self, ctx),
            OutputFormat::Table => render_registered_adapter_table(self, ctx),
        }
    }
}

impl Renderable for AdapterRegistrationState {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        render_enum_value(
            "adapter_registration_state",
            &styled_adapter_state(*self, ctx),
            self,
            format,
            ctx,
        )
    }
}

impl Renderable for SgrUnitListView {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_unit_list_text(self, ctx)),
            OutputFormat::Json => render_value(&json!({ "units": self.units })),
            OutputFormat::Tree => render_unit_list_tree(self, ctx),
            OutputFormat::Table => render_unit_list_table(self, ctx),
        }
    }
}

impl Renderable for SgrGraphView {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => Ok(render_graph_view_text(self, ctx)),
            OutputFormat::Json => render_value(&json!({
                "state": self.state,
                "ordered_unit_ids": self.ordered_unit_ids,
            })),
            OutputFormat::Tree => render_graph_view_tree(self, ctx),
            OutputFormat::Table => render_graph_view_table(self, ctx),
        }
    }
}

fn render_service_unit_text(unit: &ServiceUnit, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("unit_id", unit.unit_id.as_str()),
        renderer.render_kv("kind", unit_kind_label(unit.manifest.unit_kind)),
        renderer.render_kv("name", &unit.manifest.display_name),
        renderer.render_kv("state", &styled_unit_state(unit.state, ctx)),
        renderer.render_kv("last_transition_at", &unit.last_transition_at.to_rfc3339()),
        renderer.render_kv("evidence_head", evidence_head(&unit.evidence_chain)),
    ];

    renderer.render_section("ServiceUnit", &lines)
}

fn render_service_unit_tree(
    unit: &ServiceUnit,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("ServiceUnit {}", unit.unit_id.as_str()),
        children: vec![
            leaf(format!(
                "kind: {}",
                unit_kind_label(unit.manifest.unit_kind)
            )),
            leaf(format!("name: {}", unit.manifest.display_name)),
            leaf(format!("state: {}", styled_unit_state(unit.state, ctx))),
            leaf(format!(
                "last_transition_at: {}",
                unit.last_transition_at.to_rfc3339()
            )),
            leaf(format!(
                "evidence_head: {}",
                evidence_head(&unit.evidence_chain)
            )),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_service_unit_table(
    unit: &ServiceUnit,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "unit_id".to_owned(),
            "kind".to_owned(),
            "name".to_owned(),
            "state".to_owned(),
            "last_transition_at".to_owned(),
            "evidence_head".to_owned(),
        ],
        rows: vec![vec![
            unit.unit_id.as_str().to_owned(),
            unit_kind_label(unit.manifest.unit_kind).to_owned(),
            unit.manifest.display_name.clone(),
            styled_unit_state(unit.state, ctx),
            unit.last_transition_at.to_rfc3339(),
            evidence_head(&unit.evidence_chain).to_owned(),
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

fn render_registered_adapter_text(adapter: &RegisteredAdapter, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let lines = vec![
        renderer.render_kv("capability_id", &adapter.capability.capability_id),
        renderer.render_kv("state", &styled_adapter_state(adapter.state, ctx)),
        renderer.render_kv("registered_at", &adapter.registered_at.to_rfc3339()),
        renderer.render_kv("provides", &list_label(&adapter.capability.provides)),
        renderer.render_kv("requires", &list_label(&adapter.capability.requires)),
        renderer.render_kv("risk_template", &adapter.capability.risk_template),
        renderer.render_kv(
            "declaration",
            adapter_declaration_label(&adapter.declaration),
        ),
    ];

    renderer.render_section("RegisteredAdapter", &lines)
}

fn render_registered_adapter_tree(
    adapter: &RegisteredAdapter,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("RegisteredAdapter {}", adapter.capability.capability_id),
        children: vec![
            leaf(format!(
                "state: {}",
                styled_adapter_state(adapter.state, ctx)
            )),
            leaf(format!(
                "registered_at: {}",
                adapter.registered_at.to_rfc3339()
            )),
            TreeNode {
                label: format!("provides: count={}", adapter.capability.provides.len()),
                children: adapter
                    .capability
                    .provides
                    .iter()
                    .cloned()
                    .map(leaf)
                    .collect(),
            },
            TreeNode {
                label: format!("requires: count={}", adapter.capability.requires.len()),
                children: adapter
                    .capability
                    .requires
                    .iter()
                    .cloned()
                    .map(leaf)
                    .collect(),
            },
            leaf(format!(
                "risk_template: {}",
                adapter.capability.risk_template
            )),
            leaf(format!(
                "declaration: {}",
                adapter_declaration_label(&adapter.declaration)
            )),
        ],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_registered_adapter_table(
    adapter: &RegisteredAdapter,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "capability_id".to_owned(),
            "state".to_owned(),
            "provides".to_owned(),
            "requires".to_owned(),
            "risk_template".to_owned(),
        ],
        rows: vec![vec![
            adapter.capability.capability_id.clone(),
            styled_adapter_state(adapter.state, ctx),
            list_label(&adapter.capability.provides),
            list_label(&adapter.capability.requires),
            adapter.capability.risk_template.clone(),
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

fn render_unit_list_text(list: &SgrUnitListView, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let mut lines = vec![renderer.render_kv("units", &list.units.len().to_string())];
    lines.extend(list.units.iter().map(|unit| {
        format!(
            "{} {} {} {}",
            unit.unit_id.as_str(),
            unit_kind_label(unit.manifest.unit_kind),
            unit.manifest.display_name,
            styled_unit_state(unit.state, ctx)
        )
    }));

    renderer.render_section("SgrUnits", &lines)
}

fn render_unit_list_tree(
    list: &SgrUnitListView,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("sgr_units count={}", list.units.len()),
        children: list
            .units
            .iter()
            .map(|unit| TreeNode {
                label: unit.unit_id.as_str().to_owned(),
                children: vec![
                    leaf(format!(
                        "kind: {}",
                        unit_kind_label(unit.manifest.unit_kind)
                    )),
                    leaf(format!("name: {}", unit.manifest.display_name)),
                    leaf(format!("state: {}", styled_unit_state(unit.state, ctx))),
                ],
            })
            .collect(),
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_unit_list_table(
    list: &SgrUnitListView,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let spec = TableSpec {
        headers: vec![
            "unit_id".to_owned(),
            "kind".to_owned(),
            "name".to_owned(),
            "state".to_owned(),
        ],
        rows: list
            .units
            .iter()
            .map(|unit| {
                vec![
                    unit.unit_id.as_str().to_owned(),
                    unit_kind_label(unit.manifest.unit_kind).to_owned(),
                    unit.manifest.display_name.clone(),
                    styled_unit_state(unit.state, ctx),
                ]
            })
            .collect(),
        align: vec![
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
            TableAlign::Left,
        ],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
}

fn render_graph_view_text(view: &SgrGraphView, ctx: &RenderContext) -> String {
    let renderer = TextRenderer::new(ctx.clone());
    let mut lines = vec![
        renderer.render_kv("state", &styled_graph_state(view.state, ctx)),
        renderer.render_kv("ordered_unit_ids", &view.ordered_unit_ids.len().to_string()),
    ];
    lines.extend(view.ordered_unit_ids.iter().map(ToString::to_string));

    renderer.render_section("SgrGraph", &lines)
}

fn render_graph_view_tree(view: &SgrGraphView, ctx: &RenderContext) -> Result<String, RenderError> {
    let root = TreeNode {
        label: format!("sgr_graph state={}", graph_state_label(view.state)),
        children: vec![TreeNode {
            label: format!("ordered_unit_ids: count={}", view.ordered_unit_ids.len()),
            children: view
                .ordered_unit_ids
                .iter()
                .map(|unit_id| leaf(unit_id.as_str()))
                .collect(),
        }],
    };

    TreeRenderer::new(ctx.clone()).render(&root)
}

fn render_graph_view_table(
    view: &SgrGraphView,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let mut rows = view
        .ordered_unit_ids
        .iter()
        .enumerate()
        .map(|(index, unit_id)| {
            vec![
                index.to_string(),
                unit_id.as_str().to_owned(),
                styled_graph_state(view.state, ctx),
            ]
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        rows.push(vec![
            "-".to_owned(),
            "-".to_owned(),
            styled_graph_state(view.state, ctx),
        ]);
    }

    let spec = TableSpec {
        headers: vec!["order".to_owned(), "unit_id".to_owned(), "state".to_owned()],
        rows,
        align: vec![TableAlign::Right, TableAlign::Left, TableAlign::Left],
    };

    TableRenderer::new(ctx.clone()).render(&spec)
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

fn registered_adapter_value(adapter: &RegisteredAdapter) -> Value {
    json!({
        "capability": adapter.capability,
        "declaration": adapter.declaration,
        "registered_at": adapter.registered_at,
        "state": adapter.state,
    })
}

fn styled_unit_state(state: UnitState, ctx: &RenderContext) -> String {
    styled_label(state.as_wire_str(), unit_state_color(state), ctx)
}

fn styled_graph_state(state: GraphState, ctx: &RenderContext) -> String {
    styled_label(graph_state_label(state), graph_state_color(state), ctx)
}

fn styled_adapter_state(state: AdapterRegistrationState, ctx: &RenderContext) -> String {
    styled_label(
        adapter_registration_state_label(state),
        adapter_registration_state_color(state),
        ctx,
    )
}

fn styled_label(label: &str, color: &str, ctx: &RenderContext) -> String {
    if ctx.color {
        format!("\u{1b}[{color}m{label}\u{1b}[0m")
    } else {
        label.to_owned()
    }
}

const fn unit_state_color(state: UnitState) -> &'static str {
    match state {
        UnitState::Running | UnitState::Healthy => "32",
        UnitState::Failed => "31",
        UnitState::Starting | UnitState::Stopping | UnitState::Degraded | UnitState::Unhealthy => {
            "33"
        }
        UnitState::Draft | UnitState::Queued => "34",
        UnitState::Stopped | UnitState::Retired => "90",
    }
}

const fn graph_state_color(state: GraphState) -> &'static str {
    match state {
        GraphState::Converged => "32",
        GraphState::Failed => "31",
        GraphState::Resolving | GraphState::Converging | GraphState::Degraded => "33",
        GraphState::Empty => "90",
    }
}

const fn adapter_registration_state_color(state: AdapterRegistrationState) -> &'static str {
    match state {
        AdapterRegistrationState::Active => "32",
        AdapterRegistrationState::Suspended => "33",
        AdapterRegistrationState::Pending => "34",
        AdapterRegistrationState::Retired => "90",
    }
}

const fn unit_kind_label(kind: UnitKind) -> &'static str {
    match kind {
        UnitKind::Service => "SERVICE",
        UnitKind::OneShotJob => "ONE_SHOT_JOB",
        UnitKind::Timer => "TIMER",
        UnitKind::Mount => "MOUNT",
        UnitKind::Device => "DEVICE",
        UnitKind::AppSession => "APP_SESSION",
        UnitKind::AgentWorker => "AGENT_WORKER",
        UnitKind::ModelServer => "MODEL_SERVER",
        UnitKind::RecoveryTask => "RECOVERY_TASK",
        UnitKind::Observer => "OBSERVER",
    }
}

const fn dependency_kind_label(kind: DependencyKind) -> &'static str {
    match kind {
        DependencyKind::RequiresHealthy => "REQUIRES_HEALTHY",
        DependencyKind::RequiresRunning => "REQUIRES_RUNNING",
        DependencyKind::OrdersAfter => "ORDERS_AFTER",
    }
}

const fn graph_state_label(state: GraphState) -> &'static str {
    match state {
        GraphState::Empty => "EMPTY",
        GraphState::Resolving => "RESOLVING",
        GraphState::Converging => "CONVERGING",
        GraphState::Converged => "CONVERGED",
        GraphState::Degraded => "DEGRADED",
        GraphState::Failed => "FAILED",
    }
}

const fn adapter_registration_state_label(state: AdapterRegistrationState) -> &'static str {
    match state {
        AdapterRegistrationState::Pending => "PENDING",
        AdapterRegistrationState::Active => "ACTIVE",
        AdapterRegistrationState::Suspended => "SUSPENDED",
        AdapterRegistrationState::Retired => "RETIRED",
    }
}

const fn adapter_declaration_label(declaration: &AdapterDeclaration) -> &'static str {
    match declaration {
        AdapterDeclaration::Manifest(_) => "MANIFEST",
        AdapterDeclaration::Capability(_) => "CAPABILITY",
    }
}

fn evidence_head(evidence_chain: &[String]) -> &str {
    evidence_chain.first().map_or("-", String::as_str)
}

fn list_label(values: &[String]) -> String {
    if values.is_empty() {
        "-".to_owned()
    } else {
        values.join(", ")
    }
}

fn render_json<T: Serialize>(value: &T) -> Result<String, RenderError> {
    serde_json::to_string(value).map_err(|err| RenderError::SerializationFailed(err.to_string()))
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
