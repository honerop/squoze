use std::io;

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

pub type Result<T> = std::result::Result<T, ArchiveError>;
