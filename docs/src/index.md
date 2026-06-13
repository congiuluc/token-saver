# token-saver

token-saver is a deterministic CLI proxy that executes a command and emits a
compact summary of its output.

## Why token-saver

- Reduces noisy terminal output for humans and tooling.
- Preserves important signals like failures and warnings.
- Propagates original command exit codes for script safety.
- Works offline by default.

## Quick start

Install a prebuilt binary (Linux/macOS shown; see [Installation](installation.md)
for Windows, Cargo, and from-source options):

```sh
curl -fsSL https://raw.githubusercontent.com/congiuluc/token-saver/main/install.sh | sh
token-saver git status
```

Or build from source:

```sh
cargo build --release
./target/release/token-saver git status
```

token-saver runs on Windows, Linux, and macOS (x86_64 and arm64).

## Documentation map

- Installation: cross-platform install methods and prebuilt binaries.
- Architecture: internal execution pipeline and module responsibilities.
- Command formatters: command-specific summarization behavior.
- Usage guide: daily commands and practical examples.
- Development guide: quality checks, tests, and contribution workflow.
