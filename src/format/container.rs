//! Container-tooling formatters for `docker ps` and `kubectl get`.

use crate::format::generic;
use crate::format::table;
use crate::runner::Outcome;

/// Summarizes `docker ps` to `name  image  status`, one container per line.
pub fn docker_ps(out: &Outcome) -> String {
    if out.code != 0 {
        return generic::summarize(out);
    }
    let rows = match table::select(&out.stdout, &["NAMES", "IMAGE", "STATUS"]) {
        Some(rows) => rows,
        None => return generic::summarize(out),
    };
    if rows.is_empty() {
        return "(no containers)".to_string();
    }
    rows.iter()
        .map(|cells| cells.join("  "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Summarizes `kubectl get` resources, keeping identity and health columns and
/// flagging any row whose status is not `Running`/`Active`.
pub fn kubectl_get(out: &Outcome) -> String {
    if out.code != 0 {
        return generic::summarize(out);
    }
    let rows = match table::select(&out.stdout, &["NAME", "READY", "STATUS", "RESTARTS"]) {
        Some(rows) => rows,
        None => return generic::summarize(out),
    };
    if rows.is_empty() {
        return "(no resources)".to_string();
    }

    rows.iter()
        .map(|cells| {
            let status = cells.get(2).map(String::as_str).unwrap_or("");
            let healthy = matches!(status, "Running" | "Active" | "Completed" | "");
            let prefix = if healthy { "  " } else { "! " };
            format!("{prefix}{}", cells.join("  "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(stdout: &str) -> Outcome {
        Outcome {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code: 0,
        }
    }

    #[test]
    fn docker_ps_keeps_key_columns() {
        let text = "\
CONTAINER ID   IMAGE     COMMAND   STATUS         NAMES
abc123         nginx     \"run\"     Up 2 minutes   web
";
        assert_eq!(docker_ps(&ok(text)), "web  nginx  Up 2 minutes");
    }

    #[test]
    fn docker_ps_reports_empty() {
        let text = "CONTAINER ID   IMAGE     COMMAND   STATUS   NAMES\n";
        assert_eq!(docker_ps(&ok(text)), "(no containers)");
    }

    #[test]
    fn docker_ps_falls_back_on_error() {
        let out = Outcome {
            stdout: String::new(),
            stderr: "Cannot connect to the Docker daemon\n".to_string(),
            code: 1,
        };
        let summary = docker_ps(&out);
        assert!(summary.contains("Cannot connect to the Docker daemon"));
        assert!(summary.contains("! exit 1"));
    }

    #[test]
    fn kubectl_flags_unhealthy_rows() {
        let text = "\
NAME      READY   STATUS             RESTARTS   AGE
web-0     1/1     Running            0          5m
db-0      0/1     CrashLoopBackOff   3          5m
";
        assert_eq!(
            kubectl_get(&ok(text)),
            "  web-0  1/1  Running  0\n! db-0  0/1  CrashLoopBackOff  3"
        );
    }
}
