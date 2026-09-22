use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod backend;
mod entry;
mod error;
mod formats;
mod preview;
mod util;

use backend::*;
use error::ArchiveError;

#[derive(Parser, Debug)]
#[command(name = "squoze", version, about)]
/// Create, extract and preview archives from the terminal.
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Create an archive.
    Create {
        /// Input file or directory.
        input: PathBuf,

        /// Output archive.
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Extract an archive.
    Extract {
        /// Input archive.
        input: PathBuf,

        /// Output directory.
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Preview an archive interactively.
    Preview {
        /// Archive to preview.
        archive: PathBuf,
    },

    /// Preview an archive as JSON.
    #[command(name = "preview-json")]
    PreviewJson {
        /// Archive to preview.
        archive: PathBuf,
    },
}

fn main() {
    let args = Args::parse();

    let result = match args.command {
        Command::Create { input, output } => StandardBackend::create(&input, &output),

        Command::Extract { input, output } => StandardBackend::extract(&input, &output),

        Command::Preview { archive } => StandardBackend::preview(&archive),

        Command::PreviewJson { archive } => StandardBackend::preview_json(&archive),
    };

    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
