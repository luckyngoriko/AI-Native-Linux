//! Clap command tree and in-process execution for the `aios` binary.

#![allow(
    clippy::module_name_repetitions,
    reason = "public names mirror the AIOS CLI command vocabulary"
)]

use std::fs;
use std::path::{Path, PathBuf};

use aios_action::{ActionEnvelope, ActionId};
use aios_fs::View;
use aios_vault::{CapabilityClass, VaultCapability};
use aios_verification::VerificationIntent;
use clap::{Parser, Subcommand};
use serde_json::{json, Value};

use crate::{
    AiosClient, AiosEndpoints, KernelCandidate, OutputFormat, RenderContext, RenderError,
    Renderable, TableAlign, TableRenderer, TableSpec, TextRenderer, TreeNode, TreeRenderer,
    VerificationPrimitiveList, Version,
};

/// Parsed command-line interface for the `aios` binary.
#[derive(Debug, Clone, Parser)]
#[command(
    name = "aios",
    version,
    about = "AIOS — AI-Native Linux command interface"
)]
pub struct AiosCli {
    /// Output format.
    #[arg(short = 'o', long, default_value = "text")]
    pub format: String,
    /// Disable color output.
    #[arg(long)]
    pub no_color: bool,
    /// Show raw secrets in evidence/vault output (debug only — DO NOT use in production).
    #[arg(long)]
    pub no_redact: bool,
    /// Verbose output.
    #[arg(short = 'v', long)]
    pub verbose: bool,
    /// Override endpoint configuration (host:port set).
    #[arg(long, env = "AIOS_ENDPOINTS")]
    pub endpoints: Option<String>,
    /// Command to execute.
    #[command(subcommand)]
    pub command: AiosCommand,
}

/// Top-level `aios` subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum AiosCommand {
    /// Submit actions and inspect action lifecycle state.
    Action {
        /// Action operation.
        #[command(subcommand)]
        subcommand: ActionSubcommand,
    },
    /// Read and list AIOS-FS objects.
    Fs {
        /// Filesystem operation.
        #[command(subcommand)]
        subcommand: FsSubcommand,
    },
    /// Evaluate policy decisions.
    Policy {
        /// Policy operation.
        #[command(subcommand)]
        subcommand: PolicySubcommand,
    },
    /// Inspect and issue vault capabilities.
    Vault {
        /// Vault operation.
        #[command(subcommand)]
        subcommand: VaultSubcommand,
    },
    /// Inspect evidence receipts and chains.
    Evidence {
        /// Evidence operation.
        #[command(subcommand)]
        subcommand: EvidenceSubcommand,
    },
    /// Run verification and inspect supported primitives.
    Verify {
        /// Verification operation.
        #[command(subcommand)]
        subcommand: VerificationSubcommand,
    },
    /// Inspect and control recovery mode and first-boot provisioning.
    Recovery {
        /// Recovery operation.
        #[command(subcommand)]
        subcommand: RecoverySubcommand,
    },
    /// Inspect and control dedicated AIOS kernel candidates.
    Kernel {
        /// Kernel operation.
        #[command(subcommand)]
        subcommand: KernelSubcommand,
    },
}

/// Action subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum ActionSubcommand {
    /// Submit an action envelope JSON file.
    Submit {
        /// Path to the action envelope JSON file.
        envelope_json_file: PathBuf,
    },
    /// Read action lifecycle status.
    Status {
        /// Action id to inspect.
        action_id: String,
    },
}

/// AIOS-FS subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum FsSubcommand {
    /// Read an object by id.
    Read {
        /// Object id to read.
        object_id: String,
    },
    /// List object references, optionally filtered by namespace class.
    List {
        /// Optional namespace class token.
        namespace: Option<String>,
    },
    /// List all versions for an object.
    ListVersions {
        /// Object id to inspect.
        object_id: String,
    },
}

/// Policy subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum PolicySubcommand {
    /// Evaluate an action envelope JSON file.
    Evaluate {
        /// Path to the action envelope JSON file.
        envelope_json_file: PathBuf,
    },
}

/// Vault subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum VaultSubcommand {
    /// List capabilities issued to a subject.
    ListCapabilities {
        /// Subject canonical id.
        subject: String,
    },
    /// Issue a new capability for a subject.
    Issue {
        /// Capability class.
        class: String,
        /// Subject canonical id.
        subject: String,
    },
}

