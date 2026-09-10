use std::{fs, io, io::Read, path::Path, path::PathBuf, sync::Arc};

use crate::entry::{ArchiveEntry, ContentFetcher};
use crate::error::{ArchiveError, Result};
use crate::util::decoded_size;

pub fn create(input: &Path, output: &Path) -> Result<()> {
    if !input.is_file() {
        return Err(ArchiveError::InvalidArguments(
            "zstd only accepts a file".into(),
        ));
    }

    let mut input_file = fs::File::open(input)?;
    let output_file = fs::File::create(output)?;

    let mut encoder = zstd::Encoder::new(output_file, 3)?;

    io::copy(&mut input_file, &mut encoder)?;
    encoder.finish()?;
    Ok(())
}

pub fn extract(input: &Path, output: &Path) -> Result<()> {
    let input_file = fs::File::open(input)?;
    let mut decoder = zstd::Decoder::new(input_file)?;
    let mut output_file = fs::File::create(output)?;

    io::copy(&mut decoder, &mut output_file)?;
    Ok(())
}

fn uncompressed_size(input: &Path) -> io::Result<u64> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = fs::File::open(input)?;

    let mut magic = [0u8; 4];
    file.read_exact(&mut magic)?;

    if magic != [0x28, 0xB5, 0x2F, 0xFD] {
        file.seek(SeekFrom::Start(0))?;
        return decoded_size(zstd::Decoder::new(file)?);
    }

    let mut descriptor = [0u8; 1];
    file.read_exact(&mut descriptor)?;

    let single_segment = descriptor[0] & 0x20 != 0;
    let fcs_flag = descriptor[0] >> 6;
    let dict_id_size = match descriptor[0] & 0x03 {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => 4,
    };

    let skip = dict_id_size + usize::from(!single_segment);
    file.seek(SeekFrom::Current(skip as i64))?;

    let fcs_len = match fcs_flag {
        0 if single_segment => Some(1),
        0 => None,
        1 => Some(2),
        2 => Some(4),
        _ => Some(8),
    };

    let Some(fcs_len) = fcs_len else {
        file.seek(SeekFrom::Start(0))?;
        return decoded_size(zstd::Decoder::new(file)?);
    };

    let mut buf = [0u8; 8];
    file.read_exact(&mut buf[..fcs_len])?;

    Ok(match fcs_len {
        1 => buf[0] as u64,
        2 => u16::from_le_bytes([buf[0], buf[1]]) as u64 + 256,
        4 => u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as u64,
        _ => u64::from_le_bytes(buf),
    })
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
        let mut decoder = zstd::Decoder::new(fs::File::open(&input)?)?;

        let mut data = Vec::new();
        decoder.read_to_end(&mut data)?;

        Ok(data)
    })
}
