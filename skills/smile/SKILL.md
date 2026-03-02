---
name: smile
description: Use when the user wants theta to inspect a single-expiry volatility smile, including put and call wing shape relative to ATM IV.
---

# smile

This skill is intended to compile cleanly into a Tellar `SKILL.json`.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `smile` bin in theta:

```bash
./scripts/smile.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer one explicit tool named `smile`.
Do not invent extra subcommands; this bin is a single configurable query.
The tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/smile.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin smile` first.
- Live data requires `LONGPORT_APP_KEY`, `LONGPORT_APP_SECRET`, and `LONGPORT_ACCESS_TOKEN`.
- `expiry` must be `YYYY-MM-DD`.
- If no `target_otm_percent` values are passed, the binary defaults to `0.05`, `0.10`, and `0.15`.

## Tool definitions

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
./scripts/smile.sh --symbol <symbol> --expiry <expiry> [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--target-otm-percent <target_otm_percent>]... [--json]
```

Guidance:
- Use repeated `target_otm_percent` values only when the caller explicitly wants non-default smile buckets.
- Prefer `json=true` when the result will be machine-read.
