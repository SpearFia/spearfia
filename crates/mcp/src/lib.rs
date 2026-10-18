//! `spearfia-mcp`: MCP server exposing the command layer and asset capabilities to agents, with progressive disclosure.
//!
//! Skeleton only: no features are implemented yet.

/// The crate name, used in logs and analyzer/version stamps.
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_namespaced() {
        assert_eq!(super::NAME, "spearfia-mcp");
    }
}
