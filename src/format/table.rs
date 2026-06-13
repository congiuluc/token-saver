//! Helpers for reducing fixed-width, column-aligned command output (such as
//! `docker ps` or `kubectl get`) down to a chosen subset of columns.

use crate::format::generic;

/// A header column and the character offset at which it starts.
struct Column {
    name: String,
    start: usize,
}

/// Reduces column-aligned `text` to only the columns named in `keep`,
/// preserving the order given in `keep`.
///
/// Returns `None` when the header is missing or none of the requested columns
/// are present, signalling the caller to fall back to generic formatting.
pub fn select(text: &str, keep: &[&str]) -> Option<Vec<Vec<String>>> {
    let clean = generic::strip_ansi(text);
    let mut lines = clean.lines();
    let header = lines.next()?;
    let columns = header_columns(header);

    // Map each requested column name to its index in the detected header.
    let chosen: Vec<usize> =
        keep.iter().filter_map(|name| columns.iter().position(|c| c.name.eq_ignore_ascii_case(name))).collect();
    if chosen.is_empty() {
        return None;
    }

    let mut rows: Vec<Vec<String>> = Vec::new();
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let cells = slice_row(line, &columns);
        let picked: Vec<String> = chosen.iter().map(|&i| cells.get(i).cloned().unwrap_or_default()).collect();
        rows.push(picked);
    }
    Some(rows)
}

/// Detects column names and their starting character offsets from a header row.
/// Columns are assumed to be separated by runs of two or more spaces.
fn header_columns(header: &str) -> Vec<Column> {
    let chars: Vec<char> = header.chars().collect();
    let mut columns: Vec<Column> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let start = i;
        let mut name = String::new();
        while i < chars.len() {
            if chars[i] == ' ' && i + 1 < chars.len() && chars[i + 1] == ' ' {
                break;
            }
            name.push(chars[i]);
            i += 1;
        }
        columns.push(Column { name: name.trim().to_string(), start });
    }
    columns
}

/// Slices a data row into cell values using the column start offsets.
fn slice_row(line: &str, columns: &[Column]) -> Vec<String> {
    let chars: Vec<char> = line.chars().collect();
    let mut cells: Vec<String> = Vec::with_capacity(columns.len());

    for (i, col) in columns.iter().enumerate() {
        let start = col.start.min(chars.len());
        let end = columns.get(i + 1).map(|next| next.start.min(chars.len())).unwrap_or(chars.len());
        let value: String = chars[start..end].iter().collect();
        cells.push(value.trim().to_string());
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_named_columns() {
        let text = "\
CONTAINER ID   IMAGE     COMMAND   STATUS         NAMES
abc123         nginx     \"run\"     Up 2 minutes   web
def456         redis     \"serve\"   Exited (0)     cache
";
        let rows = select(text, &["NAMES", "IMAGE", "STATUS"]).unwrap();
        assert_eq!(rows[0], vec!["web", "nginx", "Up 2 minutes"]);
        assert_eq!(rows[1], vec!["cache", "redis", "Exited (0)"]);
    }
}
