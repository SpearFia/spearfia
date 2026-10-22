# 3. License

- Status: accepted
- Date: 2026-10-18

## Context

Options considered: permissive dual MIT + Apache-2.0, open core, and AGPL. Spear Fia
wants broad adoption of its formats, plugins and assets, and compatibility with
FilmCraft (Apache-2.0) and the Rust ecosystem.

## Decision

All public Spear Fia repositories are dual licensed **MIT OR Apache-2.0**, at the user's
option. Each repo carries `LICENSE-MIT` and `LICENSE-APACHE`, copyright "Caesar Fia and
Spear Fia contributors". Contributions are accepted under the same terms. Starter-pack
assets carry their own SPDX license in their descriptor; see `starter-pack/LICENSES.md`.

## Consequences

Anyone can use Spear Fia code and formats, including commercially. Apache-2.0 adds an
explicit patent grant. Changing the license later would need contributor agreement, so
this is meant to be permanent.
