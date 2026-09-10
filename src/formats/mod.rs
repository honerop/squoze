pub mod gzip;
pub mod sevenz;
pub mod tar;
pub mod targz;
pub mod tarzst;
pub mod zip;
pub mod zstd;

use std::path::Path;

use crate::error::{ArchiveError, Result};

#[derive(Clone, Copy, Debug)]
pub enum ArchiveFormat {
    Zip,
    Tar,
    Gzip,
    TarGzip,
    Zstd,
    TarZstd,
    SevenZip,
}

pub fn archive_format(path: &Path) -> Result<ArchiveFormat> {
    let name = path
        .file_name()
        .and_then(|x| x.to_str())
        .ok_or_else(|| ArchiveError::InvalidPath(path.display().to_string()))?
        .to_ascii_lowercase();

    if name.ends_with(".tar.gz") || name.ends_with(".tgz") {
        return Ok(ArchiveFormat::TarGzip);
    }

    if name.ends_with(".tar.zst") {
        return Ok(ArchiveFormat::TarZstd);
    }

    match path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "zip" => Ok(ArchiveFormat::Zip),
        "tar" => Ok(ArchiveFormat::Tar),
        "gz" => Ok(ArchiveFormat::Gzip),
        "zst" => Ok(ArchiveFormat::Zstd),
        "7z" => Ok(ArchiveFormat::SevenZip),
        ext => Err(ArchiveError::UnsupportedFormat(ext.to_string())),
    }
}
