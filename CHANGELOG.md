# Changelog

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog and this project adheres to Semantic
Versioning.

## [Unreleased]

### Added
- Open source community health files (Code of Conduct, Security, Support).
- GitHub issue templates and pull request template.
- CI, release, security, and GitHub Pages workflows.
- Documentation website scaffold in docs/.
- Cross-platform prebuilt release archives (Windows, Linux, macOS; x86_64 and
  arm64) published automatically when a `v*.*.*` tag is pushed, each with a
  SHA-256 checksum.
- One-line install scripts: `install.sh` (Linux/macOS) and `install.ps1`
  (Windows), plus `cargo install` support and an Installation docs page.
- `token-saver update` command to check GitHub Releases for a newer version and
  self-update the binary in place (with `--check` and `--force` options), plus
  `token-saver version` / `--version` / `-V` to print the installed version.
- `token-saver gallery` (alias `marketplace`): a local, dependency-free gallery
  for user-defined Copilot context objects. Harvests user-authored agents,
  skills, prompts and instructions out of `~/.copilot`, `~/.agents`, the VS Code
  `User/prompts` folder and home-level instruction files into
  `~/.token-saver/gallery` (dry run by default; `--apply` moves them and leaves
  VS Code extension objects untouched). Includes `list`, `show`, `install`
  (into standard workspace Copilot paths), `remove`, and `serve` — a
  localhost-only browser gallery to browse and install items.

### Changed
- Improved crate metadata in Cargo.toml.
- Added .editorconfig and rustfmt.toml for consistent formatting.

## [0.1.0] - 2026-06-13

### Added
- Initial public release of token-saver.
