# Usage Guide

## Basic execution

```bash
tokensaver <command> [args...]
```

Example:

```bash
tokensaver cargo test
```

## Raw mode

Bypass summarization and print command output unchanged.

```bash
tokensaver --raw cargo test
```

## Extreme mode

Apply aggressive compression for unknown commands.

```bash
tokensaver --extreme some-chatty-command
```

## Check version and self-update

Print the installed version:

```bash
tokensaver version
```

Check for and install the latest release. TokenSaver downloads the prebuilt
archive for your platform from GitHub Releases, verifies its SHA-256 checksum,
and replaces the running binary (and the `ts` alias) in place:

```bash
tokensaver update            # check and install if newer
tokensaver update --check    # report only, do not install
tokensaver update --force    # reinstall even if already up to date
```

The updater uses the system `curl`/`wget` or PowerShell, so no extra runtime
dependency is required. It writes to the binary's current location, so make sure
you have permission to that directory (or re-run your original install method).

## Gallery / marketplace

`tokensaver gallery` keeps your **user-defined** Copilot context objects in a
local marketplace at `~/.tokensaver/gallery`. It harvests the agents, skills,
prompts and custom instructions you authored out of `~/.copilot`, `~/.agents`,
the VS Code `User/prompts` folder and home-level `AGENTS.md` /
`copilot-instructions.md`, so they live in one place and can be reinstalled into
any workspace. Objects supplied by VS Code extensions or the installed app are
never harvested.

Harvesting **moves** objects, so it is a dry run by default and only changes
anything when you pass `--apply`:

```bash
tokensaver gallery harvest          # dry run: list what would be moved
tokensaver gallery harvest --apply  # move user objects into the gallery
tokensaver gallery list             # list stored items
tokensaver gallery show <id>        # details + content preview
tokensaver gallery install <id> --dir ./my-project
tokensaver gallery remove <id>      # delete an item from the gallery
tokensaver gallery serve --open     # browse + install from a local web UI
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
