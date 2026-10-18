//! `spearfia-analysis`: Analysis sidecars: read and write per-media analysis files (frames + rational timebase, IDs, analyzer version) and run analyzers.
//!
//! Skeleton only: no features are implemented yet.

/// The crate name, used in logs and analyzer/version stamps.
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_namespaced() {
        assert_eq!(super::NAME, "spearfia-analysis");
    }
}
