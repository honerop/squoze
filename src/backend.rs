use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use sevenz_rust::{SevenZArchiveEntry, SevenZReader, SevenZWriter};
use std::{
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tar::{Archive, Builder};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::preview::{self, ArchiveEntry};

#[derive(Debug)]
pub enum ArchiveError {
    Io(io::Error),
    Zip(zip::result::ZipError),
    SevenZ(sevenz_rust::Error),
    UnsupportedFormat(String),
    InvalidPath(String),
    InvalidArguments(String),
}

impl std::fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Zip(e) => write!(f, "ZIP error: {e}"),
            Self::SevenZ(e) => write!(f, "7z error: {e}"),
            Self::UnsupportedFormat(ext) => {
                write!(f, "unsupported format: {ext}")
            }
            Self::InvalidPath(p) => {
                write!(f, "invalid path: {p}")
            }
            Self::InvalidArguments(m) => {
                write!(f, "{m}")
            }
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<io::Error> for ArchiveError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<zip::result::ZipError> for ArchiveError {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Zip(e)
    }
}

impl From<sevenz_rust::Error> for ArchiveError {
    fn from(e: sevenz_rust::Error) -> Self {
        Self::SevenZ(e)
    }
}

type Result<T> = std::result::Result<T, ArchiveError>;

#[derive(Clone, Copy, Debug)]
enum ArchiveFormat {
    Zip,
    Tar,
    Gzip,
    TarGzip,
    Zstd,
    TarZstd,
    SevenZip,
}

fn archive_format(path: &Path) -> Result<ArchiveFormat> {
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

fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

fn archive_root_name(path: &Path) -> String {
    path.file_name()
        .and_then(|x| x.to_str())
        .unwrap_or("archive")
        .to_string()
}

fn add_tar_path<W: Write>(builder: &mut Builder<W>, input: &Path) -> Result<()> {
    let name = archive_root_name(input);

    if input.is_dir() {
        builder.append_dir_all(&name, input)?;
    } else {
        builder.append_path_with_name(input, &name)?;
    }

    Ok(())
}

fn zip_add<W: Write + Seek>(zip: &mut ZipWriter<W>, path: &Path, base: &Path) -> Result<()> {
    let rel = if path == base {
        PathBuf::from(archive_root_name(path))
    } else {
        let mut rel = PathBuf::from(archive_root_name(base));
        rel.push(path.strip_prefix(base).unwrap());
        rel
    };

    if path.is_dir() {
        zip.add_directory(format!("{}/", rel.display()), SimpleFileOptions::default())?;

        for entry in fs::read_dir(path)? {
            zip_add(zip, &entry?.path(), base)?;
        }
    } else {
        zip.start_file(
            rel.to_string_lossy().replace('\\', "/"),
            SimpleFileOptions::default(),
        )?;

        let mut file = File::open(path)?;
        io::copy(&mut file, zip)?;
    }

    Ok(())
}

fn safe_join(root: &Path, archive_path: &Path) -> Result<PathBuf> {
    let mut out = root.to_path_buf();

    for c in archive_path.components() {
        match c {
            Component::Normal(v) => out.push(v),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ArchiveError::InvalidPath(
                    archive_path.display().to_string(),
                ));
            }
        }
    }

    Ok(out)
}

/* ZIP */

fn create_zip(input: &Path, output: &Path) -> Result<()> {
    let file = File::create(output)?;
    let mut zip = ZipWriter::new(file);

    zip_add(&mut zip, input, input)?;

    zip.finish()?;
    Ok(())
}

fn extract_zip(input: &Path, output: &Path) -> Result<()> {
    fs::create_dir_all(output)?;

    let file = File::open(input)?;
    let mut zip = ZipArchive::new(file)?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;

        let enclosed = entry
            .enclosed_name()
            .ok_or_else(|| ArchiveError::InvalidPath(entry.name().to_string()))?;

        let dest = safe_join(output, &enclosed)?;

        if entry.is_dir() {
            fs::create_dir_all(&dest)?;
        } else {
            if let Some(p) = dest.parent() {
                fs::create_dir_all(p)?;
            }

            let mut out = File::create(dest)?;
            io::copy(&mut entry, &mut out)?;
        }
    }

    Ok(())
}
pub fn preview_zip(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(input)?;
    let mut archive = ZipArchive::new(file)?;

    let mut entries = Vec::with_capacity(archive.len());

    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;

        entries.push(ArchiveEntry {
            path: PathBuf::from(entry.name()),
            is_dir: entry.is_dir(),
            size: entry.size(),
        });
    }

    Ok(entries)
}

