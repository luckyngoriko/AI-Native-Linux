//! `aios apps` CLI subcommand — package store, compatibility profiles, sessions,
//! and update lifecycle (T-125).
//!
//! Maps to the `AppsService` gRPC surface (T-122) via [`crate::AiosClient`].

use std::path::PathBuf;

use clap::Subcommand;

use crate::{AiosClient, OutputFormat, RenderContext, RenderError, Renderable};

// ---------------------------------------------------------------------------
// AppsArgs — top-level `aios apps` subcommand tree
// ---------------------------------------------------------------------------

/// `aios apps` — inspect and manage packages, compatibility profiles,
/// sessions, and update lifecycle.
#[derive(Debug, Clone, Subcommand)]
pub enum AppsArgs {
    /// List all registered packages.
    #[command(name = "list-packages")]
    ListPackages,
    /// Show a single package by id.
    Show {
        /// Package id (e.g. `pkg_<ulid26>`).
        package_id: String,
    },
    /// Register a package from a JSON manifest file.
    Register {
        /// Path to the package manifest JSON file.
        manifest_file: PathBuf,
    },
    /// Look up a compatibility profile for an app+runtime pair.
    LookupProfile {
        /// Package id to query.
        package_id: String,
        /// Ecosystem runtime (e.g. `Waydroid`, `WINE`, `Native`).
        runtime: String,
    },
    /// Inspect and manage session containers.
    #[command(subcommand)]
    Sessions(SessionsSubcommand),
    /// Plan, execute, verify, activate, and rollback package updates.
    #[command(subcommand)]
    Update(UpdateSubcommand),
}

// ---------------------------------------------------------------------------
// Sessions subcommands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Subcommand)]
pub enum SessionsSubcommand {
    /// List all sessions.
    List,
    /// Get one session by id.
    Get {
        /// Session id (e.g. `sess_<ulid26>`).
        session_id: String,
    },
}

// ---------------------------------------------------------------------------
// Update subcommands
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Subcommand)]
pub enum UpdateSubcommand {
    /// Plan a package version upgrade.
    Plan {
        /// Package id to update.
        package_id: String,
        /// Current version (source).
        from_version: String,
        /// Target version (destination).
        to_version: String,
    },
    /// Execute a planned update.
    Execute {
        /// Update plan id (e.g. `updp_<ulid26>`).
        plan_id: String,
    },
    /// Verify an executed update.
    Verify {
        /// Update plan id.
        plan_id: String,
    },
    /// Activate a verified update.
    Activate {
        /// Update plan id.
        plan_id: String,
    },
    /// Roll back an update.
    Rollback {
        /// Update plan id.
        plan_id: String,
    },
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

/// Execute an `aios apps` subcommand against the configured backend client.
///
/// # Errors
///
/// Returns [`RenderError`] when a client call fails or rendering produces
/// invalid output.
pub async fn run(
    args: &AppsArgs,
    client: &mut AiosClient,
    format: OutputFormat,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    match args {
        AppsArgs::ListPackages => {
            let packages = client.list_app_packages().await?;
            render_value(&packages, format, ctx)
        }
        AppsArgs::Show { package_id } => {
            let pkg = client.get_app_package(package_id).await?;
            render_value(&pkg, format, ctx)
        }
        AppsArgs::Register { manifest_file } => {
            let pkg_id = client.register_app_package(manifest_file).await?;
            Ok(format!("registered package: {pkg_id}"))
        }
        AppsArgs::LookupProfile {
            package_id,
            runtime: _runtime,
        } => {
            let profile = client.lookup_app_profile(package_id).await?;
            render_value(&profile, format, ctx)
        }
        AppsArgs::Sessions(sub) => run_sessions(sub, client, format, ctx).await,
        AppsArgs::Update(sub) => run_update(sub, client, format, ctx).await,
    }
}

async fn run_sessions(
    sub: &SessionsSubcommand,
    client: &mut AiosClient,
    format: OutputFormat,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    match sub {
        SessionsSubcommand::List => {
            let sessions = client.list_app_sessions().await?;
            render_value(&sessions, format, ctx)
        }
        SessionsSubcommand::Get { session_id } => {
            let session = client.get_app_session(session_id).await?;
            render_value(&session, format, ctx)
        }
    }
}

async fn run_update(
    sub: &UpdateSubcommand,
    client: &mut AiosClient,
    format: OutputFormat,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    match sub {
        UpdateSubcommand::Plan {
            package_id,
            from_version,
            to_version,
        } => {
            let plan = client
                .plan_app_update(package_id, from_version, to_version)
                .await?;
            render_value(&plan, format, ctx)
        }
        UpdateSubcommand::Execute { plan_id } => {
            let outcome = client.execute_app_update(plan_id).await?;
            Ok(serde_json::to_string(&outcome)
                .map_err(|err| RenderError::SerializationFailed(err.to_string()))?)
        }
        UpdateSubcommand::Verify { plan_id } => {
            let verification = client.verify_app_update(plan_id).await?;
            Ok(serde_json::to_string(&verification)
                .map_err(|err| RenderError::SerializationFailed(err.to_string()))?)
        }
        UpdateSubcommand::Activate { plan_id } => {
            client.activate_app_update(plan_id).await?;
            Ok(format!("update `{plan_id}` activated"))
        }
        UpdateSubcommand::Rollback { plan_id } => {
            let receipt = client.rollback_app_update(plan_id).await?;
            render_value(&receipt, format, ctx)
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
