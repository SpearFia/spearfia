// SPDX-License-Identifier: MIT OR Apache-2.0
//! Measured word alignment, source fingerprints and timeline word positions.
mod alignment;
pub use alignment::*;

/// The crate name, used in logs and analyzer/version stamps.
pub const NAME: &str = env!("CARGO_PKG_NAME");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_name_is_namespaced() {
        assert_eq!(super::NAME, "spearfia-analysis");
    }
}
