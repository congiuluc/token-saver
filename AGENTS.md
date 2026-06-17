<!-- token-saver-instructions v1 -->
# token-saver — Token-Optimized Command Output

**token-saver** runs a command and prints an extremely compact summary of
its output, cutting token usage on noisy commands.

## Rule

Always prefix shell commands with `tks`:

```bash
# Instead of:        Use:
git status           tks git status
git log              tks git log
cargo test           tks cargo test
docker ps            tks docker ps
kubectl get pods     tks kubectl get pods
npm install          tks npm install
```

## Modes

- `tks -x <command>`    Extreme mode: errors + a one-line stats footer only.
- `tks --raw <command>` Bypass summarization and print raw output.
<!-- /token-saver-instructions -->
