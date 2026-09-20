use std::{fs, io, io::Seek, io::Write, path::Path, path::PathBuf, sync::Arc};

use sevenz_rust::{SevenZArchiveEntry, SevenZReader, SevenZWriter};

use crate::entry::{ArchiveEntry, ContentFetcher};
use crate::error::{ArchiveError, Result};
use crate::util::{archive_root_name, normalize_path, not_found};

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
        let file = fs::File::open(path)?;
        writer.push_archive_entry(entry, Some(file))?;
    }

    Ok(())
}

pub fn create(input: &Path, output: &Path) -> Result<()> {
    let mut writer = SevenZWriter::create(output).map_err(ArchiveError::SevenZ)?;

    sevenz_add(&mut writer, input, input)?;

    writer.finish().map_err(ArchiveError::Io)?;

    Ok(())
}

pub fn extract(input: &Path, output: &Path) -> Result<()> {
    fs::create_dir_all(output)?;
    sevenz_rust::decompress_file(input, output)?;
    Ok(())
}

pub fn preview(input: &Path) -> Result<Vec<ArchiveEntry>> {
    let file = fs::File::open(input)?;

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

pub fn make_fetcher(input: PathBuf) -> ContentFetcher {
    Arc::new(move |path: &Path| {
        let want = normalize_path(path);
        let mut reader = SevenZReader::new(
            fs::File::open(&input)?,
            1 << 16,
            sevenz_rust::Password::empty(),
        )
        .map_err(|e| io::Error::other(e.to_string()))?;

        let mut found: Option<Vec<u8>> = None;

        reader
            .for_each_entries(|entry, data| {
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
