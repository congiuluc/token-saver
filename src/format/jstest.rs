//! JavaScript test-runner formatters for Jest and Vitest.

use crate::format::generic;
use crate::runner::Outcome;

/// Summarizes a Jest run from its `Tests:` summary line and `✕`-marked failures.
pub fn jest(out: &Outcome) -> String {
    let text = combined(out);
    let mut summary: Option<String> = None;
    let mut failures: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Tests:") {
            summary = Some(rest.trim().to_string());
        }
        if let Some(name) = failure_name(trimmed) {
            failures.push(name);
        }
    }

    finish("jest", out, summary, failures)
}

/// Summarizes a Vitest run from its `Tests` summary line and `×`-marked failures.
pub fn vitest(out: &Outcome) -> String {
    let text = combined(out);
    let mut summary: Option<String> = None;
    let mut failures: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        // Vitest prints "Tests  2 failed | 8 passed (10)" (no colon).
        if let Some(rest) = trimmed.strip_prefix("Tests ") {
            if rest.contains("passed") || rest.contains("failed") {
                summary = Some(rest.trim().to_string());
            }
        }
        if let Some(name) = failure_name(trimmed) {
            failures.push(name);
        }
    }

    finish("vitest", out, summary, failures)
}

/// Extracts a failed test name from a `✕`/`×`-prefixed line, dropping any
/// trailing ` (12 ms)` duration annotation.
fn failure_name(trimmed: &str) -> Option<String> {
    let rest = trimmed.strip_prefix("✕ ").or_else(|| trimmed.strip_prefix("× "))?;
    let name = rest.split(" (").next().unwrap_or(rest).trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Builds the shared pass/fail summary, falling back to generic compression when
/// no recognizable summary line was found.
fn finish(tool: &str, out: &Outcome, summary: Option<String>, mut failures: Vec<String>) -> String {
    let summary = match summary {
        Some(s) => s,
        None => return generic::summarize(out),
    };

    failures.dedup();
    let symbol = if out.code == 0 { "✓" } else { "✗" };
    let mut lines = vec![format!("{symbol} {tool}: {summary}")];
    for name in failures.iter().take(10) {
        lines.push(format!("  - {name}"));
    }
    if failures.len() > 10 {
        lines.push(format!("  … {} more", failures.len() - 10));
    }
    lines.join("\n")
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
    fn jest_reports_failures() {
        let stdout = "\
  ✕ adds numbers (4 ms)
  ✕ subtracts numbers
Tests:       2 failed, 8 passed, 10 total
Test Suites: 1 failed, 2 passed, 3 total
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 1 };
        let summary = jest(&out);
        assert!(summary.starts_with("✗ jest: 2 failed, 8 passed, 10 total"));
        assert!(summary.contains("- adds numbers"));
        assert!(summary.contains("- subtracts numbers"));
    }

    #[test]
    fn vitest_reports_pass() {
        let stdout = "\
Test Files  2 passed (2)
     Tests  10 passed (10)
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 0 };
        assert_eq!(vitest(&out), "✓ vitest: 10 passed (10)");
    }
}
