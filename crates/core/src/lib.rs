//! `spearfia-core`: command layer, undo history and project model. Every UI, CLI, MCP and API
//! action is a typed command here, so AI and human edits share one undo history.
//!
//! Skeleton only: so far it holds the shared `AssetId` type.

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
