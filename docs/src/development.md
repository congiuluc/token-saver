# Development Guide

## Local setup

1. Install Rust stable with rustfmt and clippy.
2. Clone the repository.
3. Build and run tests.

```bash
rustup component add rustfmt clippy
cargo build
cargo test --all
```

## Contribution expectations

- Keep PRs focused and small.
- Add tests for behavior changes.
- Update docs for user-facing changes.
- Follow formatter and lint requirements before opening a PR.

## CI baseline

The repository workflows validate:

- Formatting
- Clippy warnings as errors
- Unit and integration tests
- Dependency audits
- CodeQL analysis
- GitHub Pages documentation build
