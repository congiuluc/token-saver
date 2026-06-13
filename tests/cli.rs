//! End-to-end tests that run the compiled `tokensaver` binary.

use std::process::Command;
use std::{env, fs};

/// Path to the binary built for integration tests (provided by Cargo).
const BIN: &str = env!("CARGO_BIN_EXE_tokensaver");

/// A trivial cross-platform command that prints a known token to stdout.
#[cfg(windows)]
const ECHO: &[&str] = &["cmd", "/c", "echo", "tokensavertoken"];
#[cfg(not(windows))]
const ECHO: &[&str] = &["sh", "-c", "echo tokensavertoken"];

#[test]
fn no_args_prints_usage_and_exits_two() {
    let output = Command::new(BIN).output().expect("run tokensaver");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("USAGE:"), "stderr was: {stderr}");
}

#[test]
fn help_flag_prints_usage_and_exits_zero() {
    let output = Command::new(BIN).arg("--help").output().expect("run tokensaver");
    assert_eq!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("USAGE:"), "stderr was: {stderr}");
}

#[test]
fn summarizes_child_stdout() {
    let output = Command::new(BIN).args(ECHO).output().expect("run tokensaver");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tokensavertoken"), "stdout was: {stdout}");
}

#[test]
fn raw_flag_passes_child_output_through() {
    let mut args = vec!["--raw"];
    args.extend_from_slice(ECHO);
    let output = Command::new(BIN).args(&args).output().expect("run tokensaver");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tokensavertoken"), "stdout was: {stdout}");
}

#[test]
fn reports_failure_to_launch_unknown_program() {
    let output = Command::new(BIN)
        .arg("this-program-does-not-exist-tokensaver")
        .output()
        .expect("run tokensaver");
    assert_eq!(output.status.code(), Some(127));
}

#[test]
fn tokens_prompt_reports_counts() {
    let output = Command::new(BIN)
        .args(["tokens", "--prompt", "hello world"])
        .output()
        .expect("run tokensaver");
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tokensaver — word count"), "stdout was: {stdout}");
    assert!(stdout.contains("source:       prompt"), "stdout was: {stdout}");
    assert!(stdout.contains("words:        2"), "stdout was: {stdout}");
    assert!(stdout.contains("lines:        1"), "stdout was: {stdout}");
    assert!(stdout.contains("L1: 2"), "stdout was: {stdout}");
}

#[test]
fn tokens_file_reports_counts() {
    let temp = env::temp_dir().join("tokensaver_tokens_test.txt");
    fs::write(&temp, "file content for token count").expect("write temp file");

    let output = Command::new(BIN)
        .args(["tokens", "--file", &temp.to_string_lossy()])
        .output()
        .expect("run tokensaver");

    let _ = fs::remove_file(&temp);

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("tokensaver — word count"), "stdout was: {stdout}");
    assert!(stdout.contains("source:       file:"), "stdout was: {stdout}");
    assert!(stdout.contains("words:"), "stdout was: {stdout}");
    assert!(stdout.contains("by line:"), "stdout was: {stdout}");
}

#[test]
fn tokens_prompt_reports_line_breakdown() {
    let output = Command::new(BIN)
        .args(["tokens", "--prompt", "one two\n\nthree"])
        .output()
        .expect("run tokensaver");

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
    let temp = env::temp_dir().join("tokensaver_gain_reset_test.jsonl");
    fs::write(&temp, "{\"rawTokens\":10,\"outTokens\":5}\n").expect("write temp metrics");

    let output = Command::new(BIN)
        .args(["gain", "--reset"])
        .env("TOKENSAVER_LOG", temp.to_string_lossy().to_string())
        .output()
        .expect("run tokensaver");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("reset gain stats"), "stdout was: {stdout}");
    assert!(!temp.exists(), "metrics log should be removed");
}
