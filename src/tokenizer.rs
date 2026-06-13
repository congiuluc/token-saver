//! Token counting backends used by metrics.
//!
//! `TOKEN_SAVER_TOKENIZER` selects the active tokenizer mode:
//! - `gpt5` (default): OpenAI-like BPE using `o200k` encoding
//! - `o200k`: OpenAI-like BPE (near-real for GPT-4o/GPT-5 style encodings)
//! - `cl100k`: OpenAI-like BPE (near-real for GPT-4/3.5 style encodings)
//! - `heuristic`: approximate with ceil(chars/4)
//!
//! The active mode decides the primary `rawTokens`/`outTokens` totals used by
//! `token-saver gain`. Heuristic and model counts are also computed separately so the
//! report can display both side by side.

use tiktoken_rs::{cl100k_base, o200k_base};

/// Pluggable token counting backend.
pub trait TokenCounter {
    /// Returns token count for `text`.
    fn count(&self, text: &str) -> u64;
}

/// Approximate token counting using ceil(chars / 4).
pub struct HeuristicCounter;

impl TokenCounter for HeuristicCounter {
    fn count(&self, text: &str) -> u64 {
        text.chars().count().div_ceil(4) as u64
    }
}

/// BPE-backed near-real token counting for selected OpenAI model encodings.
pub struct BpeCounter {
    encoding: TokenizerMode,
}

impl BpeCounter {
    fn new(encoding: TokenizerMode) -> Self {
        Self { encoding }
    }
}

impl TokenCounter for BpeCounter {
    fn count(&self, text: &str) -> u64 {
        let result = match self.encoding {
            TokenizerMode::Gpt5 => o200k_base(),
            TokenizerMode::Cl100k => cl100k_base(),
            TokenizerMode::O200k => o200k_base(),
            TokenizerMode::Heuristic => return 0,
        };
        match result {
            Ok(bpe) => bpe.encode_with_special_tokens(text).len() as u64,
            Err(_) => 0,
        }
    }
}

/// Tokenizer selection for active (primary) token accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizerMode {
    Gpt5,
    Heuristic,
    Cl100k,
    O200k,
}

impl TokenizerMode {
    /// Parses `TOKEN_SAVER_TOKENIZER` values.
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "gpt5" | "gpt-5" => Self::Gpt5,
            "cl100k" | "cl100k_base" => Self::Cl100k,
            "o200k" | "o200k_base" => Self::O200k,
            _ => Self::Heuristic,
        }
    }

    /// Human-readable mode label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Gpt5 => "gpt5",
            Self::Heuristic => "heuristic",
            Self::Cl100k => "cl100k",
            Self::O200k => "o200k",
        }
    }

    fn model_counter(self) -> Option<BpeCounter> {
        match self {
            Self::Gpt5 => Some(BpeCounter::new(Self::O200k)),
            Self::Heuristic => None,
            Self::Cl100k => Some(BpeCounter::new(Self::Cl100k)),
            Self::O200k => Some(BpeCounter::new(Self::O200k)),
        }
    }
}

/// Returns the active tokenizer mode from environment.
pub fn active_mode() -> TokenizerMode {
    let value = std::env::var("TOKEN_SAVER_TOKENIZER").unwrap_or_else(|_| "gpt5".to_string());
    TokenizerMode::parse(&value)
}

/// Per-text counts from both heuristic and model backends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenEstimate {
    pub heuristic: u64,
    pub model: Option<u64>,
}

/// Computes token counts for `text` using heuristic and, when available,
/// the active model tokenizer.
pub fn estimate(text: &str) -> TokenEstimate {
    let heuristic_counter = HeuristicCounter;
    let heuristic = heuristic_counter.count(text);
    let model = active_mode().model_counter().and_then(|counter| {
        let count = counter.count(text);
        if count == 0 {
            // Treat `0` as unavailable for non-empty inputs when model loading failed.
            if text.is_empty() {
                Some(0)
            } else {
                None
            }
        } else {
            Some(count)
        }
    });

    TokenEstimate { heuristic, model }
}

/// Chooses the primary token count based on active mode with safe fallback.
pub fn select_active(estimate: TokenEstimate) -> u64 {
    match active_mode() {
        TokenizerMode::Heuristic => estimate.heuristic,
        TokenizerMode::Gpt5 | TokenizerMode::Cl100k | TokenizerMode::O200k => {
            estimate.model.unwrap_or(estimate.heuristic)
        }
    }
}

/// Counts words by splitting on Unicode whitespace. Spaces are not counted.
pub fn count_words(text: &str) -> u64 {
    text.split_whitespace().count() as u64
}

/// Counts words per logical line, preserving blank lines as zero-word entries.
pub fn count_words_per_line(text: &str) -> Vec<u64> {
    text.lines().map(count_words).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mode_values() {
        assert_eq!(TokenizerMode::parse("heuristic"), TokenizerMode::Heuristic);
        assert_eq!(TokenizerMode::parse("gpt5"), TokenizerMode::Gpt5);
        assert_eq!(TokenizerMode::parse("gpt-5"), TokenizerMode::Gpt5);
        assert_eq!(TokenizerMode::parse("cl100k"), TokenizerMode::Cl100k);
        assert_eq!(TokenizerMode::parse("o200k_base"), TokenizerMode::O200k);
        assert_eq!(TokenizerMode::parse("unknown"), TokenizerMode::Heuristic);
    }

    #[test]
    fn heuristic_counter_uses_chars_div_4() {
        let c = HeuristicCounter;
        assert_eq!(c.count(""), 0);
        assert_eq!(c.count("abcd"), 1);
        assert_eq!(c.count("abcde"), 2);
    }

    #[test]
    fn word_count_ignores_spaces() {
        assert_eq!(count_words(""), 0);
        assert_eq!(count_words("a    b\t c"), 3);
        assert_eq!(count_words("  spaced   out  words  "), 3);
    }

    #[test]
    fn word_count_per_line_preserves_blank_lines() {
        let counts = count_words_per_line("one two\n\nthree");
        assert_eq!(counts, vec![2, 0, 1]);
    }
}
