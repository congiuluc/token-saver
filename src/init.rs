//! `token-saver init` — register `token-saver` with GitHub Copilot by writing a managed
//! instructions block into a `copilot-instructions.md` file.
//!
//! GitHub Copilot automatically prepends the contents of `copilot-instructions.md`
//! to every request, so a short rule telling the agent to prefix shell commands
//! with `token-saver` is enough to route tool/prompt commands through token-saver.
//! The block is delimited by HTML comment markers so re-running `init` updates the
//! block in place instead of duplicating it, leaving the rest of the file intact.

use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;

/// Opening marker for the managed instructions block.
const BEGIN_MARKER: &str = "<!-- token-saver-instructions v1 -->";
/// Closing marker for the managed instructions block.
const END_MARKER: &str = "<!-- /token-saver-instructions -->";

/// Filename for the generated Copilot hook configuration.
const HOOK_FILE: &str = "token-saver.json";

/// Filename for the generated token-saver custom agent.
const AGENT_FILE: &str = "token-saver.agent.md";

/// Where the Copilot instructions file should be written.
#[derive(Clone, Copy)]
pub enum Scope {
    /// `<cwd>/.github/copilot-instructions.md` — applies to the current repo.
    Workspace,
    /// `<home>/.copilot/copilot-instructions.md` — applies to every workspace.
    Global,
    /// `<cwd>/AGENTS.md` — the agent-file format read by Copilot CLI and other agents.
    Agents,
}

/// Writes (or refreshes) the managed `token-saver` instructions block for `scope` and
/// returns the path of the file that was written.
pub fn run(scope: Scope) -> io::Result<PathBuf> {
    let path = target_path(scope)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let merged = merge(&existing, &block());
    fs::write(&path, merged)?;
    Ok(path)
}

/// Splices the managed `block` into `existing` content.
///
/// If a previous block (between the markers) is present it is replaced in place;
/// otherwise the block is appended. Surrounding user content is preserved.
pub fn merge(existing: &str, block: &str) -> String {
    if let (Some(start), Some(end)) = (existing.find(BEGIN_MARKER), existing.find(END_MARKER)) {
        let end = end + END_MARKER.len();
        let mut out = String::with_capacity(existing.len());
        out.push_str(&existing[..start]);
        out.push_str(block);
        out.push_str(&existing[end..]);
        return out;
    }

    if existing.trim().is_empty() {
        return format!("{block}\n");
    }

    let mut out = existing.trim_end().to_string();
    out.push_str("\n\n");
    out.push_str(block);
    out.push('\n');
    out
}

/// Removes the managed `token-saver` instructions block for `scope` and returns the
/// path it was removed from, or `None` if no managed block was present.
///
/// Only the block between the markers is removed; surrounding user content is
/// preserved. If stripping the block leaves the file empty, the file is deleted.
pub fn uninstall(scope: Scope) -> io::Result<Option<PathBuf>> {
    let path = target_path(scope)?;
    let existing = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(ref err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let Some(stripped) = strip(&existing) else {
        return Ok(None);
    };
    if stripped.trim().is_empty() {
        fs::remove_file(&path)?;
    } else {
        fs::write(&path, stripped)?;
    }
    Ok(Some(path))
}

/// Removes the managed `block` (between the markers) from `existing`.
///
/// Returns `None` when no block is present; otherwise returns the remaining
/// content with surrounding blank lines collapsed. Inverse of [`merge`].
pub fn strip(existing: &str) -> Option<String> {
    let start = existing.find(BEGIN_MARKER)?;
    let end = existing.find(END_MARKER)? + END_MARKER.len();
    let before = existing[..start].trim_end();
    let after = existing[end..].trim_start();
    let out = match (before.is_empty(), after.is_empty()) {
        (true, true) => String::new(),
        (true, false) => format!("{after}\n"),
        (false, true) => format!("{before}\n"),
        (false, false) => format!("{before}\n\n{after}\n"),
    };
    Some(out)
}

/// Writes (or refreshes) the `token-saver` custom agent for `scope` and returns the
/// path of the file that was written.
///
/// The agent declares only built-in tools, so it carries a minimal tool surface
/// (and therefore a minimal token cost) while still instructing the model to route
/// shell commands through `tks`.
pub fn run_agent(scope: Scope) -> io::Result<PathBuf> {
    let path = agent_path(scope)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, agent_block())?;
    Ok(path)
}

