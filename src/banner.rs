//! `token-saver banner` — an animated ASCII-art splash, in the spirit of the
//! GitHub Copilot CLI startup banner.
//!
//! The banner draws an invented "TS" coin logo above a clean block-letter
//! `TOKEN·SAVER` word-mark, then plays a short, finite animation: a left-to-right
//! light sweep reveals the art behind a bright leading edge, followed by a
//! flowing multi-stop truecolor gradient that shimmers across the whole mark.
//! It is purely cosmetic and never changes state.
//!
//! Behavior is terminal-aware:
//!
//! - On a non-interactive stdout (pipe, file, CI) it prints a single static,
//!   uncolored frame so logs stay clean.
//! - When `NO_COLOR` is set (see <https://no-color.org>) colors are suppressed.
//! - `--no-anim` / `--static` forces a single colored frame even on a TTY.

use std::io::{self, IsTerminal, Write};
use std::thread;
use std::time::Duration;

/// Height in cells of every block glyph.
const GLYPH_H: usize = 5;

/// Word rendered in the large block font.
const WORDMARK: &str = "TOKEN-SAVER";

/// Tagline shown under the word-mark.
const TAGLINE: &str = "fewer tokens · same signal";

/// Delay between animation frames.
const FRAME_DELAY: Duration = Duration::from_millis(30);

/// Columns the reveal edge advances each frame.
const SWEEP_SPEED: f32 = 2.6;

/// Extra shimmer frames played once the reveal edge clears the art.
const SHIMMER_FRAMES: usize = 26;

/// Gradient color stops (RGB), cycled to make a flowing brand sweep. These match
/// the GitHub Copilot CLI banner palette: GitHub's purple → pink → blue accents.
const STOPS: &[(u8, u8, u8)] = &[
    (88, 166, 255),  // blue    #58a6ff
    (163, 113, 247), // purple  #a371f7
    (219, 97, 162),  // pink    #db61a2
    (247, 120, 186), // rose    #f778ba
    (31, 111, 235),  // cobalt  #1f6feb
    (121, 192, 255), // sky     #79c0ff
];

/// Runs the banner. `args` may contain `--no-anim`/`--static` to force one frame.
pub fn run(args: &[String]) -> io::Result<()> {
    let stdout = io::stdout();
    let interactive = stdout.is_terminal();
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let forced_static = args.iter().any(|a| a == "--no-anim" || a == "--static" || a == "--plain");

    let art = compose();
    let width = art.iter().map(|r| r.chars().count()).max().unwrap_or(0);
    let mut out = stdout.lock();

    // Non-interactive or explicitly static: one settled frame and return.
    if !interactive || forced_static {
        let color = interactive && !no_color;
        render_frame(&mut out, &art, width, f32::INFINITY, 0.0, color)?;
        return out.flush();
    }

    // Hide the cursor for the duration of the animation; always restore it.
    write!(out, "\x1b[?25l")?;
    out.flush()?;
    let result = animate(&mut out, &art, width, !no_color);
    let _ = write!(out, "\x1b[?25h");
    let _ = out.flush();
    result
}

/// Plays the reveal sweep followed by the shimmer, redrawing in place.
fn animate(out: &mut impl Write, art: &[String], width: usize, color: bool) -> io::Result<()> {
    let reveal_frames = ((width as f32 + 6.0) / SWEEP_SPEED).ceil() as usize;
    let total = reveal_frames + SHIMMER_FRAMES;

    for frame in 0..total {
        if frame > 0 {
            write!(out, "\x1b[{}A", art.len())?; // back to the first art row
        }
        // During the reveal phase the edge marches across; afterwards the whole
        // mark is lit and only the gradient phase keeps flowing.
        let edge = if frame < reveal_frames { frame as f32 * SWEEP_SPEED } else { f32::INFINITY };
        let time = frame as f32 * 0.45;
        render_frame(out, art, width, edge, time, color)?;
        out.flush()?;
        thread::sleep(FRAME_DELAY);
    }
    Ok(())
}

