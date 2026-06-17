# Usage Guide

This guide covers everyday command-line use. For the exhaustive list of
subcommands and flags see the [Command Reference](commands.md); for the
Copilot-specific commands (`init`, `hook`, `context`, `gallery`) see
[Copilot Integration](copilot.md).

> All examples use `token-saver`. The shorter `tks` alias works identically:
> `tks cargo test` == `token-saver cargo test`.

## Basic execution

Prefix any command with `token-saver` (or `tks`) to run it and print a compact
summary of its output. The original exit code is preserved.

```bash
token-saver <command> [args...]
```

Example:

```bash
token-saver cargo test
tks git status
```

token-saver rewrites a few commands to a machine-readable variant internally for
reliable parsing (for example `git status` becomes
`git status --porcelain=v1 --branch`), then summarizes against the command you
actually typed.

## Raw mode

Bypass summarization and print command output unchanged, while still recording
metrics:

```bash
token-saver --raw cargo test
```

## Extreme mode

Apply maximally aggressive compression. In extreme mode the output is reduced to
errors plus a one-line stats footer:

```bash
token-saver -x cargo test
token-saver --extreme some-chatty-command
```

## Stdin filter

Compress arbitrary text or a log file instead of running a command. Useful for
shrinking a blob before pasting it into a prompt:

```bash
# Linux / macOS
cat big.log | token-saver -

# Windows (PowerShell)
Get-Content big.log | token-saver -
```

Add `-x` for extreme compression of the piped text:

```bash
cat big.log | token-saver -x -
```

## Count tokens and words

`tokens` reports character, byte, word, and per-line counts for a prompt, a
file, or stdin:

```bash
token-saver tokens --prompt "hello world"
token-saver tokens --file ./notes.md
echo "some text" | token-saver tokens --stdin
```

## Compact a file

`optimize` rewrites a file's text into a more compact form and reports the token
savings. Use `--preview` to print the compacted text alongside the token diff:

```bash
token-saver optimize --file ./long-notes.md
token-saver optimize --file ./long-notes.md --preview
```

## Show token savings

`gain` aggregates the savings recorded in the metrics log and prints them using
the active tokenizer (see [Configuration](configuration.md)):

```bash
token-saver gain            # show cumulative savings
token-saver gain --reset    # clear the recorded stats
```

Metrics logging is opt-in/​configurable via `TOKEN_SAVER_LOG`; if it is disabled
there is nothing to aggregate.

## Banner

Print the animated ASCII-art banner (honors `NO_COLOR`):

```bash
token-saver banner
token-saver banner --no-anim   # static, no animation
```

## Check version and self-update

Print the installed version:

```bash
token-saver version
token-saver --version
```

Check for and install the latest release. token-saver downloads the prebuilt
archive for your platform from GitHub Releases, verifies its SHA-256 checksum,
and replaces the running binary (and the `tks` alias) in place:

```bash
token-saver update            # check and install if newer
token-saver update --check    # report only, do not install
token-saver update --force    # reinstall even if already up to date
```

The updater uses the system `curl`/`wget` or PowerShell, so no extra runtime
dependency is required. It writes to the binary's current location, so make sure
you have permission to that directory (or re-run your original install method).

## Standard quality loop

When developing token-saver itself:

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
