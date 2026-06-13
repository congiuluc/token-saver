//! End-to-end tests that run the compiled `token-saver` binary.

use std::process::Command;
use std::{env, fs};

/// Path to the binary built for integration tests (provided by Cargo).
const BIN: &str = env!("CARGO_BIN_EXE_token-saver");

/// A trivial cross-platform command that prints a known token to stdout.
#[cfg(windows)]
const ECHO: &[&str] = &["cmd", "/c", "echo", "token-savertoken"];
#[cfg(not(windows))]
const ECHO: &[&str] = &["sh", "-c", "echo token-savertoken"];

#[test]
fn no_args_prints_usage_and_exits_two() {
    let output = Command::new(BIN).output().expect("run token-saver");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("USAGE:"), "stderr was: {stderr}");
}

#[test]
fn help_flag_prints_usage_and_exits_zero() {
    let output = Command::new(BIN).arg("--help").output().expect("run token-saver");
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("USAGE:"), "stderr was: {stderr}");
}

#[test]
fn summarizes_child_stdout() {
    let output = Command::new(BIN).args(ECHO).output().expect("run token-saver");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("token-savertoken"), "stdout was: {stdout}");
}

#[test]
fn raw_flag_passes_child_output_through() {
    let mut args = vec!["--raw"];
    args.extend_from_slice(ECHO);
    let output = Command::new(BIN).args(&args).output().expect("run token-saver");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("token-savertoken"), "stdout was: {stdout}");
}

#[test]
fn reports_failure_to_launch_unknown_program() {
    let output = Command::new(BIN).arg("this-program-does-not-exist-token-saver").output().expect("run token-saver");
    assert_eq!(output.status.code(), Some(127));
}

#[test]
fn tokens_prompt_reports_counts() {
    let output = Command::new(BIN).args(["tokens", "--prompt", "hello world"]).output().expect("run token-saver");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("token-saver — word count"), "stdout was: {stdout}");
    assert!(stdout.contains("source:       prompt"), "stdout was: {stdout}");
    assert!(stdout.contains("words:        2"), "stdout was: {stdout}");
    assert!(stdout.contains("lines:        1"), "stdout was: {stdout}");
    assert!(stdout.contains("L1: 2"), "stdout was: {stdout}");
}

#[test]
fn tokens_file_reports_counts() {
    let temp = env::temp_dir().join("token-saver_tokens_test.txt");
    fs::write(&temp, "file content for token count").expect("write temp file");

    let output =
        Command::new(BIN).args(["tokens", "--file", &temp.to_string_lossy()]).output().expect("run token-saver");

    let _ = fs::remove_file(&temp);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("token-saver — word count"), "stdout was: {stdout}");
    assert!(stdout.contains("source:       file:"), "stdout was: {stdout}");
    assert!(stdout.contains("words:"), "stdout was: {stdout}");
    assert!(stdout.contains("by line:"), "stdout was: {stdout}");
}

#[test]
fn tokens_prompt_reports_line_breakdown() {
    let output = Command::new(BIN).args(["tokens", "--prompt", "one two\n\nthree"]).output().expect("run token-saver");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("words:        3"), "stdout was: {stdout}");
    assert!(stdout.contains("lines:        3"), "stdout was: {stdout}");
    assert!(stdout.contains("L1: 2"), "stdout was: {stdout}");
    assert!(stdout.contains("L2: 0"), "stdout was: {stdout}");
    assert!(stdout.contains("L3: 1"), "stdout was: {stdout}");
}

#[test]
fn gain_reset_clears_metrics_log() {
    let temp = env::temp_dir().join("token-saver_gain_reset_test.jsonl");
    fs::write(&temp, "{\"rawTokens\":10,\"outTokens\":5}\n").expect("write temp metrics");

    let output = Command::new(BIN)
        .args(["gain", "--reset"])
        .env("TOKEN_SAVER_LOG", temp.to_string_lossy().to_string())
        .output()
        .expect("run token-saver");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("reset gain stats"), "stdout was: {stdout}");
    assert!(!temp.exists(), "metrics log should be removed");
}

/// Builds an isolated workspace + home directory and returns their paths.
fn context_fixture(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = env::temp_dir().join(format!("token-saver_ctx_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let workspace = root.join("workspace");
    let home = root.join("home");
    fs::create_dir_all(workspace.join(".github")).expect("create .github");
    fs::create_dir_all(workspace.join(".copilot").join("skills").join("demo")).expect("create skills dir");
    fs::create_dir_all(&home).expect("create home");

    fs::write(
        workspace.join(".github").join("copilot-instructions.md"),
        "Always be concise and helpful in every response.\n",
    )
    .expect("write instructions");
    fs::write(
        workspace.join(".copilot").join("skills").join("demo").join("SKILL.md"),
        "---\nname: demo\ndescription: A demo skill for testing context inventory.\n---\n# Demo\nBody text.\n",
    )
    .expect("write skill");
    (workspace, home)
}

#[test]
fn context_lists_categories_and_summary() {
    let (workspace, home) = context_fixture("list");

    let output = Command::new(BIN)
        .arg("context")
        .arg("--workspace")
        .current_dir(&workspace)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .output()
        .expect("run token-saver");

    let _ = fs::remove_dir_all(workspace.parent().unwrap());

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Copilot context inventory"), "stdout was: {stdout}");
    assert!(stdout.contains("Instructions"), "stdout was: {stdout}");
    assert!(stdout.contains("Skills"), "stdout was: {stdout}");
    assert!(stdout.contains("always-on baseline:"), "stdout was: {stdout}");
}

#[test]
fn context_category_filter_limits_output() {
    let (workspace, home) = context_fixture("filter");

    let output = Command::new(BIN)
        .args(["context", "skills", "--workspace"])
        .current_dir(&workspace)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .output()
        .expect("run token-saver");

    let _ = fs::remove_dir_all(workspace.parent().unwrap());

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Skills"), "stdout was: {stdout}");
    assert!(!stdout.contains("\nInstructions ("), "stdout was: {stdout}");
}

#[test]
fn context_json_flag_emits_json() {
    let (workspace, home) = context_fixture("json");

    let output = Command::new(BIN)
        .args(["ctx", "--workspace", "--json"])
        .current_dir(&workspace)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .output()
        .expect("run token-saver");

    let _ = fs::remove_dir_all(workspace.parent().unwrap());

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim_start().starts_with('{'), "stdout was: {stdout}");
    assert!(stdout.contains("\"items\":["), "stdout was: {stdout}");
    assert!(stdout.contains("\"category\":\"instructions\""), "stdout was: {stdout}");
}

#[test]
fn context_rejects_unknown_category() {
    let output = Command::new(BIN).args(["context", "bogus"]).output().expect("run token-saver");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown category"), "stderr was: {stderr}");
}
