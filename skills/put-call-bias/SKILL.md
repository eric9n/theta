---
name: put-call-bias
description: Use when the user wants theta to summarize single-expiry put versus call demand, positioning, volume, open interest, and IV bias.
---

# put-call-bias

Run from the theta repository root (the directory containing `Cargo.toml`).

Use:

```bash
cargo run --bin put-call-bias -- --symbol TSLA.US --expiry 2026-03-20
```

Common options:
- `--min-otm-percent 0.05`
- `--json`

Requires live market data credentials.