/// Renders one frame of the art grid.
///
/// `edge` is the reveal column: filled cells to its right are hidden, and cells
/// right at the edge are brightened to a white leading highlight. `time` drives
/// the flowing gradient.
fn render_frame(
    out: &mut impl Write,
    art: &[String],
    width: usize,
    edge: f32,
    time: f32,
    color: bool,
) -> io::Result<()> {
    for (row, line) in art.iter().enumerate() {
        let mut buf = String::with_capacity(width * 12);
        for (col, ch) in line.chars().enumerate() {
            if ch == ' ' {
                buf.push(' ');
                continue;
            }
            if (col as f32) > edge {
                buf.push(' '); // not yet revealed
                continue;
            }
            if !color {
                buf.push(ch);
                continue;
            }
            let phase = col as f32 * 0.14 + row as f32 * 0.05 + time;
            let (mut r, mut g, mut b) = gradient(phase);
            // Bright leading edge during the reveal.
            let dist = edge - col as f32;
            if dist.is_finite() && (0.0..2.4).contains(&dist) {
                let k = 1.0 - dist / 2.4;
                r = lerp_u8(r, 255, k);
                g = lerp_u8(g, 255, k);
                b = lerp_u8(b, 255, k);
            }
            buf.push_str(&format!("\x1b[38;2;{r};{g};{b}m{ch}"));
        }
        if color {
            buf.push_str("\x1b[0m");
        }
        // Clear to end of line so a shorter frame never leaves stale glyphs.
        write!(out, "{buf}\x1b[K\n")?;
    }
    Ok(())
}

/// Builds the full art grid: the coin logo, the word-mark, and the tagline, all
/// padded to a common width so the gradient lines up across every row.
fn compose() -> Vec<String> {
    let word = render_word(WORDMARK);
    let width = word.iter().map(|r| r.chars().count()).max().unwrap_or(0);

    let mut rows: Vec<String> = Vec::new();
    for line in logo() {
        rows.push(center(&line, width));
    }
    rows.push(" ".repeat(width));
    rows.extend(word.into_iter().map(|r| pad(&r, width)));
    rows.push(" ".repeat(width));
    rows.push(center(TAGLINE, width));
    rows
}

/// The invented logo: a rounded coin badge enclosing a block "TS" monogram, the
/// down-chevrons hinting at shrinking token cost.
fn logo() -> Vec<String> {
    let inner = GLYPH_H; // five monogram rows
    let mut badge = Vec::with_capacity(inner + 2);
    badge.push(format!("╭{}╮", "─".repeat(13)));
    for row in 0..inner {
        badge.push(format!("│ {} {} │", glyph('T')[row], glyph('S')[row]));
    }
    badge.push(format!("╰{}╯", "─".repeat(13)));
    badge
}

/// Renders `word` into [`GLYPH_H`] rows of block glyphs joined by single spaces.
fn render_word(word: &str) -> Vec<String> {
    (0..GLYPH_H)
        .map(|row| {
            word.chars().map(|c| glyph(c)[row]).collect::<Vec<_>>().join(" ")
        })
        .collect()
}

/// Pads `s` on the right to `width` columns.
fn pad(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - len))
    }
}

/// Centers `s` within `width` columns.
fn center(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        return s.to_string();
    }
    let left = (width - len) / 2;
    let right = width - len - left;
    format!("{}{s}{}", " ".repeat(left), " ".repeat(right))
}

/// Five-row block art for one glyph; unknown characters render as blank.
fn glyph(c: char) -> [&'static str; GLYPH_H] {
    match c.to_ascii_uppercase() {
        'T' => ["█████", "  █  ", "  █  ", "  █  ", "  █  "],
        'O' => [" ███ ", "█   █", "█   █", "█   █", " ███ "],
        'K' => ["█   █", "█  █ ", "███  ", "█  █ ", "█   █"],
        'E' => ["█████", "█    ", "████ ", "█    ", "█████"],
        'N' => ["█   █", "██  █", "█ █ █", "█  ██", "█   █"],
        'S' => [" ████", "█    ", " ███ ", "    █", "████ "],
        'A' => [" ███ ", "█   █", "█████", "█   █", "█   █"],
        'V' => ["█   █", "█   █", "█   █", " █ █ ", "  █  "],
        'R' => ["████ ", "█   █", "████ ", "█  █ ", "█   █"],
        '-' => ["     ", "     ", " ███ ", "     ", "     "],
        _ => ["     ", "     ", "     ", "     ", "     "],
    }
}

/// Maps a continuously increasing phase to an RGB color flowing through [`STOPS`].
fn gradient(phase: f32) -> (u8, u8, u8) {
    let n = STOPS.len() as f32;
    let pos = phase.rem_euclid(n);
    let i = pos.floor() as usize;
    let f = pos - i as f32;
    let (r0, g0, b0) = STOPS[i % STOPS.len()];
    let (r1, g1, b1) = STOPS[(i + 1) % STOPS.len()];
    (lerp_u8(r0, r1, f), lerp_u8(g0, g1, f), lerp_u8(b0, b1, f))
}

/// Linear interpolation between two bytes by `t` in `0.0..=1.0`.
fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8
}
