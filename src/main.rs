use anyhow::Result;
use clap::Parser;
use operations::Operations;

mod config;
mod cli;
mod operations;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();

    match &cli.command {
        cli::Commands::New { path } => Operations::new(path),
        cli::Commands::Preview { port  } => Operations::preview(port.unwrap_or_default()),
        cli::Commands::Publish { rebuild } => Operations::publish(*rebuild),
    }
}
