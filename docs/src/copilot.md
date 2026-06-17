# Copilot Integration

token-saver integrates with GitHub Copilot to keep tool output — and your
context objects — small. There are three pieces: the instruction/agent
installer (`init`), the runtime hook (`hook`), and the context tooling
(`context` and `gallery`).

## Register with Copilot (`init`)

`token-saver init` writes a managed token-saver instruction block telling Copilot
to prefix shell commands with `tks`, and installs a minimal **token-saver agent**
that declares only built-in tools.

```bash
token-saver init                 # workspace: .github/ in the current repo
token-saver init --global        # your home-level Copilot config
token-saver init --cli           # CLI/agents scope
```

After running `init`, select the **token-saver** agent in Copilot Chat to use a
minimal, token-efficient tool surface.

Undo any of these with `uninit`:

```bash
token-saver uninit               # remove workspace instructions + agent
token-saver uninit --global      # remove the global configuration
```

## postToolUse hook (`hook`)

The hook compresses tool output automatically, without changing how you invoke
commands. Install it once:

```bash
token-saver init --hook           # workspace hook
token-saver init --hook --global  # global hook
```

This registers `token-saver hook` as a Copilot `postToolUse` command. The hook
reads the tool output from stdin and prints a compacted form; add `-x` behavior
through the extreme flag if your hook configuration passes it. Remove it with:

```bash
token-saver uninit --hook
token-saver uninit --hook --global
```

## Inventory context cost (`context`)

`token-saver context` (aliases `ctx`, `assess`) inventories your Copilot context
objects — agents, skills, prompts, and instructions — and reports their token
cost using the active tokenizer. Pass an optional category to narrow the scope:

```bash
token-saver context              # full inventory
token-saver context instructions # only instruction files
```

Use it to find oversized or always-on context that is inflating every request.

## Context gallery (`gallery`)

`token-saver gallery` keeps your **user-defined** Copilot context objects in a
local marketplace at `~/.token-saver/gallery`. It harvests the agents, skills,
prompts, and custom instructions you authored out of `~/.copilot`, `~/.agents`,
the VS Code `User/prompts` folder, and home-level `AGENTS.md` /
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
