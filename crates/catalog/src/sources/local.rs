//! Local source: assets in the project's library roots (folders on disk or the external drive).

use super::SourceKind;
use std::path::PathBuf;

/// A local folder that holds asset folders, each with an `<id>.asset.json` descriptor.
#[derive(Debug, Clone)]
pub struct LocalSource {
    pub root: PathBuf,
}

impl LocalSource {
    pub fn kind(&self) -> SourceKind {
        SourceKind::Local
    }
}