/// Removes the `token-saver` custom agent for `scope` and returns the path removed,
/// or `None` if no agent file was present.
pub fn uninstall_agent(scope: Scope) -> io::Result<Option<PathBuf>> {
    let path = agent_path(scope)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(Some(path)),
        Err(ref err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Resolves the custom-agent file path for the requested scope.
///
/// Global agents live under `~/.copilot/agents/`; workspace and CLI/agents scopes
/// share the repository's `.github/agents/` directory.
fn agent_path(scope: Scope) -> io::Result<PathBuf> {
    match scope {
        Scope::Global => Ok(home_dir()?.join(".copilot").join("agents").join(AGENT_FILE)),
        Scope::Workspace | Scope::Agents => {
            Ok(env::current_dir()?.join(".github").join("agents").join(AGENT_FILE))
        }
    }
}

/// Builds the `token-saver` custom-agent document: front matter declaring only
/// built-in tools, plus a caveman-style body that demands terse chat output and a
/// bias toward writing code/files over prose.
fn agent_block() -> String {
    "---\n\
     name: token-saver\n\
     description: Token-optimized agent. Talk caveman-short in chat, do the work in code/files, route shell commands through `tks`. Built-in tools only.\n\
     tools: [vscode, execute, read, agent, edit, search, web, browser, todo]\n\
     ---\n\
     # token-saver agent\n\
     \n\
     Why use many token when few do trick. Brain big, mouth small.\n\
     \n\
     ## Output rules\n\
     \n\
     - Chat output as short as possible. Fragments over sentences. Drop filler.\n\
     - No preamble, no recap, no \"I will\", no closing summary. Say what changed, where.\n\
     - One short line per file touched: `path — what`. That enough.\n\
     - Do not restate code already written to a file. The file is the answer.\n\
     - Explain only when asked, or one-line warning for a real risk.\n\
     - Keep code, paths, commands, error strings byte-exact. Compress prose, not code.\n\
     \n\
     ## Do the work\n\
     \n\
     - Put work in code and files, not in chat. Create/edit files directly.\n\
     - Default to acting. Ask only when truly blocked.\n\
     \n\
     ## Tools\n\
     \n\
     - Built-in tools above only. No MCP or extension tools.\n\
     - Always prefix shell commands with `tks` so output is summarized:\n\
     \n\
     ```bash\n\
     # Instead of:        Use:\n\
     git status           tks git status\n\
     git log              tks git log\n\
     cargo test           tks cargo test\n\
     docker ps            tks docker ps\n\
     npm install          tks npm install\n\
     ```\n\
     \n\
     - `tks -x <command>`    Extreme: errors + one-line stats footer only.\n\
     - `tks --raw <command>` Bypass summarization, print raw output.\n"
        .to_string()
}

/// Deletes the `token-saver` `postToolUse` hook configuration for the requested scope
/// and returns the path removed, or `None` if no hook config existed.
pub fn uninstall_hook(global: bool) -> io::Result<Option<PathBuf>> {
    let path = hook_path(global)?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(Some(path)),
        Err(ref err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

/// Writes (or overwrites) the `token-saver` `postToolUse` hook configuration and
/// returns the path written.
///
/// With `global` the config is written to the user-level hooks directory
/// (`~/.copilot/hooks/token-saver.json`), applying to every workspace; otherwise it is
/// written to the repository's `.github/hooks/token-saver.json`, applying to the
/// current repo only.
pub fn run_hook(global: bool) -> io::Result<PathBuf> {
    let path = hook_path(global)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, hook_config())?;
    Ok(path)
}

/// Resolves the hook configuration file path for the requested scope.
fn hook_path(global: bool) -> io::Result<PathBuf> {
    if global {
        Ok(home_dir()?.join(".copilot").join("hooks").join(HOOK_FILE))
    } else {
        Ok(env::current_dir()?.join(".github").join("hooks").join(HOOK_FILE))
    }
}

/// Builds the `postToolUse` hook configuration document.
///
/// `postToolUse` fires after every tool and does not support a `matcher`, so the
/// config simply registers `token-saver hook`; the adapter itself filters to shell
/// tools and only rewrites a result when compression shrinks it.
fn hook_config() -> String {
    r#"{
  "version": 1,
  "hooks": {
    "postToolUse": [
      {
        "type": "command",
        "command": "token-saver hook",
        "timeoutSec": 10
      }
    ]
  }
}
"#
    .to_string()
}

/// Resolves the instructions file path for the requested scope.
fn target_path(scope: Scope) -> io::Result<PathBuf> {
    match scope {
        Scope::Workspace => Ok(env::current_dir()?.join(".github").join("copilot-instructions.md")),
        Scope::Global => Ok(home_dir()?.join(".copilot").join("copilot-instructions.md")),
        Scope::Agents => Ok(env::current_dir()?.join("AGENTS.md")),
    }
}

/// Returns the user's home directory, honoring `USERPROFILE` then `HOME`.
fn home_dir() -> io::Result<PathBuf> {
    env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")).map(PathBuf::from).ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "could not determine home directory (set USERPROFILE or HOME)")
    })
}

