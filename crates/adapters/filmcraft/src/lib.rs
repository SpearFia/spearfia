//! `spearfia-adapter-filmcraft`: FilmCraft adapter (first-class): applies timeline plans through FilmCraft's MCP/control channel and reads editor state back.
//!
//! Skeleton only: no features are implemented yet.

/// The crate name, used in logs and analyzer/version stamps.
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_namespaced() {
        assert_eq!(super::NAME, "spearfia-adapter-filmcraft");
    }
}
