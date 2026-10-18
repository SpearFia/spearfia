//! `spearfia-catalog`: asset catalog. Indexes and searches assets from pluggable sources
//! (local folders now; a Git-based community registry and third-party APIs later).
//!
//! Skeleton only: no features are implemented yet.

pub mod sources;

/// The crate name, used in logs.
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_namespaced() {
        assert_eq!(super::NAME, "spearfia-catalog");
    }
}
