//! `token-saver hook` — GitHub Copilot `postToolUse` hook adapter.
//!
//! Copilot runs `postToolUse` after every tool completes and lets a hook replace
//! the tool's LLM-facing result by writing a `modifiedResult` JSON object to
//! stdout. This adapter reads the hook payload from stdin, and — only for shell
//! tools (`bash`/`powershell`) whose output actually shrinks — returns a
//! compressed result. For anything else it prints `{}` so Copilot keeps the
//! original result untouched.
//!
//! Because `postToolUse` does not support a `matcher`, the shell-tool filtering
//! happens here rather than in the hook configuration.

use std::io::{self, Read};
use std::process::ExitCode;
use std::time::Instant;

use crate::format;
use crate::metrics;

/// Reads the `postToolUse` payload from stdin and prints the hook output JSON.
/// Always exits `0`; on any miss it prints `{}` (keep the original result).
pub fn run(extreme: bool) -> ExitCode {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        println!("{{}}");
        return ExitCode::SUCCESS;
    }
    println!("{}", process(&input, extreme));
    ExitCode::SUCCESS
}

/// Core hook logic: maps a `postToolUse` payload to the JSON Copilot should read.
///
/// Returns `"{}"` (keep the original result) unless the payload is a successful
/// shell-tool result whose LLM text compresses to something strictly smaller.
fn process(input: &str, extreme: bool) -> String {
    let tool = extract_string(input, "toolName").or_else(|| extract_string(input, "tool_name")).unwrap_or_default();
    if tool != "bash" && tool != "powershell" {
        return "{}".to_string();
    }

    let Some(text) = extract_string(input, "textResultForLlm").or_else(|| extract_string(input, "text_result_for_llm"))
    else {
        return "{}".to_string();
    };

    let started = Instant::now();
    let summary = format::summarize_text(&text, extreme);
    let summary = summary.trim_end_matches('\n');
    let elapsed = started.elapsed();
    if summary.is_empty() || summary.len() >= text.len() {
        return "{}".to_string();
    }

    metrics::record("hook", &tool, &text, summary, elapsed);

    format!(
        "{{\"modifiedResult\":{{\"resultType\":\"success\",\"textResultForLlm\":\"{}\"}}}}",
        encode_json_string(summary)
    )
}

/// Finds `"key"` in `json` and returns its decoded JSON string value, if the
/// value is a string. Returns `None` when the key is absent or not a string.
fn extract_string(json: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let after_key = &json[json.find(&needle)? + needle.len()..];
    let bytes = after_key.as_bytes();

    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    i += 1;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b'"' {
        return None;
    }
    i += 1;

    decode_json_string(&after_key[i..])
}

/// Decodes a JSON string body (everything after the opening quote) up to the
/// closing quote, resolving standard escapes including `\uXXXX` surrogate pairs.
fn decode_json_string(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'b' => out.push('\u{0008}'),
                'f' => out.push('\u{000C}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let high = read_hex4(&mut chars)?;
                    if (0xD800..=0xDBFF).contains(&high) {
                        if chars.next()? != '\\' || chars.next()? != 'u' {
                            return None;
                        }
                        let low = read_hex4(&mut chars)?;
                        let code = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);
                        out.push(char::from_u32(code)?);
                    } else if let Some(ch) = char::from_u32(high) {
                        out.push(ch);
                    }
                }
                _ => return None,
            },
            _ => out.push(c),
        }
    }
    None
}

/// Reads exactly four hex digits from `chars` and returns their numeric value.
fn read_hex4(chars: &mut std::str::Chars<'_>) -> Option<u32> {
    let mut code = 0u32;
    for _ in 0..4 {
        code = code * 16 + chars.next()?.to_digit(16)?;
    }
    Some(code)
}

/// Escapes `s` for embedding inside a JSON string literal (quotes not included).
fn encode_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_reads_camel_case_value() {
        let json = r#"{"toolName":"bash","toolResult":{"resultType":"success"}}"#;
        assert_eq!(extract_string(json, "toolName"), Some("bash".to_string()));
    }

    #[test]
    fn extract_decodes_escapes() {
        let json = r#"{"k":"a\nb\t\"c\""}"#;
        assert_eq!(extract_string(json, "k"), Some("a\nb\t\"c\"".to_string()));
    }

    #[test]
    fn extract_returns_none_for_missing_key() {
        assert_eq!(extract_string(r#"{"a":"b"}"#, "missing"), None);
    }

    #[test]
    fn encode_escapes_specials() {
        assert_eq!(encode_json_string("a\"b\nc\\d"), "a\\\"b\\nc\\\\d");
    }

    #[test]
    fn keeps_result_for_non_shell_tools() {
        let payload = r#"{"toolName":"view","toolResult":{"resultType":"success","textResultForLlm":"x"}}"#;
        assert_eq!(process(payload, false), "{}");
    }

    #[test]
    fn keeps_result_when_text_absent() {
        assert_eq!(process(r#"{"toolName":"bash"}"#, false), "{}");
    }

    #[test]
    fn compresses_long_shell_result() {
        std::env::set_var("TOKEN_SAVER_LOG", "off");
        let body: String = (0..60).map(|i| format!("line {i}\\n")).collect();
        let payload = format!(
            "{{\"toolName\":\"bash\",\"toolResult\":{{\"resultType\":\"success\",\"textResultForLlm\":\"{body}\"}}}}"
        );
        let out = process(&payload, false);
        assert!(out.contains("\"modifiedResult\""));
        assert!(out.contains("\"resultType\":\"success\""));
    }

    #[test]
    fn parses_vscode_snake_case_payload() {
        std::env::set_var("TOKEN_SAVER_LOG", "off");
        let body: String = (0..60).map(|i| format!("row {i}\\n")).collect();
        let payload = format!(
            "{{\"tool_name\":\"powershell\",\"tool_result\":{{\"result_type\":\"success\",\"text_result_for_llm\":\"{body}\"}}}}"
        );
        let out = process(&payload, false);
        assert!(out.contains("\"modifiedResult\""));
    }
}
