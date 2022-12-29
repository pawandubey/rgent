use clap::Parser;
use operations::Operations;

mod config;
mod cli;
mod operations;

fn main() {
    let cli = cli::Cli::parse();

    match &cli.command {
        cli::Commands::New { name } => Operations::new(name),
        cli::Commands::Preview { port  } => Operations::preview(port.unwrap_or_default()),
        cli::Commands::Publish { rebuild } => Operations::publish(*rebuild),
    }
}
