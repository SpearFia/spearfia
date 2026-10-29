//! `spearfia-adapter-filmcraft`: FilmCraft adapter. Applies timeline plans through
//! FilmCraft's MCP/control channel and reads editor state back.
//!
//! So far: finding the FilmCraft install ([`detect`]) and the one-way push ([`push()`],
//! ADR 0005) through `filmcraft-cli mcp` ([`mcp`]).

use std::path::{Path, PathBuf};

mod edit;
pub mod mcp;
mod push;
pub use push::{Options, Pushed, Section, push, push_with};

/// The `filmcraft-cli` to push with, or an error that says how to point Spear Fia at it.
pub fn find_cli() -> spearfia_core::error::Result<PathBuf> {
    detect().and_then(|i| i.cli).ok_or_else(|| {
        spearfia_core::CoreError::Invalid(format!(
            "filmcraft-cli not found. Install FilmCraft, add it to PATH, or set {ENV_OVERRIDE} to its folder."
        ))
    })
}

/// The crate name, used in logs.
pub const NAME: &str = env!("CARGO_PKG_NAME");

/// Overrides detection: the FilmCraft install folder or the `filmcraft` executable.
pub const ENV_OVERRIDE: &str = "SPEARFIA_FILMCRAFT";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    /// The app (`filmcraft`), if found.
    pub app: Option<PathBuf>,
    /// The CLI with the MCP server (`filmcraft-cli`), if found.
    pub cli: Option<PathBuf>,
}

impl Install {
    pub fn folder(&self) -> Option<&Path> {
        self.app.as_deref().or(self.cli.as_deref())?.parent()
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    }
}

/// Look for FilmCraft in `SPEARFIA_FILMCRAFT`, the usual install folders, then `PATH`.
pub fn detect() -> Option<Install> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(v) = std::env::var_os(ENV_OVERRIDE) {
        let p = PathBuf::from(v);
        if p.is_file() {
            dirs.extend(p.parent().map(Path::to_path_buf));
        } else {
            dirs.push(p);
        }
    }
    if cfg!(windows) {
        for var in ["ProgramFiles", "LOCALAPPDATA"] {
            if let Some(base) = std::env::var_os(var) {
                let base = PathBuf::from(base);
                dirs.push(base.join("FilmCraft"));
                dirs.push(base.join("Programs").join("FilmCraft"));
            }
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    detect_in(&dirs)
}

/// Detection over an explicit list of folders (first match wins for each binary).
pub fn detect_in(dirs: &[PathBuf]) -> Option<Install> {
    let find = |name: &str| dirs.iter().map(|d| d.join(exe(name))).find(|p| p.is_file());
    let install = Install {
        app: find("filmcraft"),
        cli: find("filmcraft-cli"),
    };
    (install.app.is_some() || install.cli.is_some()).then_some(install)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_binaries_in_given_folders() {
        let dir = std::env::temp_dir().join(format!("spearfia-fc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(detect_in(std::slice::from_ref(&dir)), None);

        std::fs::write(dir.join(exe("filmcraft-cli")), b"").unwrap();
        let found = detect_in(std::slice::from_ref(&dir)).unwrap();
        assert!(found.app.is_none());
        assert_eq!(found.cli, Some(dir.join(exe("filmcraft-cli"))));
        assert_eq!(found.folder(), Some(dir.as_path()));
    }
}
