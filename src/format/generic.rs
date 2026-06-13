//! Generic, language-agnostic output compression used as a fallback for any
//! command without a dedicated formatter.

use crate::runner::Outcome;

/// Output up to this many (normalized) lines is returned almost verbatim.
const MAX_LINES: usize = 24;
/// Number of trailing lines preserved when falling back to head/tail truncation.
const TAIL_LINES: usize = 6;
/// Maximum number of notable (error/warning/summary) lines surfaced from long
/// output.
const MAX_SIGNAL: usize = 16;
/// Passthrough threshold in extreme mode — output longer than this is condensed
/// down to errors plus a stats footer only.
const MAX_LINES_EXTREME: usize = 6;
/// Maximum number of error lines surfaced in extreme mode.
const MAX_SIGNAL_EXTREME: usize = 5;

/// Summarizes arbitrary command output, aiming for the smallest message that
/// still carries the meaningful signal.
///
/// After stripping ANSI and merging stderr, output is normalized (blank runs
/// collapsed, consecutive duplicates folded into `(xN)`). Short output is
/// returned as-is. Long output is condensed: if any notable lines are present
/// (errors, warnings, summaries) only those are shown, followed by a one-line
/// stats footer; otherwise a tight head/tail excerpt is returned.
pub fn summarize(out: &Outcome) -> String {
    summarize_mode(out, false)
}

/// Like [`summarize`], but in *extreme* mode: a much smaller passthrough budget
/// and, for long output, only error lines plus a stats footer are surfaced.
pub fn summarize_extreme(out: &Outcome) -> String {
    summarize_mode(out, true)
}

fn summarize_mode(out: &Outcome, extreme: bool) -> String {
    let mut text = strip_ansi(&out.stdout);
    let err = strip_ansi(&out.stderr);

    if !err.trim().is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&err);
    }

    let lines = normalize(&text);
    let max = if extreme {
        MAX_LINES_EXTREME
    } else {
        MAX_LINES
    };
    let mut body = if lines.len() <= max {
        lines.join("\n")
    } else {
        condense(&lines, extreme)
    };

    if out.code != 0 {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(&format!("! exit {}", out.code));
    }
    body
}

/// Condenses long output into the smallest meaningful message.
fn condense(lines: &[String], extreme: bool) -> String {
    let errors = lines.iter().filter(|l| is_error(l)).count();
    let warnings = lines.iter().filter(|l| is_warning(l)).count();

    if extreme {
        // Extreme mode: surface only errors (capped), then a stats footer.
        let errs: Vec<&String> = lines.iter().filter(|l| is_error(l)).collect();
        let shown = errs.len().min(MAX_SIGNAL_EXTREME);
        let mut out: Vec<String> = errs.iter().take(shown).map(|l| (*l).clone()).collect();
        if errs.len() > shown {
            out.push(format!("… {} more errors …", errs.len() - shown));
        }
        out.push(stats_footer(lines.len(), errors, warnings));
        return out.join("\n");
    }

    let signal: Vec<&String> = lines.iter().filter(|l| is_significant(l)).collect();

    if !signal.is_empty() {
        let shown = signal.len().min(MAX_SIGNAL);
        let mut out: Vec<String> = signal.iter().take(shown).map(|l| (*l).clone()).collect();
        if signal.len() > shown {
            out.push(format!("… {} more notable lines …", signal.len() - shown));
        }
        out.push(stats_footer(lines.len(), errors, warnings));
        return out.join("\n");
    }

    // No notable lines: keep a tight head/tail excerpt with a stats footer.
    let budget = MAX_LINES.saturating_sub(TAIL_LINES + 2);
    let omitted = lines.len() - budget - TAIL_LINES;
    let mut out: Vec<String> = Vec::with_capacity(MAX_LINES);
    out.extend(lines[..budget].iter().cloned());
    out.push(format!("… {omitted} lines omitted …"));
    out.extend(lines[lines.len() - TAIL_LINES..].iter().cloned());
    out.push(stats_footer(lines.len(), errors, warnings));
    out.join("\n")
}

/// Builds the compact stats footer, omitting zeroed counts.
fn stats_footer(total: usize, errors: usize, warnings: usize) -> String {
    let mut footer = format!("Σ {total} lines");
    if errors > 0 {
        footer.push_str(&format!(" · {errors} err"));
    }
    if warnings > 0 {
        footer.push_str(&format!(" · {warnings} warn"));
    }
    footer
}

/// Returns `true` when a line carries error-level signal.
fn is_error(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    [
        "error",
        "fatal",
        "panic",
        "exception",
        "traceback",
        "failed",
        "failure",
        "cannot",
        "denied",
        "no such",
        "not found",
        "unable to",
        "✗",
    ]
    .iter()
    .any(|kw| l.contains(kw))
}

/// Returns `true` when a line carries warning-level signal.
fn is_warning(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    ["warning", "warn:", "warn ", "deprecat"]
        .iter()
        .any(|kw| l.contains(kw))
}

