//! Output formatting: routes a command's captured output to the most specific
//! formatter available, falling back to generic heuristic compression.

use crate::runner::Outcome;

pub mod cargo;
pub mod cloud;
pub mod container;
pub mod dotnet;
pub mod generic;
pub mod git;
pub mod golang;
pub mod java;
pub mod jstest;
pub mod node;
pub mod pkg;
pub mod py;
pub mod table;
pub mod ts;

/// Returns the argument vector that should actually be executed.
///
/// For a few commands we substitute a machine-readable variant so the output
/// can be parsed reliably (for example, porcelain `git status`). Every other
/// command is run exactly as the user typed it.
pub fn rewrite(args: &[String]) -> Vec<String> {
    match (arg(args, 0), arg(args, 1)) {
        (Some("git"), Some("status")) => git::rewrite_status(args),
        (Some("git"), Some("diff")) => git::rewrite_diff(args),
        (Some("git"), Some("log")) => git::rewrite_log(args),
        _ => args.to_vec(),
    }
}

/// Produces the compact summary for a finished command.
///
/// Dispatch is keyed off the command the user originally typed, not any
/// rewritten invocation. Unknown commands fall through to [`generic::summarize`].
/// When `extreme` is set, unrecognized commands are compressed far more
/// aggressively (errors plus a stats footer only).
pub fn summarize(args: &[String], out: &Outcome, extreme: bool) -> String {
    let cmd = base_name(arg(args, 0).unwrap_or(""));
    let sub = arg(args, 1).unwrap_or("");

    let mut body = match (cmd.as_str(), sub) {
        ("git", "status") => git::status(out),
        ("git", "log") => git::log(out),
        ("git", "diff") => git::diff(out),
        ("git", "branch") => git::branch(out),
        ("cargo", "build") | ("cargo", "b") | ("cargo", "check") | ("cargo", "c") => {
            cargo::build(out)
        }
        ("cargo", "test") | ("cargo", "t") => cargo::test(out),
        ("docker", "ps") => container::docker_ps(out),
        ("kubectl", "get") => container::kubectl_get(out),
        ("npm", "install") | ("npm", "i") | ("npm", "ci") => node::install(out),
        ("yarn", "install") | ("yarn", "add") | ("yarn", "") => pkg::js_install(out, "yarn"),
        ("pnpm", "install") | ("pnpm", "i") | ("pnpm", "add") | ("pnpm", "") => {
            pkg::js_install(out, "pnpm")
        }
        ("bun", "install") | ("bun", "i") | ("bun", "add") => pkg::js_install(out, "bun"),
        ("pip", "install") | ("pip", "uninstall") | ("pip3", "install") | ("pip3", "uninstall") => {
            pkg::pip(out)
        }
        ("poetry", "install") | ("poetry", "add") | ("poetry", "update") | ("poetry", "remove") => {
            pkg::poetry(out)
        }
        ("dotnet", "build")
        | ("dotnet", "publish")
        | ("dotnet", "pack")
        | ("dotnet", "msbuild") => dotnet::build(out),
        ("dotnet", "test") => dotnet::test(out),
        ("dotnet", "restore") => dotnet::restore(out),
        ("mvn", _) | ("mvnw", _) => java::maven(out),
        ("gradle", _) | ("gradlew", _) => java::gradle(out),
        ("go", "build") | ("go", "install") | ("go", "vet") => golang::build(out),
        ("go", "test") => golang::test(out),
        ("tsc", _) => ts::tsc(out),
        ("eslint", _) => ts::eslint(out),
        ("jest", _) => jstest::jest(out),
        ("vitest", _) => jstest::vitest(out),
        ("az", _) => cloud::az(out),
        ("azd", _) => cloud::azd(out),
        ("gh", _) => cloud::gh(out),
        ("copilot", _) => cloud::copilot(out),
        _ => other(args, out, extreme),
    };

    if !body.ends_with('\n') {
        body.push('\n');
    }
    body
}

/// Summarizes an arbitrary block of text (for example, piped stdin) with the
/// generic compressor. Useful for shrinking large logs or context before
/// pasting them into a prompt. When `extreme` is set, compression is far more
/// aggressive (errors plus a stats footer only).
pub fn summarize_text(text: &str, extreme: bool) -> String {
    let out = Outcome {
        stdout: text.to_string(),
        stderr: String::new(),
        code: 0,
    };
    let mut body = if extreme {
        generic::summarize_extreme(&out)
    } else {
        generic::summarize(&out)
    };
    if !body.ends_with('\n') {
        body.push('\n');
    }
    body
}

/// Handles commands without a dedicated subcommand formatter.
///
/// First looks for a known tool anywhere in the argument vector (so wrapped
/// invocations like `npx eslint` or `node_modules/.bin/jest` are recognized),
/// then sniffs the output for test-runner summaries (so `npm test` / `yarn test`
/// still route to the right formatter), and finally falls back to generic
/// compression.
fn other(args: &[String], out: &Outcome, extreme: bool) -> String {
    for a in args {
        match base_name(a).as_str() {
            "pytest" | "py.test" => return py::pytest(out),
            "tsc" => return ts::tsc(out),
            "eslint" => return ts::eslint(out),
            "jest" => return jstest::jest(out),
            "vitest" => return jstest::vitest(out),
            _ => {}
        }
    }

    // Indirect invocations (e.g. `npm test`) where the runner output is captured
    // but the binary name is not in `args`.
    if out.stdout.contains("Test Suites:") && out.stdout.contains("Tests:") {
        return jstest::jest(out);
    }
    if out.stdout.contains("Test Files ") && out.stdout.contains("Tests ") {
        return jstest::vitest(out);
    }

    if extreme {
        generic::summarize_extreme(out)
    } else {
        generic::summarize(out)
    }
}

