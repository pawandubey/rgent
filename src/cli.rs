use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(author, version, about, long_about = None, propagate_version = true)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Create a new site at the directory given by the name. Defaults to current directory.
    New {
        /// Path to the directory to generate the site at. Will be created if not present.
        #[clap(value_parser)]
        path: PathBuf,
    },
    /// Generate HTML from the markdown raw content into the output directory.
    Publish {
        /// Force a rebuild of the site from scratch.
        #[arg(short, long)]
        rebuild: bool,
    },
    /// Preview the generated site locally in your browser.
    Preview {
        /// Port on which to launch the site. It will be available on http://localhost:<port>.
        #[arg(short, long)]
        port: Option<u16>,
    },
}
