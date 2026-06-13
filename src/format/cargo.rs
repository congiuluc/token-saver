//! Cargo formatters for `build`/`check` and `test`.

use crate::format::generic;
use crate::runner::Outcome;

/// Summarizes `cargo build`/`cargo check` into a pass/fail line with warning and
/// error counts, plus the first few error messages on failure.
pub fn build(out: &Outcome) -> String {
    let text = combined(out);
    let mut warnings = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("warning:") {
            warnings += 1;
        } else if trimmed.starts_with("error") && trimmed[5..].starts_with(|c| c == ':' || c == '[') {
            errors.push(trimmed.to_string());
        }
    }

    if out.code == 0 {
        return match warnings {
            0 => "✓ build ok".to_string(),
            1 => "✓ build ok (1 warning)".to_string(),
            n => format!("✓ build ok ({n} warnings)"),
        };
    }

    let mut lines = vec![format!(
        "✗ build failed: {} error(s), {warnings} warning(s)",
        errors.len()
    )];
    for err in errors.iter().take(5) {
        lines.push(format!("  {err}"));
    }
    if errors.len() > 5 {
        lines.push(format!("  … {} more", errors.len() - 5));
    }
    if errors.is_empty() {
        // No recognizable cargo errors; fall back so the user sees something.
        return generic::summarize(out);
    }
    lines.join("\n")
}

/// Summarizes `cargo test` into a pass/fail line with totals and failed test
/// names.
pub fn test(out: &Outcome) -> String {
    let text = combined(out);

    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut ignored = 0usize;
    let mut saw_result = false;
    let mut failures: Vec<String> = Vec::new();
    let mut in_failures_block = false;

    for line in text.lines() {
        let trimmed = line.trim();

        if let Some(rest) = trimmed.strip_prefix("test result:") {
            saw_result = true;
            passed += count_before(rest, "passed");
            failed += count_before(rest, "failed");
            ignored += count_before(rest, "ignored");
        }

        // The "failures:" block lists each failed test on its own indented line.
        if trimmed == "failures:" {
            in_failures_block = true;
            continue;
        }
        if in_failures_block {
            if trimmed.is_empty() || trimmed.starts_with("test result:") {
                in_failures_block = false;
            } else if !trimmed.contains("stdout") && !trimmed.contains("----") {
                failures.push(trimmed.to_string());
            }
        }
    }

    if !saw_result {
        // Likely a compile failure before any test ran.
        return generic::summarize(out);
    }

    failures.sort();
    failures.dedup();

    if failed == 0 {
        let mut line = format!("✓ tests: {passed} passed");
        if ignored > 0 {
            line.push_str(&format!(", {ignored} ignored"));
        }
        return line;
    }

    let mut lines = vec![format!(
        "✗ tests: {failed} failed / {} run",
        passed + failed
    )];
    for name in failures.iter().take(10) {
        lines.push(format!("  - {name}"));
    }
    if failures.len() > 10 {
        lines.push(format!("  … {} more", failures.len() - 10));
    }
    lines.join("\n")
}

/// Returns the integer immediately preceding `keyword` in a `test result:` line,
/// e.g. extracts `3` from `... 3 failed; ...`.
fn count_before(text: &str, keyword: &str) -> usize {
    let idx = match text.find(keyword) {
        Some(i) => i,
        None => return 0,
    };
    text[..idx]
        .split_whitespace()
        .last()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
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
    fn summarizes_passing_tests() {
        let out = Outcome {
            stdout: "test result: ok. 42 passed; 0 failed; 1 ignored; 0 measured\n".to_string(),
            stderr: String::new(),
            code: 0,
        };
        assert_eq!(test(&out), "✓ tests: 42 passed, 1 ignored");
    }

    #[test]
    fn summarizes_failing_tests() {
        let stdout = "\
failures:
    tests::alpha
    tests::beta

test result: FAILED. 5 passed; 2 failed; 0 ignored; 0 measured
";
        let out = Outcome {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code: 101,
        };
        assert_eq!(
            test(&out),
            "✗ tests: 2 failed / 7 run\n  - tests::alpha\n  - tests::beta"
        );
    }

    #[test]
    fn summarizes_build_warnings() {
        let out = Outcome {
            stdout: String::new(),
            stderr: "warning: unused variable `x`\nwarning: unused import\n".to_string(),
            code: 0,
        };
        assert_eq!(build(&out), "✓ build ok (2 warnings)");
    }

    #[test]
    fn summarizes_build_failure_with_errors() {
        let out = Outcome {
            stdout: String::new(),
            stderr: "warning: unused import\nerror[E0432]: unresolved import `foo`\nerror: aborting due to previous error\n"
                .to_string(),
            code: 101,
        };
        let summary = build(&out);
        assert!(summary.starts_with("✗ build failed: 2 error(s), 1 warning(s)"));
        assert!(summary.contains("error[E0432]: unresolved import `foo`"));
    }
}
