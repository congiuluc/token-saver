# token-saver

token-saver is a deterministic CLI proxy that executes a command and emits a
compact summary of its output. No AI is involved in the summarization — every
transformation is rule-based and reproducible, so the same input always yields
the same output.

It ships as two binaries that share one implementation:

- `token-saver` — the full command.
- `tks` — a short alias, identical in behavior. Use whichever you prefer; the
  docs use `token-saver` for clarity and `tks` in agent/automation contexts.

## Why token-saver

- Reduces noisy terminal output for humans and AI tooling.
- Preserves important signals like failures and warnings.
- Propagates original command exit codes for script safety.
- Works offline by default — no network calls during summarization.
- Optional, opt-in metrics and OpenTelemetry export for measuring savings.

## Feature highlights

- **Command summarization** with dedicated formatters for git, cargo, dotnet,
  npm/yarn/pnpm, docker, kubectl, pytest, and more (see
  [Command Formatters](formatters.md)).
- **Extreme mode** (`-x`) for maximally compressed output.
- **Raw passthrough** (`--raw`) when you need the unmodified output.
- **Stdin filter** to compress arbitrary text or logs before pasting into a prompt.
- **Token accounting** (`gain`, `tokens`) using near-real BPE tokenizers.
- **Copilot integration** (`init`, `hook`, `context`, `gallery`) to wire
  token-saver into GitHub Copilot and manage your context objects (see
  [Copilot Integration](copilot.md)).
- **Self-update** (`update`) straight from GitHub Releases.

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

- [Installation](installation.md): cross-platform install methods and prebuilt binaries.
- [Usage Guide](usage.md): daily commands and practical examples.
- [Command Reference](commands.md): every subcommand, flag, and exit code.
- [Command Formatters](formatters.md): command-specific summarization behavior.
- [Copilot Integration](copilot.md): `init`, `hook`, `context`, and the `gallery`.
- [Configuration](configuration.md): environment variables and tuning.
- [Architecture](architecture.md): internal execution pipeline and module responsibilities.
- [Development Guide](development.md): quality checks, tests, and contribution workflow.