/// Evidence subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum EvidenceSubcommand {
    /// Query receipts bound to an action id.
    Chain {
        /// Action id to query.
        action_id: String,
        /// Optional maximum number of receipts.
        last_n: Option<u32>,
    },
    /// Read one receipt by id.
    Get {
        /// Receipt id to read.
        receipt_id: String,
    },
}

/// Verification subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum VerificationSubcommand {
    /// Run a verification intent JSON file.
    Run {
        /// Path to the verification intent JSON file.
        intent_file: PathBuf,
        /// Override the intent action id before submission.
        #[arg(long)]
        action_id: Option<String>,
    },
    /// List the supported verification primitive vocabulary.
    ListPrimitives,
}

/// Recovery subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum RecoverySubcommand {
    /// Read the current recovery state.
    Status,
    /// Enter recovery mode.
    Enter {
        /// Operator or fallback reason.
        #[arg(long)]
        reason: String,
    },
    /// Exit recovery mode.
    Exit {
        /// Recovery exit token.
        #[arg(long)]
        token: String,
    },
    /// Run first-boot provisioning.
    FirstBoot,
}

/// Kernel subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum KernelSubcommand {
    /// List registered kernel candidates.
    List,
    /// Activate a kernel candidate.
    Activate {
        /// Candidate id to activate.
        candidate_id: String,
    },
    /// Roll back a kernel candidate.
    Rollback {
        /// Candidate id to roll back.
        candidate_id: String,
    },
}

impl AiosCli {
    /// Parse the selected output format.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::UnknownFormat`] when `--format` is not one of
    /// `text`, `json`, `tree`, or `table`.
    pub fn output_format(&self) -> Result<OutputFormat, RenderError> {
        OutputFormat::from_str(&self.format)
    }

    /// Build the render context from CLI flags.
    #[must_use]
    pub fn render_context(&self) -> RenderContext {
        let mut ctx = RenderContext::new_terminal_defaults();
        ctx.color = !self.no_color;
        ctx.redact_secrets = !self.no_redact;
        ctx.verbose = self.verbose;
        ctx
    }

    /// Resolve backend endpoints from `--endpoints` or localhost defaults.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Internal`] when the endpoint override is
    /// malformed.
    pub fn endpoints_config(&self) -> Result<AiosEndpoints, RenderError> {
        self.endpoints
            .as_deref()
            .map(parse_endpoints)
            .transpose()
            .map(|endpoints| endpoints.unwrap_or_else(AiosEndpoints::localhost_default))
    }

    /// Execute the parsed command against an already-connected client.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError`] when input loading, client calls, or rendering
    /// fail.
    pub async fn execute(&self, client: &mut AiosClient) -> Result<String, RenderError> {
        let format = self.output_format()?;
        let ctx = self.render_context();

        match &self.command {
            AiosCommand::Action { subcommand } => match subcommand {
                ActionSubcommand::Submit { envelope_json_file } => {
                    let envelope = read_envelope(envelope_json_file)?;
                    render_value(&client.submit_action(envelope).await?, format, &ctx)
                }
                ActionSubcommand::Status { action_id } => {
                    render_value(&client.action_status(action_id).await?, format, &ctx)
                }
            },
            AiosCommand::Fs { subcommand } => match subcommand {
                FsSubcommand::Read { object_id } => {
                    render_value(&client.read_object(object_id).await?, format, &ctx)
                }
                FsSubcommand::List { namespace } => {
                    let view = client.list_objects(namespace.as_deref()).await?;
                    render_value(&FsListView(view), format, &ctx)
                }
                FsSubcommand::ListVersions { object_id } => {
                    let versions = client.list_versions(object_id).await?;
                    render_value(&VersionListView(versions), format, &ctx)
                }
            },
            AiosCommand::Policy { subcommand } => match subcommand {
                PolicySubcommand::Evaluate { envelope_json_file } => {
                    let envelope = read_envelope(envelope_json_file)?;
                    render_value(&client.evaluate_policy(envelope).await?, format, &ctx)
                }
            },
            AiosCommand::Vault { subcommand } => match subcommand {
                VaultSubcommand::ListCapabilities { subject } => {
                    let capabilities = client.list_capabilities(subject).await?;
                    render_value(&VaultCapabilitiesView(capabilities), format, &ctx)
                }
                VaultSubcommand::Issue { class, subject } => {
                    let class = parse_capability_class(class)?;
                    render_value(
                        &client.issue_capability(class, subject).await?,
                        format,
                        &ctx,
                    )
                }
            },
            AiosCommand::Evidence { subcommand } => match subcommand {
                EvidenceSubcommand::Chain { action_id, last_n } => render_value(
                    &client.evidence_chain(action_id, *last_n).await?,
                    format,
                    &ctx,
                ),
                EvidenceSubcommand::Get { receipt_id } => {
                    render_value(&client.evidence_receipt(receipt_id).await?, format, &ctx)
                }
            },
            AiosCommand::Verify { subcommand } => match subcommand {
                VerificationSubcommand::Run {
                    intent_file,
                    action_id,
                } => {
                    let mut intent = read_verification_intent(intent_file)?;
                    if let Some(action_id) = action_id {
                        intent.action_id = parse_action_id(action_id)?;
                    }
                    render_value(&client.verify(intent).await?, format, &ctx)
                }
                VerificationSubcommand::ListPrimitives => {
                    let primitives = client.list_primitives().await?;
                    render_value(&VerificationPrimitiveList::new(primitives), format, &ctx)
                }
            },
            AiosCommand::Recovery { subcommand } => match subcommand {
                RecoverySubcommand::Status => {
                    render_value(&client.get_recovery_state().await?, format, &ctx)
                }
                RecoverySubcommand::Enter { reason } => {
                    render_value(&client.enter_recovery(reason).await?, format, &ctx)
                }
                RecoverySubcommand::Exit { token } => {
                    render_value(&client.exit_recovery(token).await?, format, &ctx)
                }
                RecoverySubcommand::FirstBoot => {
                    render_value(&client.run_first_boot().await?, format, &ctx)
                }
            },
            AiosCommand::Kernel { subcommand } => match subcommand {
                KernelSubcommand::List => {
                    let candidates = client.list_kernel_candidates().await?;
                    render_value(&KernelCandidateListView(candidates), format, &ctx)
                }
                KernelSubcommand::Activate { candidate_id } => {
                    render_value(&client.activate_kernel(candidate_id).await?, format, &ctx)
                }
                KernelSubcommand::Rollback { candidate_id } => {
                    render_value(&client.rollback_kernel(candidate_id).await?, format, &ctx)
                }
            },
        }
    }
}