/* TAR */

fn create_tar(input: &Path, output: &Path) -> Result<()> {
    let file = File::create(output)?;
    let mut builder = Builder::new(file);

    add_tar_path(&mut builder, input)?;

    builder.finish()?;
    Ok(())
}

fn extract_tar_with<R: Read>(reader: R, output: &Path) -> Result<()> {
    fs::create_dir_all(output)?;

    let mut archive = Archive::new(reader);

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let dest = safe_join(output, &path)?;

        if let Some(p) = dest.parent() {
            fs::create_dir_all(p)?;
        }

        entry.unpack(dest)?;
    }

    Ok(())
}

fn extract_tar(input: &Path, output: &Path) -> Result<()> {
    extract_tar_with(File::open(input)?, output)
}

pub fn preview_tar_with<R: Read>(reader: R) -> Result<Vec<ArchiveEntry>> {
    let mut archive = Archive::new(reader);

    let mut entries = Vec::new();

    for entry in archive.entries()? {
        let entry = entry?;

        let path = entry.path()?.to_path_buf();
        let is_dir = entry.header().entry_type().is_dir();
        let size = entry.header().size()?;

        entries.push(ArchiveEntry { path, is_dir, size });
    }

    Ok(entries)
}

pub fn preview_tar(input: &Path) -> Result<Vec<ArchiveEntry>> {
    preview_tar_with(File::open(input)?)
}

/* GZIP */

fn create_gzip(input: &Path, output: &Path) -> Result<()> {
    if !input.is_file() {
        return Err(ArchiveError::InvalidArguments(
            "gzip only accepts a file".into(),
        ));
    }

    let mut input_file = File::open(input)?;
    let output_file = File::create(output)?;

    let mut encoder = GzEncoder::new(output_file, Compression::default());

    io::copy(&mut input_file, &mut encoder)?;
    encoder.finish()?;
    Ok(())
}

fn extract_gzip(input: &Path, output: &Path) -> Result<()> {
    let input_file = File::open(input)?;
    let mut decoder = GzDecoder::new(input_file);
    let mut output_file = File::create(output)?;

    io::copy(&mut decoder, &mut output_file)?;
    Ok(())
}
fn decoded_size<R: Read>(mut reader: R) -> io::Result<u64> {
    Ok(io::copy(&mut reader, &mut io::sink())?)
}

fn gzip_uncompressed_size(input: &Path) -> io::Result<u64> {
    let mut file = File::open(input)?;

    let mut header = [0u8; 2];
    file.read_exact(&mut header)?;

    if header == [0x1f, 0x8b] && file.metadata()?.len() >= 8 {
        file.seek(SeekFrom::End(-4))?;

        let mut trailer = [0u8; 4];
        file.read_exact(&mut trailer)?;

        return Ok(u32::from_le_bytes(trailer) as u64);
    }

    decoded_size(GzDecoder::new(File::open(input)?))
}

pub fn preview_gzip(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let name = input
        .file_stem()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("output"));

    Ok(vec![ArchiveEntry {
        path: name,
        is_dir: false,
        size: gzip_uncompressed_size(input)?,
    }])
}

/* TAR.GZ */

fn create_tar_gz(input: &Path, output: &Path) -> Result<()> {
    let file = File::create(output)?;

    let encoder = GzEncoder::new(file, Compression::default());

    let mut builder = Builder::new(encoder);

    add_tar_path(&mut builder, input)?;

    let encoder = builder.into_inner()?;
    encoder.finish()?;
    Ok(())
}

fn extract_tar_gz(input: &Path, output: &Path) -> Result<()> {
    let file = File::open(input)?;
    extract_tar_with(GzDecoder::new(file), output)
}

pub fn preview_tar_gz(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(input)?;
    preview_tar_with(GzDecoder::new(file))
}

/* ZSTD */

fn create_zstd(input: &Path, output: &Path) -> Result<()> {
    if !input.is_file() {
        return Err(ArchiveError::InvalidArguments(
            "zstd only accepts a file".into(),
        ));
    }

    let mut input_file = File::open(input)?;
    let output_file = File::create(output)?;

    let mut encoder = zstd::Encoder::new(output_file, 3)?;

    io::copy(&mut input_file, &mut encoder)?;
    encoder.finish()?;
    Ok(())
}

