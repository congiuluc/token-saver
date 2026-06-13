//! `token-saver optimize` — losslessly compact the *text* of a file to cut its
//! token cost, and report how many tokens the change saves.
//!
//! The optimization is deterministic and meaning-preserving (no model calls): it
//! normalizes line endings, strips trailing whitespace, collapses repeated inner
//! whitespace and runs of blank lines, and trims leading/trailing blank lines.
//!
//! With `--preview` the optimized text and a before/after token summary are
//! printed and **nothing is written**. Without it, the optimized text is written
//! back to the file (or to `--out`) and the realized saving is recorded so it
//! shows up in `token-saver gain`.

use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;
use std::time::Instant;

use crate::metrics;
use crate::tokenizer;

/// Parsed options for the `optimize` subcommand.
struct Options {
    text: String,
    source: String,
    /// Path to rewrite in place when applying (the file the text came from).
    file_path: Option<String>,
    /// Explicit output path; overrides in-place rewrite.
    out_path: Option<String>,
    preview: bool,
    json: bool,
}

/// Runs the `optimize` subcommand.
pub fn run(args: &[String]) -> ExitCode {
    let opts = match parse_args(args) {
        Ok(opts) => opts,
        Err(msg) => {
            if !msg.is_empty() {
                eprintln!("token-saver: {msg}");
            }
            eprintln!(
                "usage: token-saver optimize (--file <path> | --stdin | --prompt <text>) \
                 [--preview] [--out <path>] [--json]"
            );
            return ExitCode::from(2);
        }
    };

    let started = Instant::now();
    let optimized = optimize_text(&opts.text);

    let before_tokens = tokens_of(&opts.text);
    let after_tokens = tokens_of(&optimized);
    let saved_tokens = before_tokens.saturating_sub(after_tokens);
    let before_chars = opts.text.chars().count();
    let after_chars = optimized.chars().count();
    let before_lines = line_count(&opts.text);
    let after_lines = line_count(&optimized);

    // Decide whether and where to write. Preview never writes.
    let write_target: Option<String> =
        if opts.preview { None } else { opts.out_path.clone().or_else(|| opts.file_path.clone()) };

    let mut written: Option<String> = None;
    if let Some(target) = &write_target {
        if optimized == opts.text {
            // Nothing changed — avoid a pointless rewrite.
        } else if let Err(err) = fs::write(target, &optimized) {
            eprintln!("token-saver: failed to write '{target}': {err}");
            return ExitCode::FAILURE;
        } else {
            written = Some(target.clone());
            metrics::record("optimize", &opts.source, &opts.text, &optimized, started.elapsed());
        }
    }

    let saved_pct = percent(before_tokens, saved_tokens);

    if opts.json {
        println!(
            "{{\"source\":\"{}\",\"preview\":{},\"beforeTokens\":{},\"afterTokens\":{},\
             \"savedTokens\":{},\"savedPct\":{:.1},\"beforeChars\":{},\"afterChars\":{},\
             \"beforeLines\":{},\"afterLines\":{},\"written\":{}}}",
            escape_json(&opts.source),
            opts.preview,
            before_tokens,
            after_tokens,
            saved_tokens,
            saved_pct,
            before_chars,
            after_chars,
            before_lines,
            after_lines,
            written.is_some(),
        );
        return ExitCode::SUCCESS;
    }

    if opts.preview {
        // Show the optimized text, then the before/after token summary.
        println!("token-saver — optimize preview ({})", opts.source);
        println!("──── optimized text ────");
        print!("{optimized}");
        if !optimized.ends_with('\n') {
            println!();
        }
        println!("──── summary ────");
    } else if let Some(path) = &written {
        println!("token-saver — optimized {} → {path}", opts.source);
    } else if write_target.is_some() {
        println!("token-saver — already optimal ({})", opts.source);
    } else {
        // No file to write (stdin/prompt without --out): act as a filter and emit
        // the optimized text to stdout, with the summary going to stderr below.
        print!("{optimized}");
        if !optimized.ends_with('\n') {
            println!();
        }
    }

    let summary = format!(
        "  tokenizer:     {}\n\
         \x20 before tokens: {}\n\
         \x20 after tokens:  {}\n\
         \x20 saved:         {} ({:.1}%)\n\
         \x20 before chars:  {}\n\
         \x20 after chars:   {}\n\
         \x20 lines:         {} → {}",
        tokenizer::active_mode().label(),
        before_tokens,
        after_tokens,
        saved_tokens,
        saved_pct,
        before_chars,
        after_chars,
        before_lines,
        after_lines,
    );

    // For the stdin/prompt filter case the optimized text already went to stdout,
    // so keep the summary on stderr to avoid corrupting a piped result.
    let summary_to_stderr = !opts.preview && write_target.is_none();
    if summary_to_stderr {
        eprintln!("token-saver — optimize summary ({})", opts.source);
        eprintln!("{summary}");
    } else {
        println!("{summary}");
    }

    if opts.preview {
        println!("Preview only — no changes written. Re-run without --preview to apply.");
    }

    ExitCode::SUCCESS
}

