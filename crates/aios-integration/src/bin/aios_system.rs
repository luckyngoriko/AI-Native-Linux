#![allow(missing_docs, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use aios_integration::composition::ServiceComposition;
use aios_integration::composition_engine::default_aios_composition;
use aios_integration::orchestrator::Orchestrator;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "aios-system",
    about = "AIOS system orchestrator — typed scaffold for the 17-crate boot sequence",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the default boot sequence in topological order.
    Boot,
    /// Validate an external JSON `ServiceComposition` file.
    Validate {
        /// Path to the JSON composition file.
        composition_file: PathBuf,
    },
    /// Alias for boot — emit only the topological order of service IDs.
    Topo,
    /// List all services with crate names and binding endpoints.
    Services,
    /// Print scaffold health status for every service.
    HealthCheck,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Boot => {
            let orch = Orchestrator::from_default_composition()?;
            let order = orch.boot_order().await;
            let comp = default_aios_composition();
            for (i, id) in order.iter().enumerate() {
                let svc = comp
                    .services
                    .iter()
                    .find(|s| &s.service_id == id)
                    .expect("service not in default composition — invariant violated");
                println!(
                    "[{i}] {id} (crate={crate}, endpoint={endpoint})",
                    crate = svc.crate_name,
                    endpoint = svc.binding_endpoint
                );
            }
        }
        Command::Validate { composition_file } => {
            let json = std::fs::read_to_string(&composition_file)?;
            let comp: ServiceComposition = serde_json::from_str(&json)?;
            let engine = aios_integration::composition_engine::CompositionEngine::new();
            let order = engine.validate(&comp).await?;
            println!("validation passed — topological order:");
            for (i, id) in order.iter().enumerate() {
                println!("  [{i}] {id}");
            }
        }
        Command::Topo => {
            let orch = Orchestrator::from_default_composition()?;
            let order = orch.boot_order().await;
            for id in &order {
                println!("{id}");
            }
        }
        Command::Services => {
            let comp = default_aios_composition();
            println!("{} services in default composition:\n", comp.services.len());
            for svc in &comp.services {
                println!(
                    "  {id}  crate={crate}  endpoint={endpoint}",
                    id = svc.service_id,
                    crate = svc.crate_name,
                    endpoint = svc.binding_endpoint
                );
            }
        }
        Command::HealthCheck => {
            let orch = Orchestrator::from_default_composition()?;
            let summaries = orch.health_summary().await;
            for s in &summaries {
                println!("{id}: scaffold-ready", id = s.service_id);
            }
        }
    }

    Ok(())
}