/// Returns `true` when a line looks like a result/summary worth keeping.
fn is_summary(line: &str) -> bool {
    let l = line.to_ascii_lowercase();
    [
        "passed",
        "failed",
        " tests",
        "test result",
        "files changed",
        "vulnerabilit",
        "added ",
        "removed ",
        "success",
        "completed",
    ]
    .iter()
    .any(|kw| l.contains(kw))
}

/// A line is significant if it carries error, warning, or summary signal.
fn is_significant(line: &str) -> bool {
    is_error(line) || is_warning(line) || is_summary(line)
}

/// Strips ANSI, collapses blank runs, trims surrounding blanks, and folds
/// consecutive duplicate lines into a single `(xN)`-annotated entry.
fn normalize(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut prev_blank = false;
    for raw in text.lines() {
        let trimmed = raw.trim_end().to_string();
        let blank = trimmed.is_empty();
        if blank && prev_blank {
            continue;
        }
        prev_blank = blank;
        lines.push(trimmed);
    }
    while lines.first().is_some_and(String::is_empty) {
        lines.remove(0);
    }
    while lines.last().is_some_and(String::is_empty) {
        lines.pop();
    }

    let mut deduped: Vec<(String, usize)> = Vec::new();
    for line in lines {
        match deduped.last_mut() {
            Some(last) if last.0 == line => last.1 += 1,
            _ => deduped.push((line, 1)),
        }
    }
    deduped
        .into_iter()
        .map(|(line, count)| {
            if count > 1 {
                format!("{line}  (x{count})")
            } else {
                line
            }
        })
        .collect()
}

/// Removes ANSI/VT escape sequences (CSI and OSC) from `input`.
pub fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            // CSI sequence: ESC [ ... <final byte 0x40-0x7e>
            Some('[') => {
                chars.next();
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
            // OSC sequence: ESC ] ... terminated by BEL or ESC \
            Some(']') => {
                chars.next();
                while let Some(&n) = chars.peek() {
                    chars.next();
                    if n == '\u{07}' || n == '\u{1b}' {
                        break;
                    }
                }
            }
            // Any other escape: drop the ESC introducer only.
            _ => {}
        }
    }
    out
}

/// Collapses blank-line runs, deduplicates consecutive identical lines, and
/// truncates the result to at most `max_lines` (keeping `tail` lines at the end).
///
/// Used by formatters whose output is already compact (e.g. `git log`).
pub fn compress(text: &str, max_lines: usize, tail: usize) -> String {
    let rendered = normalize(text);

    if rendered.len() <= max_lines {
        return rendered.join("\n");
    }

    // Reserve one line for the "… omitted …" marker so the result never
    // exceeds `max_lines` lines in total.
    let head = max_lines.saturating_sub(tail + 1);
    let omitted = rendered.len() - head - tail;
    let mut result: Vec<String> = Vec::with_capacity(max_lines + 1);
    result.extend(rendered[..head].iter().cloned());
    result.push(format!("… {omitted} lines omitted …"));
    result.extend(rendered[rendered.len() - tail..].iter().cloned());
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_csi_color_codes() {
        let input = "\u{1b}[31mred\u{1b}[0m text";
        assert_eq!(strip_ansi(input), "red text");
    }

    #[test]
    fn collapses_blank_runs_and_dedupes() {
        let input = "a\n\n\n\na\na\nb\n";
        assert_eq!(compress(input, 40, 12), "a\n\na  (x2)\nb");
    }

    #[test]
    fn truncates_long_output() {
        let input: String = (0..100).map(|n| format!("line{n}\n")).collect();
        let out = compress(&input, 10, 3);
        assert!(out.contains("lines omitted"));
        assert!(out.lines().count() <= 10);
        assert!(out.contains("line99"));
    }

    fn outcome(stdout: &str, code: i32) -> Outcome {
        Outcome {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code,
        }
    }

    #[test]
    fn short_output_passes_through() {
        let summary = summarize(&outcome("hello\nworld\n", 0));
        assert_eq!(summary, "hello\nworld");
    }

    #[test]
    fn long_output_surfaces_only_notable_lines() {
        let mut text = String::new();
        for n in 0..200 {
            text.push_str(&format!("processing item {n}\n"));
        }
        text.push_str("error: disk full\n");
        text.push_str("warning: retrying\n");
        let summary = summarize(&outcome(&text, 1));

        // Noise is dropped; only the notable lines plus footers remain.
        assert!(!summary.contains("processing item 5"));
        assert!(summary.contains("error: disk full"));
        assert!(summary.contains("warning: retrying"));
        assert!(summary.contains("Σ 202 lines · 1 err · 1 warn"));
        assert!(summary.contains("! exit 1"));
        assert!(summary.lines().count() < 10);
    }

    #[test]
    fn long_output_without_signal_uses_head_tail() {
        let input: String = (0..100).map(|n| format!("row {n}\n")).collect();
        let summary = summarize(&outcome(&input, 0));
        assert!(summary.contains("row 0"));
        assert!(summary.contains("row 99"));
        assert!(summary.contains("lines omitted"));
        assert!(summary.contains("Σ 100 lines"));
        assert!(summary.lines().count() <= MAX_LINES);
    }
}
