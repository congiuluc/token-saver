//! Node tooling formatter for `npm install`/`ci`.

use crate::format::generic;
use crate::runner::Outcome;

/// Summarizes `npm install`/`ci` by extracting the package and audit summary
/// lines that npm prints, discarding progress noise.
pub fn install(out: &Outcome) -> String {
    let text = {
        let mut t = generic::strip_ansi(&out.stdout);
        let err = generic::strip_ansi(&out.stderr);
        if !err.is_empty() {
            t.push('\n');
            t.push_str(&err);
        }
        t
    };

    let mut highlights: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let keep = trimmed.starts_with("added ")
            || trimmed.starts_with("removed ")
            || trimmed.starts_with("changed ")
            || trimmed.contains("packages are looking for funding")
            || trimmed.contains("audited ")
            || trimmed.contains("vulnerabilit")
            || trimmed.starts_with("npm error")
            || trimmed.starts_with("npm warn");
        if keep && !trimmed.is_empty() {
            highlights.push(trimmed.to_string());
        }
    }

    if highlights.is_empty() {
        return generic::summarize(out);
    }

    highlights.dedup();
    let mut body = highlights.join("\n");
    if out.code != 0 {
        body.push_str(&format!("\n! exit {}", out.code));
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_npm_summary() {
        let out = Outcome {
            stdout: "npm http fetch GET 200 ...\nadded 120 packages in 3s\n\n12 packages are looking for funding\nfound 0 vulnerabilities\n"
                .to_string(),
            stderr: String::new(),
            code: 0,
        };
        assert_eq!(
            install(&out),
            "added 120 packages in 3s\n12 packages are looking for funding\nfound 0 vulnerabilities"
        );
    }

    #[test]
    fn surfaces_errors_and_exit_code() {
        let out = Outcome {
            stdout: String::new(),
            stderr: "npm error code E404\nnpm error 404 Not Found - GET registry\n".to_string(),
            code: 1,
        };
        let summary = install(&out);
        assert!(summary.contains("npm error code E404"));
        assert!(summary.ends_with("! exit 1"));
    }
}
