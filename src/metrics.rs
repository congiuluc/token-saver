//! Token accounting: estimates token counts and appends per-invocation usage
//! records to a log file, plus reads them back for `tokensaver gain`.
//!
//! Primary token counts are selected by `TOKENSAVER_TOKENIZER` and can be near-real
//! with a model tokenizer backend. The log also stores heuristic and model
//! counts side by side for comparison. Logging is enabled by default to
//! `~/.tokensaver/metrics.jsonl`; set `TOKENSAVER_LOG` to a path to redirect it, or to
//! `off`, `0`, or empty to disable it. Logging failures are swallowed so they
//! never break a command.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Aggregated token totals read back from the metrics log.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Number of recorded invocations.
    pub count: u64,
    /// Total character count across all raw (original) outputs.
    pub raw_chars: u64,
    /// Total character count across all tokensaver outputs.
    pub out_chars: u64,
    /// Estimated tokens across all raw (original) inputs.
    pub raw_tokens: u64,
    /// Estimated tokens across all tokensaver outputs.
    pub out_tokens: u64,
    /// Heuristic token totals across all raw inputs.
    pub raw_tokens_heuristic: u64,
    /// Heuristic token totals across all tokensaver outputs.
    pub out_tokens_heuristic: u64,
    /// Model tokenizer totals across all raw inputs.
    pub raw_tokens_model: u64,
    /// Model tokenizer totals across all tokensaver outputs.
    pub out_tokens_model: u64,
    /// Number of records that included model tokenizer counts.
    pub model_token_samples: u64,
}

/// Result of resetting the gain log.
#[derive(Debug, PartialEq, Eq)]
pub enum ResetOutcome {
    /// Logging is disabled via `TOKENSAVER_LOG`.
    Disabled,
    /// The metrics log existed and was removed.
    Cleared(PathBuf),
    /// The metrics log did not exist, so totals were already empty.
    AlreadyEmpty(PathBuf),
}

/// Appends one usage record (raw vs. tokensaver token estimates) to the log
/// and forwards the event to the OpenTelemetry exporter.
///
/// `mode` is the invocation kind (`run`, `stdin`, `hook`), `command` is the
/// user-facing command string, `raw` is the original output, `out` the
/// tokensaver form and `duration` the wall-clock run time. Skips the log
/// file when logging is disabled (OpenTelemetry export still runs), and silently
/// ignores any I/O error so the primary command is never affected.
pub fn record(mode: &str, command: &str, raw: &str, out: &str, duration: Duration) {
    let raw_estimate = crate::tokenizer::estimate(raw);
    let out_estimate = crate::tokenizer::estimate(out);

    let raw_chars = raw.chars().count() as u64;
    let out_chars = out.chars().count() as u64;
    let raw_tokens = crate::tokenizer::select_active(raw_estimate);
    let out_tokens = crate::tokenizer::select_active(out_estimate);
    let raw_bytes = raw.len() as u64;
    let out_bytes = out.len() as u64;

    let raw_tokens_heuristic = raw_estimate.heuristic;
    let out_tokens_heuristic = out_estimate.heuristic;
    let raw_tokens_model = raw_estimate.model.unwrap_or(0);
    let out_tokens_model = out_estimate.model.unwrap_or(0);
    let model_tokens_present = u64::from(raw_estimate.model.is_some() && out_estimate.model.is_some());

    if let Some(path) = log_path() {
        let line = format!(
            "{{\"ts\":{},\"mode\":\"{}\",\"cmd\":\"{}\",\"tokenizer\":\"{}\",\"modelTokensPresent\":{},\"rawChars\":{},\"outChars\":{},\"rawTokens\":{},\"outTokens\":{},\"rawTokensHeuristic\":{},\"outTokensHeuristic\":{},\"rawTokensModel\":{},\"outTokensModel\":{},\"rawBytes\":{},\"outBytes\":{}}}\n",
            now_ms(),
            escape(mode),
            escape(command),
            crate::tokenizer::active_mode().label(),
            model_tokens_present,
            raw_chars,
            out_chars,
            raw_tokens,
            out_tokens,
            raw_tokens_heuristic,
            out_tokens_heuristic,
            raw_tokens_model,
            out_tokens_model,
            raw_bytes,
            out_bytes,
        );
        let _ = append(&path, &line);
    }

    crate::otel::export(&crate::otel::Span { mode, command, raw_tokens, out_tokens, raw_bytes, out_bytes, duration });
}