fn extract_zstd(input: &Path, output: &Path) -> Result<()> {
    let input_file = File::open(input)?;
    let mut decoder = zstd::Decoder::new(input_file)?;
    let mut output_file = File::create(output)?;

    io::copy(&mut decoder, &mut output_file)?;
    Ok(())
}
fn zstd_uncompressed_size(input: &Path) -> io::Result<u64> {
    let mut file = File::open(input)?;

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

pub fn preview_zstd(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let name = input
        .file_stem()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("output"));

    Ok(vec![ArchiveEntry {
        path: name,
        is_dir: false,
        size: zstd_uncompressed_size(input)?,
    }])
}

/* TAR.ZST */

fn create_tar_zst(input: &Path, output: &Path) -> Result<()> {
    let file = File::create(output)?;
    let encoder = zstd::Encoder::new(file, 3)?;
    let mut builder = Builder::new(encoder);

    add_tar_path(&mut builder, input)?;

    let encoder = builder.into_inner()?;
    encoder.finish()?;
    Ok(())
}

fn extract_tar_zst(input: &Path, output: &Path) -> Result<()> {
    let file = File::open(input)?;
    let decoder = zstd::Decoder::new(file)?;
    extract_tar_with(decoder, output)
}

pub fn preview_tar_zst(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(input)?;
    let decoder = zstd::stream::read::Decoder::new(file)?;
    preview_tar_with(decoder)
}

/* 7Z */

fn sevenz_add<W: Write + Seek>(
    writer: &mut SevenZWriter<W>,
    path: &Path,
    base: &Path,
) -> Result<()> {
    let rel = if path == base {
        PathBuf::from(archive_root_name(path))
    } else {
        let mut rel = PathBuf::from(archive_root_name(base));
        rel.push(path.strip_prefix(base).unwrap());
        rel
    };

    let entry = SevenZArchiveEntry::from_path(path, rel.to_string_lossy().to_string());

    if path.is_dir() {
        writer.push_archive_entry::<&[u8]>(entry, None)?;

        for child in fs::read_dir(path)? {
            sevenz_add(writer, &child?.path(), base)?;
        }
    } else {
        let file = File::open(path)?;
        writer.push_archive_entry(entry, Some(file))?;
    }

    Ok(())
}

fn create_7z(input: &Path, output: &Path) -> Result<()> {
    let mut writer =
        SevenZWriter::create(output).map_err(ArchiveError::SevenZ)?;

    sevenz_add(&mut writer, input, input)?;

    writer.finish().map_err(ArchiveError::Io)?;

    Ok(())
}

fn extract_7z(input: &Path, output: &Path) -> Result<()> {
    fs::create_dir_all(output)?;
    sevenz_rust::decompress_file(input, output)?;
    Ok(())
}

pub fn preview_7z(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = File::open(input)?;

    let mut reader = SevenZReader::new(file, 1 << 16, sevenz_rust::Password::empty())?;

    let mut entries = Vec::new();

    reader.for_each_entries(|entry, _reader| {
        entries.push(ArchiveEntry {
            path: PathBuf::from(entry.name()),
            is_dir: entry.is_directory(),
            size: entry.size(),
        });

        Ok(true)
    })?;

    Ok(entries)
}

/* Content fetchers (on-demand entry readers for the preview TUI) */

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect::<Vec<_>>()
        .join("/")
}

fn not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "entry not found")
}

fn make_zip_fetcher(input: &Path) -> preview::ContentFetcher {
    let input: PathBuf = input.into();

    Arc::new(move |path: &Path| {
        let want = normalize_path(path);
        let file = File::open(&input)?;
        let mut archive = ZipArchive::new(file).map_err(|e| io::Error::other(e.to_string()))?;

        let mut data = Vec::new();

        if let Ok(mut entry) = archive.by_name(&want) {
            if !entry.is_dir() {
                entry.read_to_end(&mut data)?;
                return Ok(data);
            }
        }

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| io::Error::other(e.to_string()))?;

            if !entry.is_dir() && normalize_path(Path::new(entry.name())) == want {
                entry.read_to_end(&mut data)?;
                return Ok(data);
            }
        }

        Err(not_found())
    })
}

