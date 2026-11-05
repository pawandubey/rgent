use anyhow::{Context, Result};
use clap::Parser;
use operations::Operations;

mod cli;
mod config;
mod dates;
mod highlight;
mod operations;
mod pagination;
mod preview;
mod theme;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    match &cli.command {
        cli::Commands::New { path } => Operations::new(path),
        cli::Commands::Preview { port } => Operations::preview(&current_dir()?, *port),
        cli::Commands::Publish { rebuild } => Operations::publish(&current_dir()?, *rebuild),
        cli::Commands::HighlightCss { theme_dir } => Operations::highlight_css(theme_dir),
    }
}

fn current_dir() -> Result<std::path::PathBuf> {
    std::env::current_dir().context("Failed to get current directory")
}
