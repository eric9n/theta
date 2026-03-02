---
name: skew
description: Use when the user wants theta to measure single-expiry option skew, including delta-based and OTM-based put/call IV asymmetry.
---

# skew

Run from the theta repository root (the directory containing `Cargo.toml`).

Use:

```bash
cargo run --bin skew -- --symbol TSLA.US --expiry 2026-03-20
```

Common options:
- `--target-delta 0.25`
- `--target-otm-percent 0.05`
- `--json`

Requires live market data credentials.
