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

### Changed
- Improved crate metadata in Cargo.toml.
- Added .editorconfig and rustfmt.toml for consistent formatting.

## [0.1.0] - 2026-06-13

### Added
- Initial public release of tokensaver.
