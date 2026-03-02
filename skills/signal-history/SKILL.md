---
name: signal-history
description: Use when the user wants to inspect recent stored market-tone snapshots from theta's SQLite signal store.
---

# signal-history

Run from the theta repository root (the directory containing `Cargo.toml`).

Use:

```bash
cargo run --bin signal-history -- --
```

Typical filters:
- `--symbol TSLA.US`
- `--limit 20`
- `--json`

This is the quick verification tool after `capture-signals` has been running.
