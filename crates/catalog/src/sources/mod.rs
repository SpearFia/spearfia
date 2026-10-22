//! Catalog sources. Each source lists assets from one place. `local` is the only source
//! for now; `registry` (Git-based community registry) and third-party API sources come later.

pub mod local;

/// Where a source finds assets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Local,
    Registry,
    ThirdParty,
}
