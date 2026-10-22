//! Errors returned by `spearfia-core`.

use std::fmt;
use std::path::PathBuf;

/// Anything that can go wrong reading or writing a project.
#[derive(Debug)]
pub enum CoreError {
    /// A file or folder operation failed.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A JSON file could not be read or written.
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    /// The request itself is not valid (empty name, folder already holds a project, ...).
    Invalid(String),
}

impl CoreError {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Self::Json {
            path: path.into(),
            source,
        }
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Json { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Invalid(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            Self::Invalid(_) => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
