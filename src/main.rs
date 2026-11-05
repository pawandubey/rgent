use anyhow::{Context, Result};
use clap::Parser;
use operations::Operations;

mod cli;
mod config;
mod operations;
mod preview;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    match &cli.command {
        cli::Commands::New { path } => Operations::new(path),
        cli::Commands::Preview { port } => Operations::preview(&current_dir()?, *port),
        cli::Commands::Publish { rebuild } => Operations::publish(&current_dir()?, *rebuild),
    }
}

fn current_dir() -> Result<std::path::PathBuf> {
    std::env::current_dir().context("Failed to get current directory")
}
