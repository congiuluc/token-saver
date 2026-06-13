# Architecture

token-saver follows a simple flow:

1. Parse CLI arguments.
2. Optionally rewrite command invocation to a machine-readable variant.
3. Execute child process and collect output and exit code.
4. Route output to command-specific formatter or generic compressor.
5. Emit compact summary and return original exit code.

## Core modules

- `src/main.rs`: CLI entrypoint.
- `src/lib.rs`: shared command dispatch.
- `src/runner.rs`: process execution and output capture.
- `src/format/mod.rs`: formatter routing and rewrite logic.
- `src/format/generic.rs`: fallback compressor.
- `src/metrics.rs`: token and gain accounting.
- `src/otel.rs`: optional OpenTelemetry export.

## Design principles

- Deterministic output transformations.
- Strong behavior parity with original command execution.
- Minimal dependencies.
- Safe defaults and explicit opt-in for telemetry.
