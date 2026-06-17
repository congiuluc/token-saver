# Architecture

token-saver follows a simple, deterministic flow:

1. Parse CLI arguments and dispatch the subcommand.
2. Optionally rewrite the command invocation to a machine-readable variant
   (e.g. `git status` → `git status --porcelain=v1 --branch`).
3. Execute the child process and collect stdout, stderr, and the exit code.
4. Route the output to a command-specific formatter or the generic compressor,
   keyed off the *original* command the user typed.
5. Emit the compact summary, record metrics (and optional OpenTelemetry spans),
   and return the original exit code.

## Binaries

Two binaries share a single library crate, so behavior is identical:

- `token-saver` — full command name (`src/main.rs`).
- `tks` — short alias (`src/bin/tks.rs`).

Both call `token_saver::run()` in `src/lib.rs`.

## Core modules

| Module                  | Responsibility                                                       |
| ----------------------- | ------------------------------------------------------------------- |
| `src/main.rs`           | `token-saver` binary entrypoint.                                    |
| `src/bin/tks.rs`        | `tks` alias entrypoint.                                             |
| `src/lib.rs`            | CLI argument parsing and subcommand dispatch.                       |
| `src/runner.rs`         | Child-process execution and output capture.                         |
| `src/format/mod.rs`     | Formatter routing and command rewrite logic.                        |
| `src/format/*.rs`       | Per-tool formatters (git, cargo, dotnet, node, py, container, …).   |
| `src/format/generic.rs` | Fallback compressor for unrecognized commands.                      |
| `src/tokenizer.rs`      | Pluggable token-counting backends (BPE and heuristic).              |
| `src/metrics.rs`        | Token and gain accounting; JSONL metrics log.                       |
| `src/otel.rs`           | Optional, opt-in OpenTelemetry (OTLP) span export.                  |
| `src/optimize.rs`       | `optimize` command: file text compaction.                           |
| `src/assess.rs`         | `context` command: Copilot context inventory and cost.              |
| `src/gallery.rs`        | `gallery` command: local marketplace + web UI.                      |
| `src/init.rs`           | `init`/`uninit`: Copilot instructions, agent, and hook setup.       |
| `src/hook.rs`           | `hook` command: Copilot `postToolUse` runtime hook.                 |
| `src/update.rs`         | `update` command: self-update from GitHub Releases.                 |
| `src/banner.rs`         | `banner` command: animated ASCII-art splash.                        |

## Design principles

- **Deterministic** output transformations — no AI, fully reproducible.
- **Behavior parity** with the original command, including exit codes.
- **Minimal dependencies** and offline-by-default operation.
- **Safe defaults** with explicit opt-in for telemetry and metrics logging.
- **Failure isolation**: metrics and telemetry errors never affect the primary
  command's output or exit code.
