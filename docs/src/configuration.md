# Configuration

token-saver works with zero configuration. Every setting below is optional and
controlled through environment variables. token-saver never writes to these — it
only reads them.

## Metrics logging

| Variable          | Default                        | Description                                                                 |
| ----------------- | ------------------------------ | --------------------------------------------------------------------------- |
| `TOKEN_SAVER_LOG` | `~/.token-saver/metrics.jsonl` | Path to the JSONL metrics log. Set to `off` (or `0`/empty) to disable logging, or to a custom path to redirect it. |

The metrics log is what `token-saver gain` aggregates. Each line is a JSON
record with raw/compacted token and byte counts. Disabling the log means `gain`
has nothing to report.

## Tokenizer selection

| Variable                | Default | Values                                  | Description                              |
| ----------------------- | ------- | --------------------------------------- | ---------------------------------------- |
| `TOKEN_SAVER_TOKENIZER` | `gpt5`  | `gpt5`, `o200k`, `cl100k`, `heuristic`  | Selects the primary token-counting backend. |

- `gpt5` (default) and `o200k`: near-real BPE for GPT-4o/GPT-5-style encodings.
- `cl100k`: near-real BPE for GPT-4/3.5-style encodings.
- `heuristic`: fast approximation using `ceil(chars / 4)`.

The active mode determines the primary `rawTokens`/`outTokens` totals. Heuristic
and model counts are also computed separately so `gain` can display both side by
side.

## OpenTelemetry export

Each token-saver run (`run`, `stdin`, or `hook`) can be emitted as an OTLP span
describing how much the output was compressed. Export is fully opt-in.

| Variable                                                       | Default                       | Description                                                                                          |
| -------------------------------------------------------------- | ----------------------------- | -------------------------------------------------------------------------------------------------- |
| `TOKEN_SAVER_OTEL`                                             | unset (off)                   | Enable export when truthy (anything other than `off`, `0`, or empty).                              |
| `TOKEN_SAVER_OTEL_FILE`                                        | `~/.token-saver/traces.jsonl` | Local OTLP-JSON span file. Set to `off`/`0`/empty to disable the file sink.                        |
| `OTEL_EXPORTER_OTLP_ENDPOINT` / `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` | unset                  | When set, spans are POSTed as OTLP JSON to `<endpoint>/v1/traces`. Also implicitly enables export. |
| `OTEL_SERVICE_NAME`                                            | `token-saver`                 | Service name attached to exported spans.                                                            |

> The exporter is dependency-free and uses plain HTTP/1.1, so it cannot reach an
> HTTPS ingest directly. Point `OTEL_EXPORTER_OTLP_ENDPOINT` at a local
> OpenTelemetry Collector/agent that terminates TLS upstream. All telemetry
> failures are swallowed and never affect the primary command.

## Installer variables

Used by the install scripts (`install.sh` / `install.ps1`), not the binary itself:

| Variable                | Default                              | Description                                |
| ----------------------- | ------------------------------------ | ------------------------------------------ |
| `TOKEN_SAVER_VERSION`   | `latest`                             | Release tag to install (e.g. `v0.1.0`).    |
| `TOKEN_SAVER_BIN_DIR`   | `~/.local/bin` (Unix) / `%LOCALAPPDATA%\Programs\token-saver` (Windows) | Install directory. |

## Display

| Variable    | Description                                                       |
| ----------- | ---------------------------------------------------------------- |
| `NO_COLOR`  | When set, the `banner` command renders without ANSI color codes. |
