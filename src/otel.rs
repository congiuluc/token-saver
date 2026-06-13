//! OpenTelemetry export for token-saver events.
//!
//! Each token-saver run (a `run`, a `stdin` filter, or a `hook` compression) can be
//! emitted as an OTLP **span** describing how much the output was compressed.
//! Export is dependency-free and built on `std` only, so it is constrained to
//! plain HTTP/1.1: it cannot reach an HTTPS ingest directly — point it at a
//! local OpenTelemetry Collector/agent that terminates TLS upstream.
//!
//! Two sinks, both opt-in:
//! - **Local file** — when OpenTelemetry is enabled, spans are appended as OTLP
//!   JSON (one document per line) to `~/.token-saver/traces.jsonl`. Override the path
//!   with `TOKEN_SAVER_OTEL_FILE`, or disable the file with `off`, `0`, or empty.
//! - **OTLP/HTTP** — when `OTEL_EXPORTER_OTLP_ENDPOINT` (or
//!   `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`) is set, the span is POSTed as OTLP
//!   JSON to `<endpoint>/v1/traces`.
//!
//! OpenTelemetry is active when `TOKEN_SAVER_OTEL` is truthy (anything other than
//! `off`/`0`/empty) or when an OTLP endpoint is configured. The service name
//! honors `OTEL_SERVICE_NAME` (default `token-saver`). All failures are swallowed so
//! telemetry never affects the primary command.

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A measured token-saver event to export as an OTLP span.
pub struct Span<'a> {
    /// Invocation kind (`run`, `stdin`, `hook`).
    pub mode: &'a str,
    /// User-facing command string.
    pub command: &'a str,
    /// Estimated tokens in the raw (original) input.
    pub raw_tokens: u64,
    /// Estimated tokens in the token-saver output.
    pub out_tokens: u64,
    /// Byte length of the raw input.
    pub raw_bytes: u64,
    /// Byte length of the token-saver output.
    pub out_bytes: u64,
    /// Wall-clock duration of the run.
    pub duration: Duration,
}

/// Exports `span` as an OTLP trace to the configured sinks, if OpenTelemetry is
/// enabled. Writes the local span file and/or POSTs to the OTLP endpoint; any
/// I/O error is silently ignored.
pub fn export(span: &Span) {
    if !enabled() {
        return;
    }
    let payload = build_payload(span);
    if let Some(path) = file_path() {
        let _ = append(&path, &format!("{payload}\n"));
    }
    if let Some(endpoint) = traces_endpoint() {
        let _ = post(&endpoint, &payload);
    }
}

/// Reports whether OpenTelemetry export is active.
fn enabled() -> bool {
    is_truthy("TOKEN_SAVER_OTEL") || traces_endpoint().is_some()
}

/// Returns `true` when `var` is set to anything other than `off`, `0`, or empty.
fn is_truthy(var: &str) -> bool {
    match env::var(var) {
        Ok(value) => {
            let trimmed = value.trim();
            !(trimmed.is_empty() || trimmed.eq_ignore_ascii_case("off") || trimmed == "0")
        }
        Err(_) => false,
    }
}

/// Resolves the OTLP traces endpoint, honoring the signal-specific variable
/// first, then deriving `<base>/v1/traces` from the generic endpoint.
fn traces_endpoint() -> Option<String> {
    if let Some(value) = non_empty_var("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT") {
        return Some(value);
    }
    let base = non_empty_var("OTEL_EXPORTER_OTLP_ENDPOINT")?;
    Some(format!("{}/v1/traces", base.trim_end_matches('/')))
}

