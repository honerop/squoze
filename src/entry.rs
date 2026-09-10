use std::{io, path::Path, path::PathBuf, sync::Arc};

#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

pub type ContentFetcher = Arc<dyn Fn(&Path) -> io::Result<Vec<u8>> + Send + Sync>;
