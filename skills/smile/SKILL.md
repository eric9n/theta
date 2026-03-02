---
name: smile
description: Use when the user wants theta to inspect a single-expiry volatility smile, including put and call wing shape relative to ATM IV.
---

# smile

Run from the theta repository root (the directory containing `Cargo.toml`).

Use:

```bash
cargo run --bin smile -- --symbol TSLA.US --expiry 2026-03-20
```

Common options:
- repeat `--target-otm-percent` for custom wing points
- `--json`

Requires live market data credentials.