/// Returns the argument at `index` as a string slice, if present.
fn arg(args: &[String], index: usize) -> Option<&str> {
    args.get(index).map(String::as_str)
}

/// Normalizes a command into a comparable base name: strips any directory
/// component and a trailing executable/script extension, then lowercases it.
///
/// This lets dispatch match `./gradlew`, `gradlew.bat`, and `/usr/bin/dotnet`
/// the same way as the bare command name.
fn base_name(cmd: &str) -> String {
    let file = cmd.rsplit(['/', '\\']).next().unwrap_or(cmd);
    let stem = match file.rsplit_once('.') {
        Some((stem, "bat" | "cmd" | "exe" | "ps1")) => stem,
        _ => file,
    };
    stem.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::Outcome;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rewrites_git_status_to_porcelain() {
        assert_eq!(
            rewrite(&args(&["git", "status"])),
            args(&["git", "status", "--porcelain=v1", "--branch"])
        );
    }

    #[test]
    fn leaves_unknown_commands_unchanged() {
        let a = args(&["ls", "-la"]);
        assert_eq!(rewrite(&a), a);
    }

    #[test]
    fn summary_always_ends_with_newline() {
        let out = Outcome {
            stdout: "hello".to_string(),
            stderr: String::new(),
            code: 0,
        };
        assert!(summarize(&args(&["echo", "hello"]), &out, false).ends_with('\n'));
    }

    #[test]
    fn detects_pytest_invocation() {
        let out = Outcome {
            stdout: "==================== 1 passed in 0.01s ====================\n".to_string(),
            stderr: String::new(),
            code: 0,
        };
        let summary = summarize(&args(&["python", "-m", "pytest"]), &out, false);
        assert!(summary.contains("1 passed"));
    }

    #[test]
    fn routes_dotnet_test() {
        let out = Outcome {
            stdout: "Passed!  - Failed: 0, Passed: 3, Skipped: 0, Total: 3, Duration: 1 s\n"
                .to_string(),
            stderr: String::new(),
            code: 0,
        };
        let summary = summarize(&args(&["dotnet", "test"]), &out, false);
        assert!(summary.contains("✓ tests:"));
    }

    #[test]
    fn routes_gradle_wrapper_with_path_and_extension() {
        let out = Outcome {
            stdout: "BUILD SUCCESSFUL in 2s\n".to_string(),
            stderr: String::new(),
            code: 0,
        };
        let summary = summarize(&args(&["./gradlew.bat", "test"]), &out, false);
        assert!(summary.contains("✓ gradle: BUILD SUCCESSFUL"));
    }

    #[test]
    fn detects_eslint_via_npx() {
        let out = Outcome {
            stdout: "\n✖ 1 problem (1 error, 0 warnings)\n".to_string(),
            stderr: String::new(),
            code: 1,
        };
        let summary = summarize(&args(&["npx", "eslint", "."]), &out, false);
        assert!(summary.contains("✗ eslint: 1 problem"));
    }

    #[test]
    fn sniffs_jest_from_npm_test_output() {
        let out = Outcome {
            stdout: "Tests:       3 passed, 3 total\nTest Suites: 1 passed, 1 total\n".to_string(),
            stderr: String::new(),
            code: 0,
        };
        let summary = summarize(&args(&["npm", "test"]), &out, false);
        assert!(summary.contains("✓ jest:"));
    }

    #[test]
    fn routes_az_errors() {
        let out = Outcome {
            stdout: String::new(),
            stderr: "ERROR: (AuthorizationFailed) not authorized\n".to_string(),
            code: 1,
        };
        let summary = summarize(&args(&["az", "group", "list"]), &out, false);
        assert!(summary.contains("✗ az: 1 error(s)"));
    }

    #[test]
    fn routes_yarn_install() {
        let out = Outcome {
            stdout: "success Saved lockfile.\nDone in 2.0s.\n".to_string(),
            stderr: String::new(),
            code: 0,
        };
        let summary = summarize(&args(&["yarn", "install"]), &out, false);
        assert!(summary.contains("Done in 2.0s."));
    }

    #[test]
    fn extreme_mode_compresses_more_than_default() {
        let body: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let out = Outcome {
            stdout: body,
            stderr: String::new(),
            code: 0,
        };
        let default = summarize(&args(&["some-tool"]), &out, false);
        let extreme = summarize(&args(&["some-tool"]), &out, true);
        assert!(extreme.lines().count() < default.lines().count());
    }

    #[test]
    fn summarize_text_compresses_long_input() {
        let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
        let summary = summarize_text(&text, false);
        assert!(summary.ends_with('\n'));
        assert!(summary.lines().count() < 200);
        assert!(summary.contains("Σ 200 lines"));
    }
}