/// Reads the metrics log and returns the aggregated token totals. Returns empty
/// totals when logging is disabled or the log does not yet exist.
pub fn read_totals() -> Totals {
    let Some(path) = log_path() else {
        return Totals::default();
    };
    let contents = fs::read_to_string(&path).unwrap_or_default();
    sum_totals(&contents)
}

/// Resets persisted gain stats by removing the metrics log file.
pub fn reset_log() -> std::io::Result<ResetOutcome> {
    let Some(path) = log_path() else {
        return Ok(ResetOutcome::Disabled);
    };

    if !path.exists() {
        return Ok(ResetOutcome::AlreadyEmpty(path));
    }

    match fs::remove_file(&path) {
        Ok(()) => Ok(ResetOutcome::Cleared(path)),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(ResetOutcome::AlreadyEmpty(path)),
        Err(err) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            // Fall back to truncation if removal fails for an existing file.
            OpenOptions::new().write(true).truncate(true).open(&path).map_err(|truncate_err| {
                Error::new(
                    truncate_err.kind(),
                    format!("remove failed: {err}; truncate fallback failed: {truncate_err}"),
                )
            })?;
            Ok(ResetOutcome::Cleared(path))
        }
    }
}

/// Sums the `rawTokens`/`outTokens` fields across every well-formed JSONL
/// record in `contents`. Lines missing either field are skipped.
fn sum_totals(contents: &str) -> Totals {
    let mut totals = Totals::default();
    for line in contents.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let (Some(raw), Some(out)) = (extract_u64(line, "rawTokens"), extract_u64(line, "outTokens")) {
            // Prefer explicit char counts when present; fall back to byte counts
            // for compatibility with older log entries.
            let raw_chars = extract_u64(line, "rawChars").or_else(|| extract_u64(line, "rawBytes")).unwrap_or(0);
            let out_chars = extract_u64(line, "outChars").or_else(|| extract_u64(line, "outBytes")).unwrap_or(0);

            let raw_tokens_heuristic = extract_u64(line, "rawTokensHeuristic").unwrap_or(raw);
            let out_tokens_heuristic = extract_u64(line, "outTokensHeuristic").unwrap_or(out);

            let model_tokens_present = extract_u64(line, "modelTokensPresent").map(|v| v == 1).unwrap_or(false);
            let raw_tokens_model = extract_u64(line, "rawTokensModel").unwrap_or(0);
            let out_tokens_model = extract_u64(line, "outTokensModel").unwrap_or(0);

            totals.count += 1;
            totals.raw_chars += raw_chars;
            totals.out_chars += out_chars;
            totals.raw_tokens += raw;
            totals.out_tokens += out;
            totals.raw_tokens_heuristic += raw_tokens_heuristic;
            totals.out_tokens_heuristic += out_tokens_heuristic;
            if model_tokens_present {
                totals.raw_tokens_model += raw_tokens_model;
                totals.out_tokens_model += out_tokens_model;
                totals.model_token_samples += 1;
            }
        }
    }
    totals
}

/// Extracts the unsigned integer value of `"key":` from a JSONL record line.
fn extract_u64(line: &str, key: &str) -> Option<u64> {
    let needle = format!("\"{key}\":");
    let rest = line[line.find(&needle)? + needle.len()..].trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Resolves the active log path, honoring the `TOKENSAVER_LOG` override and the
/// disable sentinels (`off`, `0`, or empty). Returns `None` when disabled.
fn log_path() -> Option<PathBuf> {
    match env::var("TOKENSAVER_LOG") {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("off") || trimmed == "0" {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        }
        Err(_) => home_dir().map(|home| home.join(".tokensaver").join("metrics.jsonl")),
    }
}

/// Appends `line` to `path`, creating the file and parent directory as needed.
fn append(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)?.write_all(line.as_bytes())
}

/// Returns the current Unix time in milliseconds, or `0` if the clock is before
/// the epoch.
fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// Returns the user's home directory, honoring `USERPROFILE` then `HOME`.
fn home_dir() -> Option<PathBuf> {
    env::var_os("USERPROFILE").or_else(|| env::var_os("HOME")).map(PathBuf::from)
}

