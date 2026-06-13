//! Git formatters for `status`, `log`, `diff`, and `branch`.

use crate::format::generic;
use crate::runner::Outcome;

// region: invocation rewrites

/// Rewrites `git status` to porcelain v1 with branch info for reliable parsing.
pub fn rewrite_status(_args: &[String]) -> Vec<String> {
    vec!["git".into(), "status".into(), "--porcelain=v1".into(), "--branch".into()]
}

/// Rewrites `git diff [args]` to `git diff --stat [args]` unless the user
/// already requested a summary/format flag.
pub fn rewrite_diff(args: &[String]) -> Vec<String> {
    if args.iter().any(|a| matches!(a.as_str(), "--stat" | "--numstat" | "--shortstat" | "--name-only")) {
        return args.to_vec();
    }
    let mut out = vec!["git".to_string(), "diff".to_string(), "--stat".to_string()];
    out.extend(args.iter().skip(2).cloned());
    out
}

/// Rewrites `git log [args]` to a compact one-line form, capped at 30 entries
/// unless the user already specified a format or count.
pub fn rewrite_log(args: &[String]) -> Vec<String> {
    let has_format = args.iter().any(|a| a.starts_with("--pretty") || a.starts_with("--format") || a == "--oneline");
    let has_count = args
        .iter()
        .any(|a| a == "-n" || (a.starts_with('-') && a[1..].chars().all(|c| c.is_ascii_digit()) && a.len() > 1));

    let mut out = vec!["git".to_string(), "log".to_string()];
    if !has_format {
        out.push("--oneline".to_string());
    }
    if !has_count {
        out.push("-30".to_string());
    }
    out.extend(args.iter().skip(2).cloned());
    out
}

// endregion

// region: summaries

/// Formats porcelain `git status` output into grouped, symbol-prefixed lines.
pub fn status(out: &Outcome) -> String {
    if out.code != 0 {
        return generic::summarize(out);
    }

    let mut branch = String::new();
    let mut staged: Vec<String> = Vec::new();
    let mut modified: Vec<String> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();
    let mut untracked: Vec<String> = Vec::new();

    for line in generic::strip_ansi(&out.stdout).lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            branch = rest.to_string();
            continue;
        }
        if line.len() < 3 {
            continue;
        }
        let bytes = line.as_bytes();
        let index = bytes[0] as char;
        let worktree = bytes[1] as char;
        let path = line[3..].to_string();

        if index == '?' && worktree == '?' {
            untracked.push(path);
            continue;
        }
        if index != ' ' && index != '?' {
            staged.push(path.clone());
        }
        match worktree {
            'M' => modified.push(path),
            'D' => deleted.push(path),
            _ => {}
        }
    }

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("* {}", normalize_branch(&branch)));
    push_group(&mut lines, '+', &staged);
    push_group(&mut lines, '~', &modified);
    push_group(&mut lines, '-', &deleted);
    push_group(&mut lines, '?', &untracked);

    if lines.len() == 1 {
        lines[0].push_str("  (clean)");
    }
    lines.join("\n")
}

/// Normalizes the porcelain branch header, turning git's verbose ahead/behind
/// suffix into a compact `[↑N ↓M]` annotation.
fn normalize_branch(branch: &str) -> String {
    let (name, tracking) = match branch.split_once(" [") {
        Some((name, rest)) => (name, rest.trim_end_matches(']')),
        None => return branch.to_string(),
    };

    let mut marks = String::new();
    for part in tracking.split(", ") {
        if let Some(n) = part.strip_prefix("ahead ") {
            marks.push_str(&format!("↑{n} "));
        } else if let Some(n) = part.strip_prefix("behind ") {
            marks.push_str(&format!("↓{n} "));
        } else if part == "gone" {
            marks.push_str("gone ");
        }
    }
    let marks = marks.trim_end();
    if marks.is_empty() {
        name.to_string()
    } else {
        format!("{name} [{marks}]")
    }
}

/// Appends a `<symbol> <count>  file1, file2, ...` line when `files` is non-empty.
fn push_group(lines: &mut Vec<String>, symbol: char, files: &[String]) {
    if files.is_empty() {
        return;
    }
    lines.push(format!("{symbol} {}  {}", files.len(), files.join(", ")));
}

