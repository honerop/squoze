use std::{fs, io, io::Read, path::Path, path::PathBuf, sync::Arc};

use tar::{Archive, Builder};

use crate::entry::{ArchiveEntry, ContentFetcher};
use crate::error::Result;
use crate::util::{archive_root_name, normalize_path, not_found, safe_join};

pub(crate) fn add_tar_path<W: std::io::Write>(builder: &mut Builder<W>, input: &Path) -> Result<()> {
    let name = archive_root_name(input);

    if input.is_dir() {
        builder.append_dir_all(&name, input)?;
    } else {
        builder.append_path_with_name(input, &name)?;
    }

    Ok(())
}

pub fn create(input: &Path, output: &Path) -> Result<()> {
    let file = fs::File::create(output)?;
    let mut builder = Builder::new(file);

    add_tar_path(&mut builder, input)?;

    builder.finish()?;
    Ok(())
}

pub fn extract_with<R: Read>(reader: R, output: &Path) -> Result<()> {
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

pub fn extract(input: &Path, output: &Path) -> Result<()> {
    extract_with(fs::File::open(input)?, output)
}

pub fn preview_with<R: Read>(reader: R) -> Result<Vec<ArchiveEntry>> {
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

pub fn preview(input: &Path) -> Result<Vec<ArchiveEntry>> {
    preview_with(fs::File::open(input)?)
}

pub fn make_fetcher<F>(input: PathBuf, decoder: F) -> ContentFetcher
where
    F: Fn(fs::File) -> io::Result<Box<dyn Read>> + Send + Sync + 'static,
{
    Arc::new(move |path: &Path| {
        let want = normalize_path(path);
        let file = fs::File::open(&input)?;
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
