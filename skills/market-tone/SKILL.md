---
name: market-tone
description: Use when the user wants theta's aggregated market structure view, combining skew, smile, term structure, and put/call bias into a high-level tone summary.
---

# market-tone

This skill is intended to compile cleanly into a Tellar `SKILL.json`.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `market-tone` bin in theta:

```bash
./scripts/market-tone.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer one explicit tool named `market_tone`.
Do not invent extra subcommands; this bin is a single configurable query.
The tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/market-tone.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin market-tone` first.
- Live data requires `LONGPORT_APP_KEY`, `LONGPORT_APP_SECRET`, and `LONGPORT_ACCESS_TOKEN`.
- `expiry` must be `YYYY-MM-DD`.
- If no `smile_target_otm_percent` values are passed, the binary defaults to `0.05`, `0.10`, and `0.15`.

## Tool definitions

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
./scripts/market-tone.sh --symbol <symbol> --expiry <expiry> [--expiries-limit <expiries_limit>] [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--target-delta <target_delta>] [--target-otm-percent <target_otm_percent>] [--smile-target-otm-percent <smile_target_otm_percent>]... [--bias-min-otm-percent <bias_min_otm_percent>] [--json]
```

Guidance:
- Use this tool when the caller wants a single consolidated structure/sentiment read, not raw chain data.
- Prefer `json=true` when the result will be machine-read.
- Use custom `smile_target_otm_percent` values only when the caller explicitly wants non-default smile buckets.
