<!-- token-saver-instructions v1 -->
IMPORTANT USE ALWAYS token-saver for tool calling
# token-saver — Token-Optimized Command Output

**token-saver** runs a command and prints an extremely compact summary of
its output, cutting token usage on noisy commands.

## Rule

Always prefix shell commands with `token-saver`:

```bash
# Instead of:        Use:
git status           token-saver git status
git log              token-saver git log
cargo test           token-saver cargo test
docker ps            token-saver docker ps
kubectl get pods     token-saver kubectl get pods
npm install          token-saver npm install
```

## Modes

- `token-saver -x <command>`    Extreme mode: errors + a one-line stats footer only.
- `token-saver --raw <command>` Bypass summarization and print raw output.
<!-- /token-saver-instructions -->
