# Usage Guide

## Basic execution

```bash
tokensaver <command> [args...]
```

Example:

```bash
tokensaver cargo test
```

## Raw mode

Bypass summarization and print command output unchanged.

```bash
tokensaver --raw cargo test
```

## Extreme mode

Apply aggressive compression for unknown commands.

```bash
tokensaver --extreme some-chatty-command
```

## Standard quality loop

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
```
