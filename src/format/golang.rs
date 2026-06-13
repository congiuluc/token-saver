//! Go tooling formatters for `go build`/`vet` and `go test`.

use crate::format::generic;
use crate::runner::Outcome;

/// Summarizes `go build`/`vet`/`install`: an ok line on success, or the compiler
/// diagnostics on failure.
pub fn build(out: &Outcome) -> String {
    if out.code == 0 {
        return "✓ go build ok".to_string();
    }

    let text = combined(out);
    let mut errors: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        // Compiler diagnostic: "./main.go:10:2: undefined: foo".
        if t.contains(".go:") && t.matches(':').count() >= 2 {
            errors.push(t.to_string());
        }
    }

    if errors.is_empty() {
        return generic::summarize(out);
    }

    let mut lines = vec![format!("✗ go build: {} error(s)", errors.len())];
    errors.dedup();
    for e in errors.iter().take(10) {
        lines.push(format!("  {e}"));
    }
    if errors.len() > 10 {
        lines.push(format!("  … {} more", errors.len() - 10));
    }
    lines.join("\n")
}

/// Summarizes `go test`: an ok line with the package count on success, or the
/// failed test names on failure.
pub fn test(out: &Outcome) -> String {
    let text = combined(out);
    let mut failures: Vec<String> = Vec::new();
    let mut ok_pkgs = 0usize;

    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("--- FAIL:") {
            if let Some(name) = rest.trim().split_whitespace().next() {
                failures.push(name.to_string());
            }
        }
        if trimmed.starts_with("ok ") || trimmed.starts_with("ok\t") {
            ok_pkgs += 1;
        }
    }

    if out.code == 0 && failures.is_empty() {
        return match ok_pkgs {
            0 => "✓ go test: ok".to_string(),
            1 => "✓ go test: 1 package ok".to_string(),
            n => format!("✓ go test: {n} packages ok"),
        };
    }

    if failures.is_empty() {
        return generic::summarize(out);
    }

    failures.dedup();
    let mut lines = vec![format!("✗ go test: {} test(s) failed", failures.len())];
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
    fn build_reports_ok() {
        let out = Outcome {
            stdout: String::new(),
            stderr: String::new(),
            code: 0,
        };
        assert_eq!(build(&out), "✓ go build ok");
    }

    #[test]
    fn build_reports_errors() {
        let out = Outcome {
            stdout: String::new(),
            stderr: "# example/pkg\n./main.go:10:2: undefined: foo\n".to_string(),
            code: 1,
        };
        let summary = build(&out);
        assert!(summary.starts_with("✗ go build: 1 error(s)"));
        assert!(summary.contains("./main.go:10:2: undefined: foo"));
    }

    #[test]
    fn test_reports_pass() {
        let stdout = "ok  \texample/pkg\t0.123s\nok  \texample/pkg2\t0.045s\n";
        let out = Outcome {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code: 0,
        };
        assert_eq!(test(&out), "✓ go test: 2 packages ok");
    }

    #[test]
    fn test_reports_failures() {
        let stdout = "\
--- FAIL: TestAdd (0.00s)
    add_test.go:10: expected 4, got 5
FAIL
FAIL\texample/pkg\t0.002s
";
        let out = Outcome {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code: 1,
        };
        let summary = test(&out);
        assert!(summary.starts_with("✗ go test: 1 test(s) failed"));
        assert!(summary.contains("- TestAdd"));
    }
}
