# Command Formatters

TokenSaver includes dedicated formatters for common developer commands. If a
formatter is not available, output falls back to the generic compressor.

## Supported command groups

- Source control: git
- Rust: cargo
- .NET: dotnet
- Java build tools: maven, gradle
- Go: go
- JavaScript/TypeScript: tsc, eslint, jest, vitest
- Package managers: npm, yarn, pnpm, bun, pip, poetry
- Cloud and platform: az, azd, gh, copilot
- Containers: docker, kubectl
- Python tests: pytest

## Generic fallback behavior

The generic formatter prioritizes signal extraction:

- Errors
- Warnings
- Summary lines

When output is still too long, it prints a concise head/tail excerpt with a
stats footer.