/// Parses CLI arguments into [`Options`]. Returns an error message (possibly
/// empty) on failure so the caller can print usage.
fn parse_args(args: &[String]) -> Result<Options, String> {
    if args.is_empty() {
        return Err("optimize requires a source: --file, --stdin, or --prompt".to_string());
    }

    let mut text: Option<String> = None;
    let mut source = String::new();
    let mut file_path: Option<String> = None;
    let mut out_path: Option<String> = None;
    let mut preview = false;
    let mut json = false;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--preview" | "-n" | "--dry-run" => {
                preview = true;
                i += 1;
            }
            "--json" => {
                json = true;
                i += 1;
            }
            "--out" | "-o" => {
                if i + 1 >= args.len() {
                    return Err("--out requires a path".to_string());
                }
                out_path = Some(args[i + 1].clone());
                i += 2;
            }
            "--file" | "-f" => {
                if i + 1 >= args.len() {
                    return Err("--file requires a path".to_string());
                }
                if text.is_some() {
                    return Err("use only one source: --file, --stdin, or --prompt".to_string());
                }
                let path = &args[i + 1];
                match fs::read_to_string(path) {
                    Ok(content) => {
                        text = Some(content);
                        source = format!("file:{path}");
                        file_path = Some(path.clone());
                    }
                    Err(err) => {
                        return Err(format!("failed to read file '{path}': {err}"));
                    }
                }
                i += 2;
            }
            "--prompt" | "-p" => {
                if i + 1 >= args.len() {
                    return Err("--prompt requires a value".to_string());
                }
                if text.is_some() {
                    return Err("use only one source: --file, --stdin, or --prompt".to_string());
                }
                text = Some(args[i + 1].clone());
                source = "prompt".to_string();
                i += 2;
            }
            "--stdin" | "-" => {
                if text.is_some() {
                    return Err("use only one source: --file, --stdin, or --prompt".to_string());
                }
                let mut stdin_text = String::new();
                if let Err(err) = io::stdin().read_to_string(&mut stdin_text) {
                    return Err(format!("failed to read stdin: {err}"));
                }
                text = Some(stdin_text);
                source = "stdin".to_string();
                i += 1;
            }
            unknown => {
                return Err(format!("unknown optimize option '{unknown}'"));
            }
        }
    }

    let Some(text) = text else {
        return Err("optimize requires a source: --file, --stdin, or --prompt".to_string());
    };

    Ok(Options { text, source, file_path, out_path, preview, json })
}

/// Deterministically compacts `input` while preserving its meaning:
/// normalizes line endings to `\n`, strips trailing whitespace, collapses inner
/// whitespace runs (keeping leading indentation), collapses consecutive blank
/// lines to one, and trims leading/trailing blank lines. The result ends with a
/// single trailing newline unless it is empty.
pub fn optimize_text(input: &str) -> String {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");

    let mut lines: Vec<String> = Vec::new();
    let mut blank_run = 0usize;
    for raw in normalized.split('\n') {
        let collapsed = collapse_inner_whitespace(raw.trim_end());
        if collapsed.is_empty() {
            blank_run += 1;
            if blank_run <= 1 {
                lines.push(String::new());
            }
        } else {
            blank_run = 0;
            lines.push(collapsed);
        }
    }

    while lines.first().is_some_and(|l| l.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }

    let mut result = lines.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    result
}

/// Collapses runs of spaces/tabs inside `line` to a single space while keeping
/// any leading indentation intact. Assumes trailing whitespace is already gone.
fn collapse_inner_whitespace(line: &str) -> String {
    let indent_len = line.len() - line.trim_start().len();
    let (indent, body) = line.split_at(indent_len);

    let mut out = String::with_capacity(line.len());
    out.push_str(indent);
    let mut prev_space = false;
    for ch in body.chars() {
        if ch == ' ' || ch == '\t' {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

/// Active-tokenizer token count for `text`.
fn tokens_of(text: &str) -> u64 {
    tokenizer::select_active(tokenizer::estimate(text))
}

/// Number of logical lines in `text` (0 for empty input).
fn line_count(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.lines().count()
    }
}

/// Saved-token percentage relative to the original count.
fn percent(before: u64, saved: u64) -> f64 {
    if before > 0 {
        saved as f64 / before as f64 * 100.0
    } else {
        0.0
    }
}

/// Minimal JSON string escaping for the `--json` output.
fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_trailing_whitespace_and_normalizes_endings() {
        let input = "alpha   \r\nbeta\t\r\n";
        assert_eq!(optimize_text(input), "alpha\nbeta\n");
    }

    #[test]
    fn collapses_inner_whitespace_keeps_indentation() {
        let input = "    foo     bar    baz";
        assert_eq!(optimize_text(input), "    foo bar baz\n");
    }

    #[test]
    fn collapses_blank_line_runs_to_one() {
        let input = "a\n\n\n\nb\n";
        assert_eq!(optimize_text(input), "a\n\nb\n");
    }

    #[test]
    fn trims_leading_and_trailing_blank_lines() {
        let input = "\n\nhello  \n\n\n";
        assert_eq!(optimize_text(input), "hello\n");
    }

    #[test]
    fn empty_input_stays_empty() {
        assert_eq!(optimize_text(""), "");
        assert_eq!(optimize_text("   \n  \n"), "");
    }

    #[test]
    fn already_optimal_is_idempotent() {
        let once = optimize_text("a b\n\nc d\n");
        assert_eq!(once, "a b\n\nc d\n");
        assert_eq!(optimize_text(&once), once);
    }

    #[test]
    fn line_count_counts_logical_lines() {
        assert_eq!(line_count(""), 0);
        assert_eq!(line_count("one"), 1);
        assert_eq!(line_count("one\ntwo\n"), 2);
    }
}
