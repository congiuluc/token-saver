//! TypeScript/JavaScript tooling formatters for `tsc` and `eslint`.

use crate::format::generic;
use crate::runner::Outcome;

/// Summarizes a TypeScript compiler (`tsc`) run: a clean line on success, or the
/// error count plus the first diagnostics on failure.
pub fn tsc(out: &Outcome) -> String {
    let text = combined(out);
    let mut errors: Vec<String> = Vec::new();
    let mut found: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        // Diagnostic: "src/index.ts(12,5): error TS2322: ...".
        if trimmed.contains(": error TS") {
            errors.push(trimmed.to_string());
        }
        // Footer: "Found 3 errors in 2 files." (or "Found 1 error in ...").
        if trimmed.starts_with("Found ") && trimmed.contains("error") {
            found = Some(trimmed.trim_end_matches('.').to_string());
        }
    }

    if out.code == 0 && errors.is_empty() {
        return "✓ tsc: no type errors".to_string();
    }

    if errors.is_empty() {
        return generic::summarize(out);
    }

    let header = found.unwrap_or_else(|| format!("Found {} errors", errors.len()));
    let mut lines = vec![format!("✗ tsc: {header}")];
    errors.dedup();
    for e in errors.iter().take(10) {
        lines.push(format!("  {e}"));
    }
    if errors.len() > 10 {
        lines.push(format!("  … {} more", errors.len() - 10));
    }
    lines.join("\n")
}

/// Summarizes an ESLint run (default "stylish" formatter): the problem summary
/// plus the error-level findings, each prefixed with its file.
pub fn eslint(out: &Outcome) -> String {
    let text = combined(out);
    let mut summary: Option<String> = None;
    let mut errors: Vec<String> = Vec::new();
    let mut file = String::new();

    for raw in text.lines() {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }

        // Summary footer: "✖ 5 problems (3 errors, 2 warnings)".
        if let Some(rest) = t.strip_prefix('✖') {
            summary = Some(rest.trim().to_string());
            continue;
        }

        // A file header has no leading whitespace and is not a problem line.
        if !raw.starts_with([' ', '\t']) && !is_problem(t) {
            file = t.to_string();
            continue;
        }

        if is_problem(t) && t.contains("error") {
            errors.push(format!("{file}  {t}"));
        }
    }

    if out.code == 0 && summary.is_none() {
        return "✓ eslint: clean".to_string();
    }

    let symbol = if out.code == 0 { "✓" } else { "✗" };
    let header = summary.unwrap_or_else(|| format!("{} error(s)", errors.len()));
    let mut lines = vec![format!("{symbol} eslint: {header}")];
    errors.dedup();
    for e in errors.iter().take(10) {
        lines.push(format!("  {e}"));
    }
    if errors.len() > 10 {
        lines.push(format!("  … {} more", errors.len() - 10));
    }
    lines.join("\n")
}

/// Returns `true` when `t` looks like an ESLint problem line, i.e. it starts
/// with a `line:col` location such as `12:5`.
fn is_problem(t: &str) -> bool {
    t.split_whitespace()
        .next()
        .map(|loc| {
            let mut parts = loc.split(':');
            matches!((parts.next(), parts.next(), parts.next()), (Some(a), Some(b), None)
                if !a.is_empty()
                    && !b.is_empty()
                    && a.bytes().all(|c| c.is_ascii_digit())
                    && b.bytes().all(|c| c.is_ascii_digit()))
        })
        .unwrap_or(false)
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
    fn tsc_reports_clean() {
        let out = Outcome { stdout: String::new(), stderr: String::new(), code: 0 };
        assert_eq!(tsc(&out), "✓ tsc: no type errors");
    }

    #[test]
    fn tsc_reports_errors() {
        let stdout = "\
src/index.ts(12,5): error TS2322: Type 'string' is not assignable to type 'number'.
Found 1 error in 1 file.
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 2 };
        let summary = tsc(&out);
        assert!(summary.starts_with("✗ tsc: Found 1 error in 1 file"));
        assert!(summary.contains("error TS2322"));
    }

    #[test]
    fn eslint_reports_clean() {
        let out = Outcome { stdout: String::new(), stderr: String::new(), code: 0 };
        assert_eq!(eslint(&out), "✓ eslint: clean");
    }

    #[test]
    fn eslint_surfaces_errors_with_file() {
        let stdout = "\
/repo/src/app.js
  12:5  error  'x' is assigned a value but never used  no-unused-vars
  20:1  warning  Missing semicolon  semi

✖ 2 problems (1 error, 1 warning)
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 1 };
        let summary = eslint(&out);
        assert!(summary.starts_with("✗ eslint: 2 problems (1 error, 1 warning)"));
        assert!(summary.contains("/repo/src/app.js  12:5  error"));
        assert!(!summary.contains("Missing semicolon"));
    }
}