/// Builds the managed instructions block (markers included, no trailing newline).
fn block() -> String {
    format!(
        "{BEGIN_MARKER}\n\
         # token-saver — Token-Optimized Command Output\n\
         \n\
         **token-saver** runs a command and prints an extremely compact summary of\n\
         its output, cutting token usage on noisy commands.\n\
         \n\
         ## Rule\n\
         \n\
         Always prefix shell commands with `tks`:\n\
         \n\
         ```bash\n\
         # Instead of:        Use:\n\
         git status           tks git status\n\
         git log              tks git log\n\
         cargo test           tks cargo test\n\
         docker ps            tks docker ps\n\
         kubectl get pods     tks kubectl get pods\n\
         npm install          tks npm install\n\
         ```\n\
         \n\
         ## Modes\n\
         \n\
         - `tks -x <command>`    Extreme mode: errors + a one-line stats footer only.\n\
         - `tks --raw <command>` Bypass summarization and print raw output.\n\
         {END_MARKER}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_into_empty_creates_block() {
        let out = merge("", &block());
        assert!(out.contains(BEGIN_MARKER));
        assert!(out.contains(END_MARKER));
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn merge_appends_to_existing_content() {
        let out = merge("# Project notes\n", &block());
        assert!(out.starts_with("# Project notes"));
        assert!(out.contains(BEGIN_MARKER));
    }

    #[test]
    fn merge_replaces_existing_block_in_place() {
        let stale = format!("# Top\n\n{BEGIN_MARKER}\nold rules\n{END_MARKER}\n\n# Bottom\n");
        let out = merge(&stale, &block());
        // Only one managed block remains.
        assert_eq!(out.matches(BEGIN_MARKER).count(), 1);
        // Surrounding user content is preserved.
        assert!(out.contains("# Top"));
        assert!(out.contains("# Bottom"));
        // The stale body is gone, replaced with the current rule.
        assert!(!out.contains("old rules"));
        assert!(out.contains("Always prefix shell commands"));
    }

    #[test]
    fn hook_config_registers_post_tool_use_command() {
        let cfg = hook_config();
        assert!(cfg.contains("\"version\": 1"));
        assert!(cfg.contains("\"postToolUse\""));
        assert!(cfg.contains("\"command\": \"token-saver hook\""));
        assert!(cfg.ends_with('\n'));
    }

    #[test]
    fn agent_block_declares_builtin_tools_and_ts_rule() {
        let agent = agent_block();
        assert!(agent.starts_with("---\n"));
        assert!(agent.contains("name: token-saver"));
        assert!(agent.contains("tools: ["));
        assert!(agent.contains("execute"));
        assert!(agent.contains("Always prefix shell commands with `tks`"));
        assert!(agent.ends_with('\n'));
    }

    #[test]
    fn strip_returns_none_without_block() {
        assert_eq!(strip("# Just notes\n"), None);
    }

    #[test]
    fn strip_removes_block_and_preserves_surrounding_content() {
        let merged = merge("# Top\n", &block());
        let stripped = strip(&merged).expect("block present");
        assert!(!stripped.contains(BEGIN_MARKER));
        assert!(!stripped.contains(END_MARKER));
        assert!(stripped.contains("# Top"));
    }

    #[test]
    fn strip_empties_file_that_held_only_the_block() {
        let only = merge("", &block());
        let stripped = strip(&only).expect("block present");
        assert!(stripped.trim().is_empty());
    }
}