/// Escapes a string for embedding inside a JSON string literal.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn unique_temp_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        env::temp_dir().join(format!("tokensaver-{name}-{nonce}.jsonl"))
    }

    #[test]
    fn escapes_json_specials() {
        assert_eq!(escape("a\"b\\c\nd"), "a\\\"b\\\\c\\nd");
    }

    #[test]
    fn extracts_integer_field() {
        let line = r#"{"ts":1,"rawTokens":120,"outTokens":30}"#;
        assert_eq!(extract_u64(line, "rawTokens"), Some(120));
        assert_eq!(extract_u64(line, "outTokens"), Some(30));
        assert_eq!(extract_u64(line, "missing"), None);
    }

    #[test]
    fn sums_totals_across_records() {
        let contents = concat!(
            "{\"rawTokens\":100,\"outTokens\":20}\n",
            "\n",
            "{\"rawTokens\":50,\"outTokens\":10}\n",
            "garbage line\n",
        );
        let totals = sum_totals(contents);
        assert_eq!(
            totals,
            Totals {
                count: 2,
                raw_chars: 0,
                out_chars: 0,
                raw_tokens: 150,
                out_tokens: 30,
                raw_tokens_heuristic: 150,
                out_tokens_heuristic: 30,
                raw_tokens_model: 0,
                out_tokens_model: 0,
                model_token_samples: 0,
            }
        );
    }

    #[test]
    fn sums_char_totals_with_legacy_byte_fallback() {
        let contents = concat!(
            "{\"rawChars\":100,\"outChars\":20,\"rawTokens\":25,\"outTokens\":5}\n",
            "{\"rawBytes\":80,\"outBytes\":16,\"rawTokens\":20,\"outTokens\":4}\n",
        );
        let totals = sum_totals(contents);
        assert_eq!(
            totals,
            Totals {
                count: 2,
                raw_chars: 180,
                out_chars: 36,
                raw_tokens: 45,
                out_tokens: 9,
                raw_tokens_heuristic: 45,
                out_tokens_heuristic: 9,
                raw_tokens_model: 0,
                out_tokens_model: 0,
                model_token_samples: 0,
            }
        );
    }

    #[test]
    fn sums_model_token_totals_when_present() {
        let contents = concat!(
            "{\"modelTokensPresent\":1,\"rawTokens\":10,\"outTokens\":4,\"rawTokensHeuristic\":12,\"outTokensHeuristic\":5,\"rawTokensModel\":9,\"outTokensModel\":3}\n",
            "{\"modelTokensPresent\":0,\"rawTokens\":8,\"outTokens\":2,\"rawTokensHeuristic\":8,\"outTokensHeuristic\":2,\"rawTokensModel\":0,\"outTokensModel\":0}\n",
        );
        let totals = sum_totals(contents);
        assert_eq!(totals.raw_tokens, 18);
        assert_eq!(totals.out_tokens, 6);
        assert_eq!(totals.raw_tokens_heuristic, 20);
        assert_eq!(totals.out_tokens_heuristic, 7);
        assert_eq!(totals.raw_tokens_model, 9);
        assert_eq!(totals.out_tokens_model, 3);
        assert_eq!(totals.model_token_samples, 1);
    }

    #[test]
    fn reset_log_reports_disabled_when_logging_is_off() {
        let _guard = env_lock().lock().expect("env lock");
        unsafe { env::set_var("TOKENSAVER_LOG", "off") };
        let outcome = reset_log().expect("reset should succeed");
        assert_eq!(outcome, ResetOutcome::Disabled);
        unsafe { env::remove_var("TOKENSAVER_LOG") };
    }

    #[test]
    fn reset_log_clears_existing_log() {
        let _guard = env_lock().lock().expect("env lock");
        let path = unique_temp_path("reset-existing");
        unsafe { env::set_var("TOKENSAVER_LOG", &path) };
        fs::write(&path, "{\"rawTokens\":10,\"outTokens\":5}\n").expect("write test log");

        let outcome = reset_log().expect("reset should succeed");
        assert_eq!(outcome, ResetOutcome::Cleared(path.clone()));
        assert!(!path.exists(), "log file should be removed");

        unsafe { env::remove_var("TOKENSAVER_LOG") };
    }

    #[test]
    fn reset_log_reports_already_empty_for_missing_log() {
        let _guard = env_lock().lock().expect("env lock");
        let path = unique_temp_path("reset-missing");
        let _ = fs::remove_file(&path);
        unsafe { env::set_var("TOKENSAVER_LOG", &path) };

        let outcome = reset_log().expect("reset should succeed");
        assert_eq!(outcome, ResetOutcome::AlreadyEmpty(path.clone()));

        unsafe { env::remove_var("TOKENSAVER_LOG") };
    }
}
