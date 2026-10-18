//! `spearfia-vision`: Vision resolution: layers app defaults, preset, project vision.yaml and section :vision{} overrides, resolves weights, and keeps the feedback log.
//!
//! Skeleton only: no features are implemented yet.

/// The crate name, used in logs and analyzer/version stamps.
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_namespaced() {
        assert_eq!(super::NAME, "spearfia-vision");
    }
}
