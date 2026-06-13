//! .NET formatters for `dotnet build`/`publish` and `dotnet test`.

use crate::format::generic;
use crate::runner::Outcome;

/// Summarizes `dotnet build`/`publish`/`pack` into a pass/fail line with warning
/// and error counts, surfacing the first few compiler diagnostics on failure.
pub fn build(out: &Outcome) -> String {
    let text = combined(out);
    let mut errors: Vec<String> = Vec::new();
    let mut warnings = 0usize;
    let mut summary_errors: Option<usize> = None;
    let mut summary_warnings: Option<usize> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        // MSBuild diagnostics: "Program.cs(12,5): error CS1002: ; expected [proj]".
        if let Some(idx) = trimmed.find(": error ") {
            let mut err = trimmed[idx + 2..].to_string();
            // Drop a trailing " [project.csproj]" so the same error from multiple
            // target frameworks collapses on dedup.
            if err.ends_with(']') {
                if let Some(b) = err.rfind(" [") {
                    err.truncate(b);
                }
            }
            errors.push(err);
        } else if trimmed.contains(": warning ") {
            warnings += 1;
        }

        // MSBuild summary footer: "    3 Warning(s)" / "    1 Error(s)".
        if let Some(n) = trimmed.strip_suffix(" Warning(s)").and_then(|s| s.trim().parse::<usize>().ok()) {
            summary_warnings = Some(n);
        }
        if let Some(n) = trimmed.strip_suffix(" Error(s)").and_then(|s| s.trim().parse::<usize>().ok()) {
            summary_errors = Some(n);
        }
    }

    let warn_count = summary_warnings.unwrap_or(warnings);
    let err_count = summary_errors.unwrap_or(errors.len());

    if out.code == 0 && err_count == 0 {
        return match warn_count {
            0 => "✓ build ok".to_string(),
            1 => "✓ build ok (1 warning)".to_string(),
            n => format!("✓ build ok ({n} warnings)"),
        };
    }

    if errors.is_empty() {
        // No recognizable compiler errors; fall back so the user sees something.
        return generic::summarize(out);
    }

    let mut lines = vec![format!("✗ build failed: {err_count} error(s), {warn_count} warning(s)")];
    errors.dedup();
    for err in errors.iter().take(5) {
        lines.push(format!("  {err}"));
    }
    if errors.len() > 5 {
        lines.push(format!("  … {} more", errors.len() - 5));
    }
    lines.join("\n")
}

/// Summarizes `dotnet test` (VSTest) into a pass/fail line with the failed test
/// names.
pub fn test(out: &Outcome) -> String {
    let text = combined(out);
    let mut failures: Vec<String> = Vec::new();
    let mut summary: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        // Failed tests are reported as "Failed Namespace.Class.Method [12 ms]".
        if let Some(rest) = trimmed.strip_prefix("Failed ") {
            let name = rest.split(" [").next().unwrap_or(rest);
            failures.push(name.trim().to_string());
        }

        // VSTest summary: "Passed!  - Failed: 0, Passed: 10, ..." (or "Failed!").
        if trimmed.starts_with("Passed!") || trimmed.starts_with("Failed!") {
            summary = Some(trimmed.to_string());
        }
    }

    let summary = match summary {
        Some(s) => s,
        None => return generic::summarize(out),
    };

    failures.sort();
    failures.dedup();
    let symbol = if out.code == 0 { "✓" } else { "✗" };
    // Strip the leading "Passed! / Failed!" marker, keeping the count detail.
    let detail = summary.split_once(" - ").map(|x| x.1).unwrap_or(&summary).trim();
    let mut lines = vec![format!("{symbol} tests: {detail}")];
    for name in failures.iter().take(10) {
        lines.push(format!("  - {name}"));
    }
    if failures.len() > 10 {
        lines.push(format!("  … {} more", failures.len() - 10));
    }
    lines.join("\n")
}

/// Summarizes `dotnet restore` (NuGet) into an ok line with the project count.
pub fn restore(out: &Outcome) -> String {
    if out.code != 0 {
        return generic::summarize(out);
    }
    let text = combined(out);
    let restored = text.lines().filter(|l| l.trim().starts_with("Restored ")).count();
    match restored {
        0 => "✓ restore ok".to_string(),
        1 => "✓ restore ok (1 project)".to_string(),
        n => format!("✓ restore ok ({n} projects)"),
    }
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
    fn summarizes_successful_build() {
        let out = Outcome {
            stdout: "Build succeeded.\n    0 Warning(s)\n    0 Error(s)\n".to_string(),
            stderr: String::new(),
            code: 0,
        };
        assert_eq!(build(&out), "✓ build ok");
    }

    #[test]
    fn summarizes_build_warnings_from_summary() {
        let out = Outcome {
            stdout: "Build succeeded.\n    3 Warning(s)\n    0 Error(s)\n".to_string(),
            stderr: String::new(),
            code: 0,
        };
        assert_eq!(build(&out), "✓ build ok (3 warnings)");
    }

    #[test]
    fn summarizes_build_failure_with_errors() {
        let out = Outcome {
            stdout: "Program.cs(12,5): error CS1002: ; expected [/repo/App.csproj]\n    1 Error(s)\n".to_string(),
            stderr: String::new(),
            code: 1,
        };
        let summary = build(&out);
        assert!(summary.starts_with("✗ build failed: 1 error(s), 0 warning(s)"));
        assert!(summary.contains("error CS1002: ; expected"));
        assert!(!summary.contains("App.csproj"));
    }

    #[test]
    fn summarizes_passing_tests() {
        let out = Outcome {
            stdout: "Passed!  - Failed: 0, Passed: 10, Skipped: 0, Total: 10, Duration: 1 s\n".to_string(),
            stderr: String::new(),
            code: 0,
        };
        assert_eq!(test(&out), "✓ tests: Failed: 0, Passed: 10, Skipped: 0, Total: 10, Duration: 1 s");
    }

    #[test]
    fn summarizes_failing_tests() {
        let stdout = "\
  Failed App.Tests.MathTests.Adds [3 ms]
  Failed App.Tests.MathTests.Subtracts [2 ms]
Failed!  - Failed: 2, Passed: 8, Skipped: 0, Total: 10, Duration: 1 s
";
        let out = Outcome { stdout: stdout.to_string(), stderr: String::new(), code: 1 };
        let summary = test(&out);
        assert!(summary.starts_with("✗ tests: Failed: 2, Passed: 8"));
        assert!(summary.contains("- App.Tests.MathTests.Adds"));
        assert!(summary.contains("- App.Tests.MathTests.Subtracts"));
    }
}
