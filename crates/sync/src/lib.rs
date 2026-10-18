//! `spearfia-sync`: Two-way editor sync: compiles timeline plans, keeps the cue ID map, and three-way merges editor changes against the last-synced snapshot.
//!
//! Skeleton only: no features are implemented yet.

/// The crate name, used in logs and analyzer/version stamps.
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_namespaced() {
        assert_eq!(super::NAME, "spearfia-sync");
    }
}
