# TokenSaver

TokenSaver is a deterministic CLI proxy that executes a command and emits a
compact summary of its output.

## Why TokenSaver

- Reduces noisy terminal output for humans and tooling.
- Preserves important signals like failures and warnings.
- Propagates original command exit codes for script safety.
- Works offline by default.

## Quick start

```bash
cargo build --release
./target/release/tokensaver git status
```

## Documentation map

- Architecture: internal execution pipeline and module responsibilities.
- Command formatters: command-specific summarization behavior.
- Usage guide: daily commands and practical examples.
- Development guide: quality checks, tests, and contribution workflow.