/// Resolves the local span file path, honoring the `TOKEN_SAVER_OTEL_FILE` override
/// and its disable sentinels (`off`, `0`, empty). Returns `None` when disabled
/// or when the home directory cannot be determined.
fn file_path() -> Option<PathBuf> {
    match env::var("TOKEN_SAVER_OTEL_FILE") {
        Ok(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("off") || trimmed == "0" {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        }
        Err(_) => home_dir().map(|home| home.join(".token-saver").join("traces.jsonl")),
    }
}

/// Returns the configured OTLP service name, defaulting to `token-saver`.
fn service_name() -> String {
    non_empty_var("OTEL_SERVICE_NAME").unwrap_or_else(|| "token-saver".to_string())
}

/// Returns the trimmed value of `var` when it is set and non-empty.
fn non_empty_var(var: &str) -> Option<String> {
    env::var(var).ok().map(|value| value.trim().to_string()).filter(|value| !value.is_empty())
}

/// Builds the OTLP/HTTP JSON traces document for a single span.
fn build_payload(span: &Span) -> String {
    let end = now_nanos();
    let start = end.saturating_sub(span.duration.as_nanos());
    let ids = rand_hex(24);
    let trace_id = &ids[..32];
    let span_id = &ids[32..48];
    let saved = span.raw_tokens.saturating_sub(span.out_tokens);
    let attrs = [
        attr_str("token-saver.mode", span.mode),
        attr_str("token-saver.command", span.command),
        attr_int("token-saver.raw_tokens", span.raw_tokens),
        attr_int("token-saver.out_tokens", span.out_tokens),
        attr_int("token-saver.saved_tokens", saved),
        attr_int("token-saver.raw_bytes", span.raw_bytes),
        attr_int("token-saver.out_bytes", span.out_bytes),
    ]
    .join(",");
    format!(
        "{{\"resourceSpans\":[{{\"resource\":{{\"attributes\":[{resource}]}},\
         \"scopeSpans\":[{{\"scope\":{{\"name\":\"token-saver\",\"version\":\"{version}\"}},\
         \"spans\":[{{\"traceId\":\"{trace_id}\",\"spanId\":\"{span_id}\",\
         \"name\":\"token-saver.{name}\",\"kind\":1,\
         \"startTimeUnixNano\":\"{start}\",\"endTimeUnixNano\":\"{end}\",\
         \"attributes\":[{attrs}],\"status\":{{\"code\":1}}}}]}}]}}]}}",
        resource = attr_str("service.name", &service_name()),
        version = env!("CARGO_PKG_VERSION"),
        name = escape(span.mode),
    )
}

/// Formats an OTLP string-valued attribute.
fn attr_str(key: &str, value: &str) -> String {
    format!("{{\"key\":\"{}\",\"value\":{{\"stringValue\":\"{}\"}}}}", escape(key), escape(value))
}

/// Formats an OTLP integer-valued attribute (int64 is JSON-encoded as a string).
fn attr_int(key: &str, value: u64) -> String {
    format!("{{\"key\":\"{}\",\"value\":{{\"intValue\":\"{}\"}}}}", escape(key), value)
}

/// POSTs `body` as `application/json` to a plain-HTTP OTLP endpoint.
///
/// Only `http://` URLs are supported (`std` provides no TLS); an `https://`
/// endpoint yields an error that the caller swallows.
fn post(endpoint: &str, body: &str) -> io::Result<()> {
    let (host, port, path) = parse_http_url(endpoint)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "non-http OTLP endpoint"))?;
    let addr = (host.as_str(), port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "could not resolve OTLP host"))?;
    let mut stream = TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {host}:{port}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\r\n\
         {body}",
        len = body.len(),
    );
    stream.write_all(request.as_bytes())?;
    stream.flush()?;
    // Drain the response so the server can finish writing; contents are ignored.
    let mut sink = Vec::new();
    let _ = stream.read_to_end(&mut sink);
    Ok(())
}

/// Splits an `http://host[:port][/path]` URL into `(host, port, path)`.
///
/// Returns `None` for any non-`http://` URL (including `https://`). The port
/// defaults to `80` and the path to `/` when omitted.
fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rfind(':') {
        Some(index) => (authority[..index].to_string(), authority[index + 1..].parse().ok()?),
        None => (authority.to_string(), 80),
    };
    if host.is_empty() {
        return None;
    }
    Some((host, port, path.to_string()))
}

/// Generates `bytes` of pseudo-random data as a lowercase hex string.
///
/// Uses a SplitMix64 sequence seeded from the current time. This is for trace
/// and span identifiers only — it is not cryptographically secure.
fn rand_hex(bytes: usize) -> String {
    let mut state = (now_nanos() as u64) ^ 0x9E37_79B9_7F4A_7C15;
    let mut out = String::with_capacity(bytes * 2);
    let mut produced = 0;
    while produced < bytes {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        for b in z.to_le_bytes() {
            if produced >= bytes {
                break;
            }
            out.push_str(&format!("{b:02x}"));
            produced += 1;
        }
    }
    out
}

/// Appends `line` to `path`, creating the file and parent directory as needed.
fn append(path: &Path, line: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    OpenOptions::new().create(true).append(true).open(path)?.write_all(line.as_bytes())
}

/// Returns the current Unix time in nanoseconds, or `0` if the clock predates
/// the epoch.
fn now_nanos() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
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

    #[test]
    fn parses_http_url_parts() {
        assert_eq!(
            parse_http_url("http://localhost:4318/v1/traces"),
            Some(("localhost".to_string(), 4318, "/v1/traces".to_string()))
        );
        assert_eq!(parse_http_url("http://collector"), Some(("collector".to_string(), 80, "/".to_string())));
    }

    #[test]
    fn rejects_non_http_urls() {
        assert_eq!(parse_http_url("https://localhost:4318/v1/traces"), None);
        assert_eq!(parse_http_url("grpc://localhost:4317"), None);
    }

    #[test]
    fn rand_hex_has_expected_length() {
        assert_eq!(rand_hex(16).len(), 32);
        assert_eq!(rand_hex(8).len(), 16);
        assert!(rand_hex(24).chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn payload_is_otlp_shaped() {
        let span = Span {
            mode: "run",
            command: "git status",
            raw_tokens: 100,
            out_tokens: 20,
            raw_bytes: 400,
            out_bytes: 80,
            duration: Duration::from_millis(5),
        };
        let payload = build_payload(&span);
        assert!(payload.contains("\"resourceSpans\""));
        assert!(payload.contains("\"name\":\"token-saver.run\""));
        assert!(payload.contains("\"service.name\""));
        assert!(payload.contains("\"token-saver.saved_tokens\""));
        assert!(payload.contains("\"intValue\":\"80\""));
    }
}