fn render_value(
    value: &impl Renderable,
    format: OutputFormat,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    value.render(format, ctx)
}

fn read_envelope(path: &Path) -> Result<ActionEnvelope, RenderError> {
    let bytes = fs::read(path).map_err(|err| {
        RenderError::Internal(format!(
            "read envelope JSON `{}` failed: {err}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|err| {
        RenderError::SerializationFailed(format!(
            "parse envelope JSON `{}` failed: {err}",
            path.display()
        ))
    })
}

fn read_verification_intent(path: &Path) -> Result<VerificationIntent, RenderError> {
    let bytes = fs::read(path).map_err(|err| {
        RenderError::Internal(format!(
            "read verification intent JSON `{}` failed: {err}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|err| {
        RenderError::SerializationFailed(format!(
            "parse verification intent JSON `{}` failed: {err}",
            path.display()
        ))
    })
}

fn parse_action_id(input: &str) -> Result<ActionId, RenderError> {
    ActionId::parse(input)
        .map_err(|err| RenderError::Internal(format!("invalid action id `{input}`: {err}")))
}

fn parse_capability_class(input: &str) -> Result<CapabilityClass, RenderError> {
    let normalized = input
        .chars()
        .filter(|ch| *ch != '-' && *ch != '_')
        .flat_map(char::to_lowercase)
        .collect::<String>();

    match normalized.as_str() {
        "keysign" => Ok(CapabilityClass::KeySign),
        "keyverify" => Ok(CapabilityClass::KeyVerify),
        "keyencrypt" => Ok(CapabilityClass::KeyEncrypt),
        "keydecrypt" => Ok(CapabilityClass::KeyDecrypt),
        "macgenerate" => Ok(CapabilityClass::MacGenerate),
        "macverify" => Ok(CapabilityClass::MacVerify),
        "randomgenerate" => Ok(CapabilityClass::RandomGenerate),
        "secretget" => Ok(CapabilityClass::SecretGet),
        "bootstrapkeysign" => Ok(CapabilityClass::BootstrapKeySign),
        _ => Err(RenderError::Internal(format!(
            "unknown vault capability class `{input}`"
        ))),
    }
}

fn parse_endpoints(input: &str) -> Result<AiosEndpoints, RenderError> {
    let parts = input
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();

    if parts.is_empty() {
        return Err(RenderError::Internal(
            "AIOS_ENDPOINTS must not be empty".to_owned(),
        ));
    }

    if parts.iter().any(|part| part.contains('=')) {
        parse_keyed_endpoints(&parts)
    } else {
        parse_positional_endpoints(&parts)
    }
}

fn parse_keyed_endpoints(parts: &[&str]) -> Result<AiosEndpoints, RenderError> {
    let mut endpoints = AiosEndpoints::localhost_default();

    for part in parts {
        let (key, value) = part.split_once('=').ok_or_else(|| {
            RenderError::Internal(format!(
                "invalid endpoint entry `{part}`; expected key=value"
            ))
        })?;
        let endpoint = normalize_endpoint(value)?;
        match key.trim() {
            "policy" => endpoints.policy = endpoint,
            "runtime" => endpoints.runtime = endpoint,
            "fs" => endpoints.fs = endpoint,
            "vault" => endpoints.vault = endpoint,
            "verification" => endpoints.verification = endpoint,
            "recovery" => endpoints.recovery = endpoint,
            "evidence" => {
                endpoints.evidence = if value.trim().eq_ignore_ascii_case("none") {
                    None
                } else {
                    Some(endpoint)
                };
            }
            other => {
                return Err(RenderError::Internal(format!(
                    "unknown endpoint key `{other}`"
                )));
            }
        }
    }

    Ok(endpoints)
}

fn parse_positional_endpoints(parts: &[&str]) -> Result<AiosEndpoints, RenderError> {
    if !(6..=7).contains(&parts.len()) {
        return Err(RenderError::Internal(
            "AIOS_ENDPOINTS positional form is policy,runtime,fs,vault,verification,recovery[,evidence]"
                .to_owned(),
        ));
    }

    Ok(AiosEndpoints {
        policy: normalize_endpoint(parts[0])?,
        runtime: normalize_endpoint(parts[1])?,
        fs: normalize_endpoint(parts[2])?,
        vault: normalize_endpoint(parts[3])?,
        verification: normalize_endpoint(parts[4])?,
        recovery: normalize_endpoint(parts[5])?,
        evidence: parts
            .get(6)
            .map(|value| {
                if value.trim().eq_ignore_ascii_case("none") {
                    Ok(None)
                } else {
                    normalize_endpoint(value).map(Some)
                }
            })
            .transpose()?
            .flatten(),
    })
}

fn normalize_endpoint(value: &str) -> Result<String, RenderError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(RenderError::Internal(
            "endpoint value must not be empty".to_owned(),
        ));
    }
    if trimmed.eq_ignore_ascii_case("none") {
        return Ok(trimmed.to_owned());
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Ok(trimmed.to_owned())
    } else {
        Ok(format!("http://{trimmed}"))
    }
}

struct FsListView(View);

impl Renderable for FsListView {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                let mut lines = vec![
                    renderer.render_kv("snapshot_id", &self.0.snapshot_id.0),
                    renderer.render_kv("matched", &self.0.matched.len().to_string()),
                ];
                lines.extend(
                    self.0
                        .matched
                        .iter()
                        .map(|reference| reference.object_id.as_str().to_owned()),
                );
                Ok(renderer.render_section("FsList", &lines))
            }
            OutputFormat::Json => serde_json::to_string(&self.0)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "fs_list".to_owned(),
                    children: self
                        .0
                        .matched
                        .iter()
                        .map(|reference| TreeNode {
                            label: reference.object_id.as_str().to_owned(),
                            children: Vec::new(),
                        })
                        .collect(),
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["object_id".to_owned()],
                    rows: self
                        .0
                        .matched
                        .iter()
                        .map(|reference| vec![reference.object_id.as_str().to_owned()])
                        .collect(),
                    align: vec![TableAlign::Left],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

struct VersionListView(Vec<Version>);

impl Renderable for VersionListView {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                let mut lines = vec![renderer.render_kv("versions", &self.0.len().to_string())];
                lines.extend(
                    self.0
                        .iter()
                        .map(|version| version.version_id.as_str().to_owned()),
                );
                Ok(renderer.render_section("FsVersions", &lines))
            }
            OutputFormat::Json => serde_json::to_string(&self.0)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "fs_versions".to_owned(),
                    children: self
                        .0
                        .iter()
                        .map(|version| TreeNode {
                            label: version.version_id.as_str().to_owned(),
                            children: Vec::new(),
                        })
                        .collect(),
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec!["version_id".to_owned(), "state".to_owned()],
                    rows: self
                        .0
                        .iter()
                        .map(|version| {
                            vec![
                                version.version_id.as_str().to_owned(),
                                format!("{:?}", version.state),
                            ]
                        })
                        .collect(),
                    align: vec![TableAlign::Left, TableAlign::Left],
                };
                TableRenderer::new(ctx.clone()).render(&spec)
            }
        }
    }
}

