use anyhow::{Context, Result};
use clap::Parser;
use operations::Operations;

mod cli;
mod config;
mod operations;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    match &cli.command {
        cli::Commands::New { path } => Operations::new(path),
        cli::Commands::Preview { port } => Operations::preview(port.unwrap_or_default()),
        cli::Commands::Publish { rebuild } => {
            let root = std::env::current_dir().context("Failed to get current directory")?;
            Operations::publish(&root, *rebuild)
        }
    }
}