fn make_tar_fetcher<F>(input: PathBuf, decoder: F) -> preview::ContentFetcher
where
    F: Fn(File) -> io::Result<Box<dyn Read>> + Send + Sync + 'static,
{
    Arc::new(move |path: &Path| {
        let want = normalize_path(path);
        let file = File::open(&input)?;
        let mut archive = Archive::new(decoder(file)?);

        for entry in archive.entries()? {
            let mut entry = entry?;

            if entry.header().entry_type().is_dir() {
                continue;
            }

            if normalize_path(&entry.path()?.to_path_buf()) == want {
                let mut data = Vec::new();
                entry.read_to_end(&mut data)?;
                return Ok(data);
            }
        }

        Err(not_found())
    })
}

fn make_gzip_fetcher(input: PathBuf) -> preview::ContentFetcher {
    Arc::new(move |_path| {
        let mut decoder = GzDecoder::new(File::open(&input)?);

        let mut data = Vec::new();
        decoder.read_to_end(&mut data)?;

        Ok(data)
    })
}

fn make_zstd_fetcher(input: PathBuf) -> preview::ContentFetcher {
    Arc::new(move |_path| {
        let mut decoder = zstd::Decoder::new(File::open(&input)?)?;

        let mut data = Vec::new();
        decoder.read_to_end(&mut data)?;

        Ok(data)
    })
}

fn make_sevenz_fetcher(input: PathBuf) -> preview::ContentFetcher {
    Arc::new(move |path: &Path| {
        let want = normalize_path(path);
        let mut reader = SevenZReader::new(File::open(&input)?, 1 << 16, sevenz_rust::Password::empty())
            .map_err(|e| io::Error::other(e.to_string()))?;

        let mut found: Option<Vec<u8>> = None;

        reader.for_each_entries(|entry, data| {
            if found.is_none()
                && !entry.is_directory()
                && normalize_path(Path::new(entry.name())) == want
            {
                let mut buf = Vec::new();
                data.read_to_end(&mut buf)?;
                found = Some(buf);

                return Ok(false);
            }

            Ok(true)
        })
        .map_err(|e| io::Error::other(e.to_string()))?;

        found.ok_or_else(not_found)
    })
}

/* Backend */

pub struct StandardBackend;

impl StandardBackend {
    pub fn create(input: &Path, output: &Path) -> Result<()> {
        if !input.exists() {
            return Err(ArchiveError::InvalidPath(input.display().to_string()));
        }

        ensure_parent(output)?;

        match archive_format(output)? {
            ArchiveFormat::Zip => create_zip(input, output)?,
            ArchiveFormat::Tar => create_tar(input, output)?,
            ArchiveFormat::Gzip => create_gzip(input, output)?,
            ArchiveFormat::TarGzip => create_tar_gz(input, output)?,
            ArchiveFormat::Zstd => create_zstd(input, output)?,
            ArchiveFormat::TarZstd => create_tar_zst(input, output)?,
            ArchiveFormat::SevenZip => create_7z(input, output)?,
        }

        Ok(())
    }

