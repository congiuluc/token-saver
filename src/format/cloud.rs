//! Formatters for cloud and developer CLIs: Azure CLI (`az`), Azure Developer
//! CLI (`azd`), GitHub CLI (`gh`), and the Copilot CLI (`copilot`).

use crate::format::generic;
use crate::runner::Outcome;

/// Summarizes an Azure CLI (`az`) invocation. On failure the `ERROR:` lines are
/// surfaced; on success the (often large JSON/table) payload is compressed to a
/// head/tail excerpt.
pub fn az(out: &Outcome) -> String {
    let text = combined(out);
    let mut errors: Vec<String> = Vec::new();

    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("ERROR:") {
            errors.push(rest.trim().to_string());
        }
    }

    if !errors.is_empty() {
        errors.dedup();
        let mut lines = vec![format!("✗ az: {} error(s)", errors.len())];
        for e in errors.iter().take(8) {
            lines.push(format!("  {e}"));
        }
        if errors.len() > 8 {
            lines.push(format!("  … {} more", errors.len() - 8));
        }
        return lines.join("\n");
    }

    if out.code != 0 {
        return generic::summarize(out);
    }

    let payload = generic::strip_ansi(&out.stdout);
    if payload.trim().is_empty() {
        return "✓ az: ok".to_string();
    }
    generic::compress(&payload, 16, 4)
}

/// Summarizes an Azure Developer CLI (`azd`) run, keeping the step results
/// (`(✓) Done:` / `(x) Failed:`), endpoints, and the final SUCCESS/ERROR line.
pub fn azd(out: &Outcome) -> String {
    let text = combined(out);
    let mut keep: Vec<String> = Vec::new();

    for line in text.lines() {
        let t = line.trim();
        let want = t.starts_with("SUCCESS:")
            || t.starts_with("ERROR:")
            || t.starts_with("WARNING:")
            || t.contains("(✓) Done:")
            || t.contains("(x) Failed:")
            || t.starts_with("- Endpoint:")
            || t.starts_with("Deploying services")
            || t.starts_with("Provisioning Azure resources");
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

/// Summarizes a GitHub CLI (`gh`) invocation: errors fall back to generic
/// extraction; successful list/table/text output is compressed.
pub fn gh(out: &Outcome) -> String {
    if out.code != 0 {
        return generic::summarize(out);
    }
    let text = generic::strip_ansi(&out.stdout);
    if text.trim().is_empty() {
        return "✓ gh: ok".to_string();
    }
    generic::compress(&text, 20, 4)
}

/// Summarizes a Copilot CLI (`copilot`) invocation, compressing its output on
/// success and surfacing errors on failure.
pub fn copilot(out: &Outcome) -> String {
    if out.code != 0 {
        return generic::summarize(out);
    }
    let text = generic::strip_ansi(&out.stdout);
    if text.trim().is_empty() {
        return "✓ copilot: ok".to_string();
    }
    generic::compress(&text, 24, 6)
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
    fn az_surfaces_errors() {
        let out = Outcome {
            stdout: String::new(),
            stderr: "ERROR: (ResourceNotFound) The Resource 'x' was not found.\n".to_string(),
            code: 1,
        };
        let summary = az(&out);
        assert!(summary.starts_with("✗ az: 1 error(s)"));
        assert!(summary.contains("(ResourceNotFound) The Resource 'x' was not found."));
    }

    #[test]
    fn az_compresses_success_payload() {
        let out = Outcome {
            stdout: "Name    Location\n------  --------\nfoo     eastus\n".to_string(),
            stderr: String::new(),
            code: 0,
        };
        let summary = az(&out);
        assert!(summary.contains("foo"));
    }

    #[test]
    fn azd_keeps_step_results() {
        let stdout = "\
Provisioning Azure resources (azd provision)

  (✓) Done: Resource group: rg-foo
  (✓) Done: Storage account: stfoo123

SUCCESS: Your application was provisioned in Azure in 1 minute.
";
        let out = Outcome {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code: 0,
        };
        let summary = azd(&out);
        assert!(summary.contains("(✓) Done: Resource group: rg-foo"));
        assert!(summary.contains("SUCCESS: Your application was provisioned"));
        assert!(!summary.contains("Fetching"));
    }

    #[test]
    fn gh_reports_ok_when_empty() {
        let out = Outcome {
            stdout: String::new(),
            stderr: String::new(),
            code: 0,
        };
        assert_eq!(gh(&out), "✓ gh: ok");
    }
}
