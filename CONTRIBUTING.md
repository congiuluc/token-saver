# Contributing to token-saver

Thanks for your interest in improving **token-saver**! This document explains how
to set up your environment, the conventions the project follows, and how to get
a change merged.

## Code of Conduct

This project adheres to the [Contributor Covenant](CODE_OF_CONDUCT.md). By
participating you are expected to uphold it. Please report unacceptable behavior
as described in that document.

## Getting started

1. Install the [Rust toolchain](https://rustup.rs/) (stable). `cargo`, `rustfmt`,
   and `clippy` must be available:

   ```bash
   rustup component add rustfmt clippy
   ```

2. Fork and clone the repository:

   ```bash
   git clone https://github.com/<your-user>/token-saver.git
   cd token-saver
   ```

3. Build and run the test suite:

   ```bash
   cargo build
   cargo test
   ```

> `token-saver` has a single build-time dependency (`tiktoken-rs`) and **zero
> runtime dependencies**. Please keep it that way — new dependencies need a clear
> justification in the pull request.

## Development workflow

The project is a small Rust CLI with two binaries (`token-saver` and `ts`) that
both delegate to `token_saver::run()` in [`src/lib.rs`](src/lib.rs). Most work
falls into one of these areas:

- **A new per-command formatter** → add a module under [`src/format/`](src/format)
  and route it from [`src/format/mod.rs`](src/format/mod.rs).
- **A new subcommand** → add a module, register it in `src/lib.rs` (module
  declaration + dispatch arm + usage text).
- **Tokenizer / accounting changes** → see [`src/tokenizer.rs`](src/tokenizer.rs)
  and [`src/metrics.rs`](src/metrics.rs).

See the [architecture documentation](docs/src/architecture.md) for a full tour.

## Before you open a pull request

Run the same checks CI runs. All three must pass:

```bash
cargo fmt --all -- --check        # formatting
cargo clippy --all-targets --all-features -- -D warnings   # lints
cargo test --all                  # unit + integration tests
```

To auto-fix formatting and many lints:

```bash
cargo fmt --all
cargo clippy --fix --allow-dirty --all-targets --all-features
```

### Conventions

- **Formatting** is enforced by `rustfmt` (default config). Do not hand-format.
- **Lints**: clippy runs with `-D warnings`; the tree must be warning-free.
- **Doc comments** (`///`) on public functions and modules, explaining intent.
- **Status messages** go to **stderr** with a `token-saver:` prefix; user-facing
  results (summaries, JSON) go to **stdout** so they stay pipeable and testable.
- **Tests**: add unit tests next to the code (`#[cfg(test)] mod tests`) and
  end-to-end tests in [`tests/cli.rs`](tests/cli.rs). Run CLI tests serially
  (`cargo test -- --test-threads=1`) if you hit load-related flakiness.
- **No network at runtime** outside the opt-in OpenTelemetry exporter, and the
  child process must always see exactly the command the user typed.

## Commit messages & pull requests

- Write clear, imperative commit subjects (e.g. "Add poetry install formatter").
- Keep pull requests focused; unrelated changes belong in separate PRs.
- Describe **what** changed and **why**, and note any user-visible behavior
  changes. Update the [README](README.md), the [docs](docs/), and
  [CHANGELOG.md](CHANGELOG.md) when behavior changes.
- Link any related issues.

## Reporting bugs & requesting features

Use the [issue templates](https://github.com/congiuluc/token-saver/issues/new/choose).
For security issues, follow [SECURITY.md](SECURITY.md) instead of opening a
public issue.

## License

By contributing, you agree that your contributions will be licensed under the
[MIT License](LICENSE).
