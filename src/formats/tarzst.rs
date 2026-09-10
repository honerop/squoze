use std::{fs, io::Read, path::Path, path::PathBuf};

use ::tar::Builder as TarBuilder;

use crate::entry::{ArchiveEntry, ContentFetcher};
use crate::error::Result;
use crate::formats::tar;

pub fn create(input: &Path, output: &Path) -> Result<()> {
    let file = fs::File::create(output)?;
    let encoder = zstd::Encoder::new(file, 3)?;
    let mut builder = TarBuilder::new(encoder);

    tar::add_tar_path(&mut builder, input)?;

    let encoder = builder.into_inner()?;
    encoder.finish()?;
    Ok(())
}

pub fn extract(input: &Path, output: &Path) -> Result<()> {
    let file = fs::File::open(input)?;
    let decoder = zstd::Decoder::new(file)?;
    tar::extract_with(decoder, output)
}

pub fn preview(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = fs::File::open(input)?;
    let decoder = zstd::stream::read::Decoder::new(file)?;
    tar::preview_with(decoder)
}

pub fn make_fetcher(input: PathBuf) -> ContentFetcher {
    tar::make_fetcher(input, |file| {
        Ok(Box::new(zstd::stream::read::Decoder::new(file)?) as Box<dyn Read>)
    })
}