    pub fn extract(input: &Path, output: &Path) -> Result<()> {
        if !input.exists() {
            return Err(ArchiveError::InvalidPath(input.display().to_string()));
        }

        match archive_format(input)? {
            ArchiveFormat::Zip => extract_zip(input, output)?,
            ArchiveFormat::Tar => extract_tar(input, output)?,
            ArchiveFormat::Gzip => extract_gzip(input, output)?,
            ArchiveFormat::TarGzip => extract_tar_gz(input, output)?,
            ArchiveFormat::Zstd => extract_zstd(input, output)?,
            ArchiveFormat::TarZstd => extract_tar_zst(input, output)?,
            ArchiveFormat::SevenZip => extract_7z(input, output)?,
        }

        Ok(())
    }
    pub fn preview(input: &Path) -> Result<()> {
        let format = archive_format(input)?;

        let (archive_entries, read_content) = match format {
            ArchiveFormat::Zip => (preview_zip(input)?, make_zip_fetcher(input)),
            ArchiveFormat::Tar => (
                preview_tar(input)?,
                make_tar_fetcher(input.into(), |file| Ok(Box::new(file) as Box<dyn Read>)),
            ),
            ArchiveFormat::Gzip => (preview_gzip(input)?, make_gzip_fetcher(input.into())),
            ArchiveFormat::TarGzip => (
                preview_tar_gz(input)?,
                make_tar_fetcher(input.into(), |file| {
                    Ok(Box::new(GzDecoder::new(file)) as Box<dyn Read>)
                }),
            ),
            ArchiveFormat::Zstd => (preview_zstd(input)?, make_zstd_fetcher(input.into())),
            ArchiveFormat::TarZstd => (
                preview_tar_zst(input)?,
                make_tar_fetcher(input.into(), |file| {
                    Ok(Box::new(zstd::stream::read::Decoder::new(file)?) as Box<dyn Read>)
                }),
            ),
            ArchiveFormat::SevenZip => (preview_7z(input)?, make_sevenz_fetcher(input.into())),
        };

        preview::run_preview(archive_entries, read_content)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fixture(ext: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("project");

        fs::create_dir_all(input.join("src/parser")).unwrap();
        fs::create_dir_all(input.join("docs/guides")).unwrap();
        fs::write(input.join("README.md"), "readme").unwrap();
        fs::write(input.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(input.join("src/parser/lexer.rs"), "lexer").unwrap();
        fs::write(input.join("docs/guides/intro.md"), "intro").unwrap();

        let output = dir.path().join(format!("project{ext}"));

        StandardBackend::create(&input, &output).unwrap();

        (dir, output)
    }

    fn sorted_names(entries: &[ArchiveEntry]) -> Vec<String> {
        let mut names: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().replace('\\', "/"))
            .collect();
        names.sort();
        names
    }

    fn preview_of(fx: &PathBuf) -> Vec<ArchiveEntry> {
        match archive_format(fx).unwrap() {
            ArchiveFormat::Zip => preview_zip(fx).unwrap(),
            ArchiveFormat::Tar => preview_tar(fx).unwrap(),
            ArchiveFormat::TarGzip => preview_tar_gz(fx).unwrap(),
            ArchiveFormat::TarZstd => preview_tar_zst(fx).unwrap(),
            ArchiveFormat::SevenZip => preview_7z(fx).unwrap(),
            other => panic!("unsupported test format: {other:?}"),
        }
    }

    fn assert_roundtrip(ext: &str) {
        let (_dir, output) = fixture(ext);

        let names = sorted_names(&preview_of(&output));

        for expected in [
            "project/README.md",
            "project/src/main.rs",
            "project/src/parser/lexer.rs",
            "project/docs/guides/intro.md",
        ] {
            assert!(
                names.iter().any(|n| n.contains(expected)),
                "{ext}: missing {expected} in {names:?}"
            );
        }

        let out = output.parent().unwrap().join("extracted");
        StandardBackend::extract(&output, &out).unwrap();

        assert_eq!(
            fs::read_to_string(out.join("project/README.md")).unwrap(),
            "readme"
        );
        assert_eq!(
            fs::read_to_string(out.join("project/src/parser/lexer.rs")).unwrap(),
            "lexer"
        );
        assert_eq!(
            fs::read_to_string(out.join("project/docs/guides/intro.md")).unwrap(),
            "intro"
        );
    }

    #[test]
    fn zip_roundtrip() {
        assert_roundtrip(".zip");
    }

    #[test]
    fn tar_roundtrip() {
        assert_roundtrip(".tar");
    }

    #[test]
    fn tar_gz_roundtrip() {
        assert_roundtrip(".tar.gz");
    }

    #[test]
    fn tar_zst_roundtrip() {
        assert_roundtrip(".tar.zst");
    }

    #[test]
    fn sevenz_roundtrip() {
        assert_roundtrip(".7z");
    }

    #[test]
    fn zip_preview_reports_dirs() {
        let (_dir, output) = fixture(".zip");

        let entries = preview_zip(&output).unwrap();

        assert!(
            entries.iter().filter(|e| e.is_dir).count() >= 5,
            "expected dir entries, got: {:?}",
            sorted_names(&entries)
        );
    }

    #[test]
    fn gzip_roundtrip_and_size() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("file.txt");
        fs::write(&input, vec![b'x'; 4321]).unwrap();

        let output = dir.path().join("file.txt.gz");
        StandardBackend::create(&input, &output).unwrap();

        let entries = preview_gzip(&output).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("file.txt"));
        assert_eq!(entries[0].size, 4321);

        let out = dir.path().join("out");
        StandardBackend::extract(&output, &out).unwrap();

