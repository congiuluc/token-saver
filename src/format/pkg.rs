//! Package-manager formatters for pip, Poetry, and the JavaScript installers
//! (Yarn, pnpm, Bun) that don't share npm's exact output shape.

use crate::format::generic;
use crate::runner::Outcome;

/// Summarizes a `pip install`/`uninstall` run, keeping the "Successfully …"
/// lines and any errors while discarding the verbose collecting/downloading
/// progress and "Requirement already satisfied" noise.
pub fn pip(out: &Outcome) -> String {
    let text = combined(out);
    let mut keep: Vec<String> = Vec::new();

    for line in text.lines() {
        let t = line.trim();
        let want = t.starts_with("Successfully installed")
            || t.starts_with("Successfully uninstalled")
            || t.starts_with("Successfully built")
            || t.starts_with("ERROR")
            || t.contains("error:")
            || t.starts_with("Would install");
        if want && !t.is_empty() {
            keep.push(t.to_string());
        }
    }

    if keep.is_empty() {
        return generic::summarize(out);
    }

    keep.dedup();
    let mut body = keep.join("\n");
    if out.code != 0 {
        body.push_str(&format!("\n! exit {}", out.code));
    }
    body
}

/// Summarizes a Poetry install/update from the "Package operations:" summary
/// line (or a count of the bullet actions), surfacing errors on failure.
pub fn poetry(out: &Outcome) -> String {
    let text = combined(out);
    let mut summary: Option<String> = None;
    let (mut installs, mut updates, mut removes) = (0usize, 0usize, 0usize);
    let mut errors: Vec<String> = Vec::new();

    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("Package operations:") {
            summary = Some(rest.trim().to_string());
        }
        if let Some(action) = t.strip_prefix('•') {
            let a = action.trim();
            if a.starts_with("Installing") {
                installs += 1;
            } else if a.starts_with("Updating") {
                updates += 1;
            } else if a.starts_with("Removing") {
                removes += 1;
            }
        }
        if t.to_ascii_lowercase().contains("error") {
            errors.push(t.to_string());
        }
    }

    if out.code != 0 || !errors.is_empty() {
        if errors.is_empty() {
            return generic::summarize(out);
        }
        errors.dedup();
        let mut lines = vec![format!("✗ poetry: {} error(s)", errors.len())];
        for e in errors.iter().take(6) {
            lines.push(format!("  {e}"));
        }
        if errors.len() > 6 {
            lines.push(format!("  … {} more", errors.len() - 6));
        }
        return lines.join("\n");
    }

    let detail = summary.unwrap_or_else(|| format!("{installs} installed, {updates} updated, {removes} removed"));
    format!("✓ poetry: {detail}")
}

/// Summarizes a Yarn/pnpm/Bun install by keeping the result/summary lines and
/// discarding resolution progress.
pub fn js_install(out: &Outcome, tool: &str) -> String {
    let text = combined(out);
    let mut keep: Vec<String> = Vec::new();

    for line in text.lines() {
        let t = line.trim();
        let low = t.to_ascii_lowercase();
        let want = t.starts_with("Done in")
            || low.starts_with("success")
            || low.contains("packages installed")
            || low.starts_with("packages:")
            || low.contains("packages: +")
            || low.contains("already up to date")
            || low.contains("up to date")
            || low.starts_with("error")
            || low.contains("err!")
            || low.starts_with("warning")
            || low.contains("deprecat");
        if want && !t.is_empty() {
            keep.push(t.to_string());
        }
    }

    if keep.is_empty() {
        return generic::summarize(out);
    }

    keep.dedup();
    let mut body = keep.join("\n");
    if out.code != 0 {
        body.push_str(&format!("\n! {tool} exit {}", out.code));
    }
    body
}

/// Strips ANSI and concatenates stdout and stderr.
fn combined(out: &Outcome) -> String {
    let mut text = generic::strip_ansi(&out.stdout);
    let err = generic::strip_ansi(&out.stderr);
    if !err.is_empty() {
        text.push('\n');
        text.push_str(&err);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pip_keeps_success_line() {
        let stdout = "\
Collecting requests
  Downloading requests-2.31.0-py3-none-any.whl (62 kB)
Requirement already satisfied: idna in /usr/lib
Installing collected packages: requests
Successfully installed requests-2.31.0
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 0 };
        assert_eq!(pip(&out), "Successfully installed requests-2.31.0");
    }

    #[test]
    fn pip_surfaces_error_and_exit() {
        let out = Outcome {
            stdout: String::new(),
            stderr: "ERROR: Could not find a version that satisfies the requirement foo\n".to_string(),
            code: 1,
        };
        let summary = pip(&out);
        assert!(summary.contains("ERROR: Could not find a version"));
        assert!(summary.ends_with("! exit 1"));
    }

    #[test]
    fn poetry_uses_operations_summary() {
        let stdout = "\
Installing dependencies from lock file

Package operations: 3 installs, 1 update, 0 removals

  • Installing certifi (2023.7.22)
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 0 };
        assert_eq!(poetry(&out), "✓ poetry: 3 installs, 1 update, 0 removals");
    }

    #[test]
    fn js_install_keeps_yarn_summary() {
        let stdout = "\
yarn install v1.22.19
[1/4] Resolving packages...
[2/4] Fetching packages...
success Saved lockfile.
Done in 3.45s.
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 0 };
        assert_eq!(js_install(&out, "yarn"), "success Saved lockfile.\nDone in 3.45s.");
    }
}
