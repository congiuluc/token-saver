# Usage Guide

## Basic execution

```bash
token-saver <command> [args...]
```

Example:

```bash
token-saver cargo test
```

## Raw mode

Bypass summarization and print command output unchanged.

```bash
token-saver --raw cargo test
```

## Extreme mode

Apply aggressive compression for unknown commands.

```bash
token-saver --extreme some-chatty-command
```

## Check version and self-update

Print the installed version:

```bash
token-saver version
```

Check for and install the latest release. token-saver downloads the prebuilt
archive for your platform from GitHub Releases, verifies its SHA-256 checksum,
and replaces the running binary (and the `ts` alias) in place:

```bash
token-saver update            # check and install if newer
token-saver update --check    # report only, do not install
token-saver update --force    # reinstall even if already up to date
```

The updater uses the system `curl`/`wget` or PowerShell, so no extra runtime
dependency is required. It writes to the binary's current location, so make sure
you have permission to that directory (or re-run your original install method).

## Gallery / marketplace

`token-saver gallery` keeps your **user-defined** Copilot context objects in a
local marketplace at `~/.token-saver/gallery`. It harvests the agents, skills,
prompts and custom instructions you authored out of `~/.copilot`, `~/.agents`,
the VS Code `User/prompts` folder and home-level `AGENTS.md` /
`copilot-instructions.md`, so they live in one place and can be reinstalled into
any workspace. Objects supplied by VS Code extensions or the installed app are
never harvested.

Harvesting **moves** objects, so it is a dry run by default and only changes
anything when you pass `--apply`:

```bash
token-saver gallery harvest          # dry run: list what would be moved
token-saver gallery harvest --apply  # move user objects into the gallery
token-saver gallery list             # list stored items
token-saver gallery show <id>        # details + content preview
token-saver gallery install <id> --dir ./my-project
token-saver gallery remove <id>      # delete an item from the gallery
token-saver gallery serve --open     # browse + install from a local web UI
```

Installs land at standard Copilot paths: `.github/` for instructions and
prompts, `.github/skills/` for skills, `.github/agents/` (or `.github/chatmodes/`)
for agents, `.vscode/mcp.json` for tool configs, and a merge into the workspace
`AGENTS.md` / `copilot-instructions.md`. `gallery serve` runs a dependency-free
web server bound to `127.0.0.1` only.

## Standard quality loop

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
