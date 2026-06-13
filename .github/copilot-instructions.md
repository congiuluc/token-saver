<!-- tokensaver-instructions v1 -->
IMPORTANT USE ALWAYS TOKENSAVER for tool calling
# tokensaver — Token-Optimized Command Output

**tokensaver** runs a command and prints an extremely compact summary of
its output, cutting token usage on noisy commands.

## Rule

Always prefix shell commands with `tokensaver`:

```bash
# Instead of:        Use:
git status           tokensaver git status
git log              tokensaver git log
cargo test           tokensaver cargo test
docker ps            tokensaver docker ps
kubectl get pods     tokensaver kubectl get pods
npm install          tokensaver npm install
```

## Modes

- `tokensaver -x <command>`    Extreme mode: errors + a one-line stats footer only.
- `tokensaver --raw <command>` Bypass summarization and print raw output.
<!-- /tokensaver-instructions -->