        assert_eq!(fs::read(&out).unwrap(), vec![b'x'; 4321]);
    }

    #[test]
    fn zstd_roundtrip_and_size() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("file.txt");
        fs::write(&input, vec![b'y'; 70000]).unwrap();

        let output = dir.path().join("file.zst");
        StandardBackend::create(&input, &output).unwrap();

        let entries = preview_zstd(&output).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size, 70000);

        let out = dir.path().join("out");
        StandardBackend::extract(&output, &out).unwrap();

        assert_eq!(fs::read(&out).unwrap(), vec![b'y'; 70000]);
    }

    #[test]
    fn rejects_missing_input() {
        let dir = TempDir::new().unwrap();

        let zip = dir.path().join("out.zip");
        assert!(StandardBackend::create(&dir.path().join("nope"), &zip).is_err());
        assert!(StandardBackend::extract(&dir.path().join("nope.zip"), &zip).is_err());

        assert!(StandardBackend::preview(&dir.path().join("nope.7z")).is_err());
    }

    #[test]
    fn rejects_unsupported_format() {
        let dir = TempDir::new().unwrap();

        assert!(matches!(
            archive_format(&dir.path().join("x.rar")).unwrap_err(),
            ArchiveError::UnsupportedFormat(_)
        ));
    }

    #[test]
    fn format_detection() {
        let dir = TempDir::new().unwrap();

        let p = |name: &str| dir.path().join(name);

        assert!(matches!(archive_format(&p("a.zip")), Ok(ArchiveFormat::Zip)));
        assert!(matches!(archive_format(&p("a.tar")), Ok(ArchiveFormat::Tar)));
        assert!(matches!(
            archive_format(&p("a.tar.gz")),
            Ok(ArchiveFormat::TarGzip)
        ));
        assert!(matches!(archive_format(&p("a.tgz")), Ok(ArchiveFormat::TarGzip)));
        assert!(matches!(
            archive_format(&p("a.tar.zst")),
            Ok(ArchiveFormat::TarZstd)
        ));
        assert!(matches!(archive_format(&p("a.gz")), Ok(ArchiveFormat::Gzip)));
        assert!(matches!(archive_format(&p("a.zst")), Ok(ArchiveFormat::Zstd)));
        assert!(matches!(
            archive_format(&p("a.7z")),
            Ok(ArchiveFormat::SevenZip)
        ));
    }
}

#[cfg(test)]
mod fetcher_tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn fetchers_read_entries() {
        for ext in [".zip", ".tar", ".tar.gz", ".tar.zst", ".7z"] {
            let dir = TempDir::new().unwrap();
            let input = dir.path().join("project");

            fs::create_dir_all(input.join("src/parser")).unwrap();
            fs::write(input.join("README.md"), "readme").unwrap();
            fs::write(input.join("src/main.rs"), "fn main() {}").unwrap();

            let output = dir.path().join(format!("project{ext}"));
            StandardBackend::create(&input, &output).unwrap();

            let fetcher: preview::ContentFetcher = match archive_format(&output).unwrap() {
                ArchiveFormat::Zip => make_zip_fetcher(&output),
                ArchiveFormat::Tar => {
                    make_tar_fetcher(output.clone(), |f| Ok(Box::new(f) as Box<dyn Read>))
                }
                ArchiveFormat::TarGzip => make_tar_fetcher(output.clone(), |f| {
                    Ok(Box::new(GzDecoder::new(f)) as Box<dyn Read>)
                }),
                ArchiveFormat::TarZstd => make_tar_fetcher(output.clone(), |f| {
                    Ok(Box::new(zstd::stream::read::Decoder::new(f)?) as Box<dyn Read>)
                }),
                ArchiveFormat::SevenZip => make_sevenz_fetcher(output.clone()),
                other => panic!("unexpected: {other:?}"),
            };

            assert_eq!(
                fetcher(&Path::new("project/README.md")).unwrap(),
                b"readme".to_vec(),
                "{ext}"
            );
            assert_eq!(
                fetcher(&Path::new("project/src/main.rs")).unwrap(),
                b"fn main() {}".to_vec(),
                "{ext}"
            );
            assert!(fetcher(&Path::new("project/src/")).is_err(), "{ext}");
            assert!(fetcher(&Path::new("no/such/file")).is_err(), "{ext}");
        }
    }

    #[test]
    fn single_file_fetchers() {
        let dir = TempDir::new().unwrap();

        let txt = dir.path().join("note.txt");
        fs::write(&txt, "hello").unwrap();

        let gz = dir.path().join("note.txt.gz");
        StandardBackend::create(&txt, &gz).unwrap();
        assert_eq!(
            make_gzip_fetcher(gz.clone())(&Path::new("whatever")).unwrap(),
            b"hello".to_vec()
        );

        let zst = dir.path().join("note.zst");
        StandardBackend::create(&txt, &zst).unwrap();
        assert_eq!(
            make_zstd_fetcher(zst.clone())(&Path::new("whatever")).unwrap(),
            b"hello".to_vec()
        );
    }
}
