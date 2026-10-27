//! `spearfia-core`: project model and files. Includes project/channel/catalog state and the
//! validation behind agent document/cue edits. Changes are applied, recorded and undone by
//! the command layer in `spearfia-ops`.

pub mod app_state;
pub mod channels;
pub mod comments;
pub mod docx;
pub mod editing;
pub mod error;
pub mod links;
pub mod media;
mod media_ops;
pub mod project;
pub mod tabs;
pub mod visions;
pub mod visuals;

pub use error::CoreError;

/// A document's version: a hash of its exact bytes, e.g. `v3f9a1c0d2e4b`.
pub fn version(text: &str) -> String {
    // FNV-1a: stable across builds and processes, so a version printed by the CLI can be
    // passed back to it (or to the app or an agent) later.
    let h = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("v{:012x}", h >> 16)
}

use std::fmt;
use std::str::FromStr;

/// An asset ID such as `spearfia/focus-loupe@0.1.0`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssetId {
    pub namespace: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseAssetIdError(pub String);

impl fmt::Display for ParseAssetIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid asset id `{}`, expected <namespace>/<name>@<version>",
            self.0
        )
    }
}

impl std::error::Error for ParseAssetIdError {}

impl FromStr for AssetId {
    type Err = ParseAssetIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || ParseAssetIdError(s.to_owned());
        let (path, version) = s.split_once('@').ok_or_else(err)?;
        let (namespace, name) = path.split_once('/').ok_or_else(err)?;
        if [namespace, name, version].iter().any(|p| p.is_empty()) {
            return Err(err());
        }
        Ok(Self {
            namespace: namespace.into(),
            name: name.into(),
            version: version.into(),
        })
    }
}

impl fmt::Display for AssetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}@{}", self.namespace, self.name, self.version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_asset_id() {
        let id: AssetId = "spearfia/focus-loupe@0.1.0".parse().unwrap();
        assert_eq!(id.name, "focus-loupe");
        assert_eq!(id.to_string(), "spearfia/focus-loupe@0.1.0");
    }

    #[test]
    fn rejects_malformed_ids() {
        for bad in ["focus-loupe@0.1.0", "spearfia/focus-loupe", "/x@1", "a/@1"] {
            assert!(bad.parse::<AssetId>().is_err(), "{bad}");
        }
    }
}

#[cfg(test)]
pub(crate) mod testutil {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// A fresh, empty folder under the system temp dir for one test.
    pub fn temp_dir(label: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("spearfia-core-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
