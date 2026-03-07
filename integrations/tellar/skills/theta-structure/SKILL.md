---
name: theta-structure
description: Use when the user wants theta's options structure analysis through `theta structure`, including skew, smile, put/call bias, market tone, and term structure.
---

# theta-structure

This skill is intended to compile cleanly into a Tellar `SKILL.json` with multiple explicit tools.

Use this skill only for the `theta structure` command group:

```bash
./scripts/theta.sh structure --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, preserve the explicit tools below instead of collapsing everything into one generic command.
Each tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/theta.sh structure ...`.
- If the release binary is missing, build with `cargo build --release --bin theta`.
- Live data requires the local `theta-daemon` to be running and reachable at `/tmp/theta.sock`.
- LongPort credentials are required by `theta-daemon`, not by the `theta` CLI process itself.
- `expiry` values must use `YYYY-MM-DD` where required.

## Tool definitions

### Tool: skew

Purpose: measure single-expiry option skew using both delta-based and OTM-based put/call IV asymmetry.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_market_price` (boolean)
- `target_delta` (number)
- `target_otm_percent` (number)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh structure skew --symbol <symbol> --expiry <expiry> [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--target-delta <target_delta>] [--target-otm-percent <target_otm_percent>] [--json]
```

### Tool: smile

Purpose: inspect a single-expiry volatility smile, including put and call wing shape relative to ATM IV.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_market_price` (boolean)
- `target_otm_percent` (number, repeatable)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh structure smile --symbol <symbol> --expiry <expiry> [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--target-otm-percent <target_otm_percent>]... [--json]
```

### Tool: put_call_bias

Purpose: summarize single-expiry put versus call demand, positioning, volume, open interest, and IV bias.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_market_price` (boolean)
- `min_otm_percent` (number)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh structure put-call-bias --symbol <symbol> --expiry <expiry> [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--min-otm-percent <min_otm_percent>] [--json]
```

### Tool: market_tone

Purpose: produce a high-level market structure summary by aggregating skew, smile, term structure, and put/call bias for one symbol and front expiry.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `expiries_limit` (integer)
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_market_price` (boolean)
- `target_delta` (number)
- `target_otm_percent` (number)
- `smile_target_otm_percent` (number, repeatable)
- `bias_min_otm_percent` (number)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh structure market-tone --symbol <symbol> --expiry <expiry> [--expiries-limit <expiries_limit>] [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--target-delta <target_delta>] [--target-otm-percent <target_otm_percent>] [--smile-target-otm-percent <smile_target_otm_percent>]... [--bias-min-otm-percent <bias_min_otm_percent>] [--json]
```

### Tool: term_structure

Purpose: compute ATM IV term structure across the next several expiries for a symbol.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)

Optional parameters:
- `expiries_limit` (integer)
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_market_price` (boolean)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh structure term-structure --symbol <symbol> [--expiries-limit <expiries_limit>] [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--json]
```
