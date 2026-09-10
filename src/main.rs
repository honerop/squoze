use clap::Parser;
mod preview;
use std::path::Path;

mod backend;
use backend::*;

#[derive(Parser, Debug)]
#[command(name = "squoze", version, about, long_about = None)]
/// Create, extract and preview archives from the terminal.
struct Args {
    /// Preview without writing anything.
    #[arg(short, long)]
    preview: Option<String>,

    /// Create an archive.
    #[arg(short, long, conflicts_with = "extract")]
    create: Option<String>,

    /// Extract an archive.
    #[arg(short, long, conflicts_with = "create")]
    extract: Option<String>,

    /// Output archive or extraction directory.
    #[arg(short, long)]
    output: Option<String>,
}

/* CLI */

fn main() {
    let args = Args::parse();

    let result = match (
        &args.create,
        &args.extract,
        &args.preview,
        &args.output,
    ) {
        (Some(input), None, None, Some(output)) => {
            let output = Path::new(output);
            StandardBackend::create(Path::new(input), output)
        }
        (None, Some(input), None, Some(output)) => {
            let output = Path::new(output);
            StandardBackend::extract(Path::new(input), output)
        }
        (None, None, Some(preview_path), None) => {
            StandardBackend::preview(Path::new(preview_path))
        }
        (None, None, None, None) => Err(ArchiveError::InvalidArguments(
            "Type --help for how to use this cli".into(),
        )),
        _ => unreachable!(),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
