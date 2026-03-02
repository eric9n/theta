---
name: term-structure
description: Use when the user wants theta to compute ATM IV term structure across the next several expiries for a symbol.
---

# term-structure

Run from the theta repository root (the directory containing `Cargo.toml`).

Use:

```bash
cargo run --bin term-structure -- --symbol TSLA.US
```

Common options:
- `--expiries-limit 4`
- `--json`

Requires live market data credentials.