/// Formats `git log` output; the rewritten one-line form is already compact, so
/// this just trims and truncates.
pub fn log(out: &Outcome) -> String {
    if out.code != 0 {
        return generic::summarize(out);
    }
    generic::compress(&generic::strip_ansi(&out.stdout), 30, 5)
}

/// Formats `git diff --stat` output, trimming and truncating long file lists.
pub fn diff(out: &Outcome) -> String {
    if out.code != 0 {
        return generic::summarize(out);
    }
    let clean = generic::strip_ansi(&out.stdout);
    if clean.trim().is_empty() {
        return "(no changes)".to_string();
    }
    generic::compress(&clean, 30, 2)
}

/// Formats `git branch` output as the current branch plus a compact list of
/// the others.
pub fn branch(out: &Outcome) -> String {
    if out.code != 0 {
        return generic::summarize(out);
    }

    let mut current = String::new();
    let mut others: Vec<String> = Vec::new();
    for line in generic::strip_ansi(&out.stdout).lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("* ") {
            current = name.to_string();
        } else {
            others.push(trimmed.to_string());
        }
    }

    let mut result = format!("* {current}");
    if !others.is_empty() {
        result.push_str(&format!("  (+{}: {})", others.len(), others.join(", ")));
    }
    result
}

// endregion

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(stdout: &str) -> Outcome {
        Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 0 }
    }

    #[test]
    fn formats_status_groups() {
        let porcelain = "## master...origin/master\n M index.html\n M src/main.rs\n M src/config.rs\n?? .fastembed_cache/\n?? tests/\n";
        let summary = status(&outcome(porcelain));
        assert_eq!(
            summary,
            "* master...origin/master\n~ 3  index.html, src/main.rs, src/config.rs\n? 2  .fastembed_cache/, tests/"
        );
    }

    #[test]
    fn marks_clean_tree() {
        let summary = status(&outcome("## main...origin/main\n"));
        assert_eq!(summary, "* main...origin/main  (clean)");
    }

    #[test]
    fn compacts_ahead_behind() {
        let summary = status(&outcome("## main...origin/main [ahead 1, behind 2]\n"));
        assert_eq!(summary, "* main...origin/main [↑1 ↓2]  (clean)");
    }

    #[test]
    fn separates_staged_and_modified() {
        let summary = status(&outcome("## dev\nM  staged.rs\n M worktree.rs\nMM both.rs\n"));
        assert_eq!(summary, "* dev\n+ 2  staged.rs, both.rs\n~ 2  worktree.rs, both.rs");
    }

    #[test]
    fn formats_branch_list() {
        let summary = branch(&outcome("* master\n  feature\n  dev\n"));
        assert_eq!(summary, "* master  (+2: feature, dev)");
    }

    #[test]
    fn rewrite_status_uses_porcelain() {
        assert_eq!(
            rewrite_status(&["git".into(), "status".into()]),
            vec!["git", "status", "--porcelain=v1", "--branch"]
        );
    }

    #[test]
    fn rewrite_diff_inserts_stat_and_keeps_args() {
        assert_eq!(
            rewrite_diff(&["git".into(), "diff".into(), "HEAD~1".into()]),
            vec!["git", "diff", "--stat", "HEAD~1"]
        );
    }

    #[test]
    fn rewrite_diff_respects_existing_stat_flag() {
        let a = vec!["git".to_string(), "diff".to_string(), "--numstat".to_string()];
        assert_eq!(rewrite_diff(&a), a);
    }

    #[test]
    fn rewrite_log_adds_oneline_and_limit() {
        assert_eq!(rewrite_log(&["git".into(), "log".into()]), vec!["git", "log", "--oneline", "-30"]);
    }

    #[test]
    fn rewrite_log_respects_user_format_and_count() {
        assert_eq!(
            rewrite_log(&["git".into(), "log".into(), "--oneline".into(), "-5".into()]),
            vec!["git", "log", "--oneline", "-5"]
        );
    }
}
