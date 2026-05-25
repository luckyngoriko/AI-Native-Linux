//! `aios` binary entrypoint.

use std::process::ExitCode;

use aios_renderer_cli::{AiosCli, AiosClient, OutputFormat, RenderContext};
use anyhow::Result;
use clap::Parser;
use serde_json::json;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = AiosCli::parse();
    let error_format = cli.output_format().unwrap_or(OutputFormat::Text);
    let error_context = cli.render_context();

    match run(cli).await {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", render_error(&error, error_format, &error_context));
            ExitCode::from(1)
        }
    }
}

async fn run(cli: AiosCli) -> Result<String> {
    let endpoints = cli.endpoints_config()?;
    let mut client = AiosClient::connect(&endpoints).await?;
    // AiosCli::execute owns subcommand dispatch, including `aios verify`.
    Ok(cli.execute(&mut client).await?)
}

fn render_error(error: &anyhow::Error, format: OutputFormat, ctx: &RenderContext) -> String {
    match format {
        OutputFormat::Json => serde_json::to_string(&json!({
            "error": {
                "message": error.to_string(),
                "chain": error.chain().map(ToString::to_string).collect::<Vec<_>>(),
            }
        }))
        .unwrap_or_else(|json_error| {
            format!("{{\"error\":{{\"message\":\"error serialization failed: {json_error}\"}}}}")
        }),
        OutputFormat::Text | OutputFormat::Tree | OutputFormat::Table => {
            let label = if ctx.color {
                "\u{1b}[31merror\u{1b}[0m"
            } else {
                "error"
            };
            format!("{label}: {error}")
        }
    }
}
