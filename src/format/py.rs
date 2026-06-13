//! Python tooling formatter for `pytest`.

use crate::format::generic;
use crate::runner::Outcome;

/// Summarizes a pytest run into a pass/fail line plus the names of failed tests.
pub fn pytest(out: &Outcome) -> String {
    let text = {
        let mut t = generic::strip_ansi(&out.stdout);
        let err = generic::strip_ansi(&out.stderr);
        if !err.is_empty() {
            t.push('\n');
            t.push_str(&err);
        }
        t
    };

    let mut failures: Vec<String> = Vec::new();
    let mut summary_line: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        // Failed tests are reported as "FAILED path::test - reason".
        if let Some(rest) = trimmed.strip_prefix("FAILED ") {
            let name = rest.split(" - ").next().unwrap_or(rest);
            failures.push(name.trim().to_string());
        }

        // The final summary line is wrapped in '=' characters, e.g.
        // "===== 2 failed, 5 passed in 0.31s =====".
        if trimmed.starts_with('=') && trimmed.ends_with('=') {
            let inner = trimmed.trim_matches('=').trim();
            if inner.contains("passed")
                || inner.contains("failed")
                || inner.contains("error")
                || inner.contains("no tests ran")
            {
                summary_line = Some(inner.to_string());
            }
        }
    }

    let summary = match summary_line {
        Some(s) => s,
        None => return generic::summarize(out),
    };

    failures.dedup();
    let symbol = if out.code == 0 { "✓" } else { "✗" };
    let mut lines = vec![format!("{symbol} {summary}")];
    for name in failures.iter().take(10) {
        lines.push(format!("  - {name}"));
    }
    if failures.len() > 10 {
        lines.push(format!("  … {} more", failures.len() - 10));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_failures() {
        let stdout = "\
FAILED tests/test_api.py::test_login - AssertionError
FAILED tests/test_api.py::test_logout - KeyError
==================== 2 failed, 8 passed in 1.20s ====================
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 1 };
        assert_eq!(
            pytest(&out),
            "✗ 2 failed, 8 passed in 1.20s\n  - tests/test_api.py::test_login\n  - tests/test_api.py::test_logout"
        );
    }
}
