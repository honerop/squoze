use std::{
    fs::{self},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};

use crate::error::{ArchiveError, Result};

pub fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    Ok(())
}

pub fn archive_root_name(path: &Path) -> String {
    path.file_name()
        .and_then(|x| x.to_str())
        .unwrap_or("archive")
        .to_string()
}

pub fn safe_join(root: &Path, archive_path: &Path) -> Result<PathBuf> {
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

pub fn normalize_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .split('/')
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .collect::<Vec<_>>()
        .join("/")
}

pub fn not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "entry not found")
}

pub fn decoded_size<R: Read>(mut reader: R) -> io::Result<u64> {
    Ok(io::copy(&mut reader, &mut io::sink())?)
}