struct VaultCapabilitiesView(Vec<VaultCapability>);

impl Renderable for VaultCapabilitiesView {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                let mut lines = vec![renderer.render_kv("capabilities", &self.0.len().to_string())];
                lines.extend(self.0.iter().map(|capability| {
                    format!(
                        "{} {:?} {} <vault-handle>",
                        capability.capability_id.as_str(),
                        capability.class,
                        capability.issued_to
                    )
                }));
                Ok(renderer.render_section("VaultCapabilities", &lines))
            }
            OutputFormat::Json => {
                let capabilities = self
                    .0
                    .iter()
                    .map(|capability| {
                        capability
                            .render(OutputFormat::Json, ctx)
                            .and_then(|rendered| {
                                serde_json::from_str::<Value>(&rendered).map_err(|err| {
                                    RenderError::SerializationFailed(err.to_string())
                                })
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                serde_json::to_string(&json!({ "capabilities": capabilities }))
                    .map_err(|err| RenderError::SerializationFailed(err.to_string()))
            }
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: "vault_capabilities".to_owned(),
                    children: self
                        .0
                        .iter()
                        .map(|capability| TreeNode {
                            label: capability.capability_id.as_str().to_owned(),
                            children: vec![
                                TreeNode {
                                    label: format!("class: {:?}", capability.class),
                                    children: Vec::new(),
                                },
                                TreeNode {
                                    label: format!("issued_to: {}", capability.issued_to),
                                    children: Vec::new(),
                                },
                            ],
                        })
                        .collect(),
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec![
                        "capability_id".to_owned(),
                        "class".to_owned(),
                        "issued_to".to_owned(),
                        "key_material_handle".to_owned(),
                    ],
                    rows: self
                        .0
                        .iter()
                        .map(|capability| {
                            vec![
                                capability.capability_id.as_str().to_owned(),
                                format!("{:?}", capability.class),
                                capability.issued_to.to_string(),
                                "<vault-handle>".to_owned(),
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
        }
    }
}

struct KernelCandidateListView(Vec<KernelCandidate>);

impl Renderable for KernelCandidateListView {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => {
                let renderer = TextRenderer::new(ctx.clone());
                let mut lines = vec![renderer.render_kv("candidates", &self.0.len().to_string())];
                lines.extend(self.0.iter().map(|candidate| {
                    format!(
                        "{} {} {} {}",
                        candidate.candidate_id.as_str(),
                        candidate.version,
                        truncate_kernel_hash(&candidate.kernel_blake3),
                        candidate.state.as_wire_str()
                    )
                }));
                Ok(renderer.render_section("KernelCandidates", &lines))
            }
            OutputFormat::Json => {
                let candidates = self
                    .0
                    .iter()
                    .map(|candidate| {
                        candidate
                            .render(OutputFormat::Json, ctx)
                            .and_then(|rendered| {
                                serde_json::from_str::<Value>(&rendered).map_err(|err| {
                                    RenderError::SerializationFailed(err.to_string())
                                })
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                serde_json::to_string(&json!({ "candidates": candidates }))
                    .map_err(|err| RenderError::SerializationFailed(err.to_string()))
            }
            OutputFormat::Tree => {
                let root = TreeNode {
                    label: format!("kernel_candidates count={}", self.0.len()),
                    children: self
                        .0
                        .iter()
                        .map(|candidate| TreeNode {
                            label: candidate.candidate_id.as_str().to_owned(),
                            children: vec![
                                TreeNode {
                                    label: format!("version: {}", candidate.version),
                                    children: Vec::new(),
                                },
                                TreeNode {
                                    label: format!("state: {}", candidate.state.as_wire_str()),
                                    children: Vec::new(),
                                },
                            ],
                        })
                        .collect(),
                };
                TreeRenderer::new(ctx.clone()).render(&root)
            }
            OutputFormat::Table => {
                let spec = TableSpec {
                    headers: vec![
                        "candidate_id".to_owned(),
                        "version".to_owned(),
                        "kernel_blake3".to_owned(),
                        "state".to_owned(),
                    ],
                    rows: self
                        .0
                        .iter()
                        .map(|candidate| {
                            vec![
                                candidate.candidate_id.as_str().to_owned(),
                                candidate.version.clone(),
                                truncate_kernel_hash(&candidate.kernel_blake3),
                                candidate.state.as_wire_str().to_owned(),
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
        }
    }
}

fn truncate_kernel_hash(hash: &str) -> String {
    hash.chars().take(12).collect()
}
