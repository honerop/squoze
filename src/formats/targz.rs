use std::{fs, io::Read, path::Path, path::PathBuf};

use ::tar::Builder as TarBuilder;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};

use crate::entry::ArchiveEntry;
use crate::error::Result;
use crate::formats::tar;

pub fn create(input: &Path, output: &Path) -> Result<()> {
    let file = fs::File::create(output)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut builder = TarBuilder::new(encoder);

    tar::add_tar_path(&mut builder, input)?;

    let encoder = builder.into_inner()?;
    encoder.finish()?;
    Ok(())
}

pub fn extract(input: &Path, output: &Path) -> Result<()> {
    let file = fs::File::open(input)?;
    tar::extract_with(GzDecoder::new(file), output)
}

pub fn preview(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = fs::File::open(input)?;
    tar::preview_with(GzDecoder::new(file))
}

pub fn make_fetcher(input: PathBuf) -> crate::entry::ContentFetcher {
    tar::make_fetcher(input, |file| {
        Ok(Box::new(GzDecoder::new(file)) as Box<dyn Read>)
    })
}
