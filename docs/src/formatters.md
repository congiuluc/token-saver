# Command Formatters

token-saver includes dedicated formatters for common developer commands. When a
command has no dedicated formatter, output falls back to the generic compressor.
Dispatch is keyed off the command you originally typed, not any internal
rewrite.

## Supported commands

| Group              | Commands                                              | Source                  |
| ------------------ | ---------------------------------------------------- | ----------------------- |
| Source control     | `git status`, `git log`, `git diff`, `git branch`    | `src/format/git.rs`     |
| Rust               | `cargo build`/`check`, `cargo test`                  | `src/format/cargo.rs`   |
| .NET               | `dotnet build`/`publish`/`pack`/`msbuild`, `test`, `restore` | `src/format/dotnet.rs` |
| Java               | `mvn`/`mvnw`, `gradle`/`gradlew`                     | `src/format/java.rs`    |
| Go                 | `go build`/`install`/`vet`, `go test`                | `src/format/golang.rs`  |
| TypeScript / lint  | `tsc`, `eslint`                                      | `src/format/ts.rs`      |
| JS test runners    | `jest`, `vitest` (incl. `npm test` / `yarn test`)    | `src/format/jstest.rs`  |
| Node install       | `npm install`/`i`/`ci`                               | `src/format/node.rs`    |
| Package managers   | `yarn`, `pnpm`, `bun`, `pip`/`pip3`, `poetry`        | `src/format/pkg.rs`     |
| Python tests       | `pytest`, `py.test`                                  | `src/format/py.rs`      |
| Cloud / platform   | `az`, `azd`, `gh`, `copilot`                         | `src/format/cloud.rs`   |
| Containers         | `docker ps`, `kubectl get`                           | `src/format/container.rs` |

Wrapped invocations are also recognized — for example `npx eslint`,
`node_modules/.bin/jest`, or `npm test` route to the right formatter even when
the tool name is not the first argument.

## Command rewriting

A few git commands are rewritten to a machine-readable variant before execution
so their output can be parsed reliably, then summarized against the command you
typed:

- `git status` → `git status --porcelain=v1 --branch`
- `git diff` and `git log` use parseable variants as well.

## Generic fallback behavior

The generic formatter prioritizes signal extraction:

- Errors
- Warnings
- Summary lines

When output is still too long, it prints a concise head/tail excerpt with a
stats footer. In [extreme mode](usage.md#extreme-mode) (`-x`), unrecognized
output is reduced to errors plus a one-line stats footer only.
