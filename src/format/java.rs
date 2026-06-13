//! Java build-tool formatters for Maven (`mvn`) and Gradle (`gradle`).

use crate::format::generic;
use crate::runner::Outcome;

/// Summarizes a Maven run, reporting the BUILD SUCCESS/FAILURE verdict, the
/// Surefire test totals when present, and the first few `[ERROR]` lines.
pub fn maven(out: &Outcome) -> String {
    let text = combined(out);
    let mut status: Option<bool> = None;
    let mut test_summary: Option<String> = None;
    let mut errors: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        let body = strip_mvn_prefix(trimmed);

        if body.contains("BUILD SUCCESS") {
            status = Some(true);
        } else if body.contains("BUILD FAILURE") {
            status = Some(false);
        }

        // Surefire aggregate: "Tests run: 12, Failures: 1, Errors: 0, Skipped: 2".
        if body.starts_with("Tests run:") && (body.contains("Failures:") || body.contains("Errors:")) {
            test_summary = Some(body.trim().to_string());
        }

        if let Some(rest) = trimmed.strip_prefix("[ERROR]") {
            let r = rest.trim();
            if !r.is_empty()
                && !r.contains("BUILD FAILURE")
                && !r.starts_with("->")
                && !r.starts_with("For more information")
                && !r.starts_with("Re-run Maven")
                && !r.starts_with("To see the full")
                && !r.starts_with("After correcting")
            {
                errors.push(r.to_string());
            }
        }
    }

    let success = match status {
        Some(s) => s,
        None => return generic::summarize(out),
    };

    let verdict = if success { "BUILD SUCCESS" } else { "BUILD FAILURE" };
    let symbol = if success && out.code == 0 { "✓" } else { "✗" };
    let head = match &test_summary {
        Some(s) => format!("{symbol} maven: {verdict} · {s}"),
        None => format!("{symbol} maven: {verdict}"),
    };

    if success && out.code == 0 {
        return head;
    }

    let mut lines = vec![head];
    errors.dedup();
    for e in errors.iter().take(8) {
        lines.push(format!("  {e}"));
    }
    if errors.len() > 8 {
        lines.push(format!("  … {} more", errors.len() - 8));
    }
    lines.join("\n")
}

/// Summarizes a Gradle run, reporting the BUILD SUCCESSFUL/FAILED verdict and
/// listing failed tasks and tests.
pub fn gradle(out: &Outcome) -> String {
    let text = combined(out);
    let mut status: Option<bool> = None;
    let mut failed_tasks: Vec<String> = Vec::new();
    let mut failed_tests: Vec<String> = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("BUILD SUCCESSFUL") {
            status = Some(true);
        } else if trimmed.starts_with("BUILD FAILED") {
            status = Some(false);
        }

        if let Some(name) = trimmed.strip_suffix(" FAILED") {
            if let Some(task) = name.strip_prefix("> Task ") {
                failed_tasks.push(task.trim().to_string());
            } else {
                // Test failure, e.g. "com.example.MathTest > adds FAILED".
                failed_tests.push(name.trim().to_string());
            }
        }
    }

    let success = match status {
        Some(s) => s,
        None => return generic::summarize(out),
    };

    if success && out.code == 0 {
        return "✓ gradle: BUILD SUCCESSFUL".to_string();
    }

    let mut lines = vec!["✗ gradle: BUILD FAILED".to_string()];
    failed_tasks.dedup();
    for t in failed_tasks.iter().take(5) {
        lines.push(format!("  task {t}"));
    }
    failed_tests.dedup();
    for t in failed_tests.iter().take(10) {
        lines.push(format!("  - {t}"));
    }
    let extra = failed_tests.len().saturating_sub(10);
    if extra > 0 {
        lines.push(format!("  … {extra} more"));
    }
    lines.join("\n")
}

/// Strips a leading Maven log level prefix (`[INFO] `, `[ERROR] `, `[WARNING] `).
fn strip_mvn_prefix(line: &str) -> &str {
    for p in ["[INFO] ", "[ERROR] ", "[WARNING] "] {
        if let Some(rest) = line.strip_prefix(p) {
            return rest;
        }
    }
    line
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
    fn maven_reports_success_with_tests() {
        let stdout = "\
[INFO] Tests run: 12, Failures: 0, Errors: 0, Skipped: 2
[INFO] BUILD SUCCESS
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 0 };
        assert_eq!(maven(&out), "✓ maven: BUILD SUCCESS · Tests run: 12, Failures: 0, Errors: 0, Skipped: 2");
    }

    #[test]
    fn maven_reports_failure_with_errors() {
        let stdout = "\
[ERROR] /repo/Main.java:[10,5] cannot find symbol
[INFO] BUILD FAILURE
[ERROR] To see the full stack trace, re-run with -e.
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 1 };
        let summary = maven(&out);
        assert!(summary.starts_with("✗ maven: BUILD FAILURE"));
        assert!(summary.contains("cannot find symbol"));
        assert!(!summary.contains("To see the full"));
    }

    #[test]
    fn gradle_reports_success() {
        let out = Outcome { stdout: "BUILD SUCCESSFUL in 3s\n".to_string(), stderr: String::new(), code: 0 };
        assert_eq!(gradle(&out), "✓ gradle: BUILD SUCCESSFUL");
    }

    #[test]
    fn gradle_reports_failed_tasks_and_tests() {
        let stdout = "\
com.example.MathTest > adds FAILED
> Task :test FAILED
BUILD FAILED in 4s
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 1 };
        let summary = gradle(&out);
        assert!(summary.starts_with("✗ gradle: BUILD FAILED"));
        assert!(summary.contains("task :test"));
        assert!(summary.contains("- com.example.MathTest > adds"));
    }
}
