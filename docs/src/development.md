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

## Project layout

The project is a small Rust CLI with two binaries (`token-saver` and `tks`) that
both delegate to `token_saver::run()` in `src/lib.rs`. See
[Architecture](architecture.md) for the full module map. Most work falls into one
of these areas:

- **Dispatch / CLI** — `src/lib.rs` (argument parsing and subcommands).
- **Formatters** — `src/format/*.rs` (per-tool summarization).
- **Token accounting** — `src/tokenizer.rs`, `src/metrics.rs`.
- **Copilot tooling** — `src/init.rs`, `src/hook.rs`, `src/assess.rs`, `src/gallery.rs`.

## Building and running

```bash
cargo build --release          # optimized binaries in target/release
./target/release/token-saver git status
./target/release/tks --help
```

## Tests

```bash
cargo test --all               # unit + integration tests
```

Integration tests in `tests/cli.rs` run the compiled `token-saver` binary
end-to-end via Cargo's `CARGO_BIN_EXE_token-saver` path. Unit tests live
alongside their modules.

## Documentation

The documentation site is built with mdBook from `docs/`:

```bash
cargo install mdbook
mdbook serve docs --open       # live preview
mdbook build docs              # static output in docs/book
```

The site is published to GitHub Pages automatically by
`.github/workflows/docs.yml` on every push to `main` that touches `docs/**`,
`README.md`, or the workflow itself.

## Contribution expectations

- Keep PRs focused and small.
- Add tests for behavior changes.
- Update docs for user-facing changes.
- Follow formatter and lint requirements before opening a PR.

## CI baseline

The repository workflows validate:

- Formatting (`cargo fmt`)
- Clippy warnings as errors
- Unit and integration tests
- Dependency audits
- CodeQL analysis
- GitHub Pages documentation build
