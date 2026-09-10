use std::{fs, io, io::Read, path::Path, path::PathBuf, sync::Arc};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};

use crate::entry::{ArchiveEntry, ContentFetcher};
use crate::error::{ArchiveError, Result};
use crate::util::decoded_size;

pub fn create(input: &Path, output: &Path) -> Result<()> {
    if !input.is_file() {
        return Err(ArchiveError::InvalidArguments(
            "gzip only accepts a file".into(),
        ));
    }

    let mut input_file = fs::File::open(input)?;
    let output_file = fs::File::create(output)?;

    let mut encoder = GzEncoder::new(output_file, Compression::default());

    io::copy(&mut input_file, &mut encoder)?;
    encoder.finish()?;
    Ok(())
}

pub fn extract(input: &Path, output: &Path) -> Result<()> {
    let input_file = fs::File::open(input)?;
    let mut decoder = GzDecoder::new(input_file);
    let mut output_file = fs::File::create(output)?;

    io::copy(&mut decoder, &mut output_file)?;
    Ok(())
}

fn uncompressed_size(input: &Path) -> io::Result<u64> {
    let mut file = fs::File::open(input)?;

    let mut header = [0u8; 2];
    file.read_exact(&mut header)?;

    if header == [0x1f, 0x8b] && file.metadata()?.len() >= 8 {
        use std::io::{Seek, SeekFrom};
        file.seek(SeekFrom::End(-4))?;

        let mut trailer = [0u8; 4];
        file.read_exact(&mut trailer)?;

        return Ok(u32::from_le_bytes(trailer) as u64);
    }

    decoded_size(GzDecoder::new(fs::File::open(input)?))
}

pub fn preview(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let name = input
        .file_stem()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("output"));

    Ok(vec![ArchiveEntry {
        path: name,
        is_dir: false,
        size: uncompressed_size(input)?,
    }])
}

pub fn make_fetcher(input: PathBuf) -> ContentFetcher {
    Arc::new(move |_path| {
        let mut decoder = GzDecoder::new(fs::File::open(&input)?);

        let mut data = Vec::new();
        decoder.read_to_end(&mut data)?;

        Ok(data)
    })
}
