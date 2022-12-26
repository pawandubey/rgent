use clap::Parser;

mod config;
mod cli;

fn main() {
    let _cli = cli::Cli::parse();
}
