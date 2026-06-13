<!-- token-saver-instructions v1 -->
# token-saver — Token-Optimized Command Output

**token-saver** runs a command and prints an extremely compact summary of
its output, cutting token usage on noisy commands.

## Rule

Always prefix shell commands with `ts`:

```bash
# Instead of:        Use:
git status           ts git status
git log              ts git log
cargo test           ts cargo test
docker ps            ts docker ps
kubectl get pods     ts kubectl get pods
npm install          ts npm install
```

## Modes

- `ts -x <command>`    Extreme mode: errors + a one-line stats footer only.
- `ts --raw <command>` Bypass summarization and print raw output.
<!-- /token-saver-instructions -->
