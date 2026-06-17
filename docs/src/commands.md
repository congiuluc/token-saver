# Command Reference

This page lists every token-saver subcommand and flag. The `tks` alias accepts
the exact same arguments as `token-saver`.

## Synopsis

```text
token-saver <command> [args...]        Run the command and print a compact summary
token-saver -x | --extreme <command>   Run and print a maximally compressed summary
token-saver --raw <command> ...        Run the command and print its raw output unchanged
token-saver - | --stdin                Read stdin and print its compact form
token-saver init [--global|--cli]      Register token-saver with GitHub Copilot (+ agent)
token-saver init --hook [--global]     Install a Copilot postToolUse hook
token-saver uninit [--global|--cli]    Remove what `token-saver init` configured
token-saver uninit --hook [--global]   Remove the Copilot postToolUse hook
token-saver hook                       Run as a Copilot postToolUse hook (reads stdin)
token-saver gain [--reset]             Show or reset logged token savings
token-saver tokens <source>            Count tokens/words for prompt, file, or stdin
token-saver optimize --file <path>     Compact a file's text and report token savings
token-saver context [category]         Inventory Copilot context objects and token cost
token-saver gallery <command>          Harvest, list, install, or serve a context gallery
token-saver banner [--no-anim]         Show the animated ASCII-art banner
token-saver update [--check|--force]   Update token-saver to the latest release
token-saver version | --version | -V   Print the installed version
token-saver -h | --help                Show usage
```

## Summarization

### Default run

```text
token-saver <command> [args...]
```

Runs the command, routes its output through a command-specific formatter (or the
generic compressor), prints the summary, records metrics, and returns the
original exit code.

### `-x`, `--extreme`

Tighten compression to errors plus a one-line stats footer. Can also be combined
with the stdin filter (`token-saver -x -`).

### `--raw`

Run the command and print its output unchanged (no summarization). Metrics are
still recorded.

### `-`, `--stdin`

Read all of stdin and print its compact form instead of running a command.

## Token accounting

### `gain`

| Flag      | Description                          |
| --------- | ------------------------------------ |
| (none)    | Print cumulative recorded savings.   |
| `--reset` | Clear the recorded savings log.      |

### `tokens`

Exactly one source is required:

| Flag                 | Description                         |
| -------------------- | ----------------------------------- |
| `--prompt`, `-p` `<text>` | Count tokens/words for inline text. |
| `--file`, `-f` `<path>`   | Count tokens/words for a file.      |
| `--stdin`, `-`            | Count tokens/words read from stdin. |

### `optimize` (alias `opt`)

| Flag             | Description                                         |
| ---------------- | --------------------------------------------------- |
| `--file <path>`  | File whose text is compacted (required).            |
| `--preview`      | Print the compacted text alongside the token diff.  |

## Copilot integration

These are documented in detail in [Copilot Integration](copilot.md).

### `init` / `uninit`

| Flag             | Description                                                      |
| ---------------- | --------------------------------------------------------------- |
| (none)           | Workspace scope: write `.github` instructions + agent.          |
| `--global`, `-g` | Apply to the global Copilot config in your home directory.      |
| `--cli`, `--agents` | Target the CLI/agents scope.                                 |
| `--hook`, `--hooks` | Install/remove the Copilot `postToolUse` hook instead.       |

### `hook`

Runs token-saver as a Copilot `postToolUse` hook, reading the tool output from
stdin and printing a compacted form. Accepts `-x` / `--extreme`.

### `context` (aliases `ctx`, `assess`, `assessment`)

Inventories your Copilot context objects and reports their token cost. Accepts an
optional category argument to narrow the inventory.

### `gallery` (aliases `market`, `marketplace`)

| Subcommand                         | Description                                  |
| ---------------------------------- | -------------------------------------------- |
| `harvest [--apply]`                | Dry run (default) or move user objects in.   |
| `list`                             | List stored items.                           |
| `show <id>`                        | Show item details and a content preview.     |
| `install <id> --dir <path>`        | Install an item into a workspace.            |
| `remove <id>`                      | Delete an item from the gallery.             |
| `serve [--open]`                   | Browse and install via a local web UI.       |

## Maintenance

### `update` (aliases `upgrade`, `self-update`)

| Flag      | Description                                  |
| --------- | -------------------------------------------- |
| (none)    | Check and install if a newer release exists. |
| `--check` | Report only; do not install.                 |
| `--force` | Reinstall even if already up to date.        |

### `banner` (aliases `splash`, `logo`)

| Flag        | Description                  |
| ----------- | ---------------------------- |
| `--no-anim` | Print the banner statically. |

### `version`, `--version`, `-V`

Print the installed version.

### `-h`, `--help`

Print usage.

## Exit codes

- A run returns the **child command's** exit code (clamped to 0–255).
- Usage errors (missing/unknown arguments) return `2`.
- token-saver's own operational failures (such as a failed `init` write) return `1`.
- With no arguments, token-saver prints usage and exits `2`.
