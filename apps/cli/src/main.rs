//! `spearfia`: command-line interface. Calls the same command layer as the desktop app.
//!
//! Skeleton only: prints the version.

fn version_line() -> String {
    format!("spearfia {}", env!("CARGO_PKG_VERSION"))
}

fn main() {
    println!("{}", version_line());
}

#[cfg(test)]
mod tests {
    #[test]
    fn version_line_names_the_binary() {
        assert!(super::version_line().starts_with("spearfia "));
    }
}
