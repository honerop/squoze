use std::{fs, io, io::Read, io::Seek, io::Write, path::Path, path::PathBuf, sync::Arc};

use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

use crate::entry::{ArchiveEntry, ContentFetcher};
use crate::error::{ArchiveError, Result};
use crate::util::{archive_root_name, normalize_path, not_found, safe_join};

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

        let mut file = fs::File::open(path)?;
        io::copy(&mut file, zip)?;
    }

    Ok(())
}

pub fn create(input: &Path, output: &Path) -> Result<()> {
    let file = fs::File::create(output)?;
    let mut zip = ZipWriter::new(file);

    zip_add(&mut zip, input, input)?;

    zip.finish()?;
    Ok(())
}

pub fn extract(input: &Path, output: &Path) -> Result<()> {
    fs::create_dir_all(output)?;

    let file = fs::File::open(input)?;
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

            let mut out = fs::File::create(dest)?;
            io::copy(&mut entry, &mut out)?;
        }
    }

    Ok(())
}

pub fn preview(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = fs::File::open(input)?;
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

pub fn make_fetcher(input: &Path) -> ContentFetcher {
    let input: PathBuf = input.into();

    Arc::new(move |path: &Path| {
        let want = normalize_path(path);
        let file = fs::File::open(&input)?;
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
