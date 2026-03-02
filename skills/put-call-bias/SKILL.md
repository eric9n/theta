---
name: put-call-bias
description: Use when the user wants theta to summarize single-expiry put versus call demand, positioning, volume, open interest, and IV bias.
---

# put-call-bias

This skill is intended to compile cleanly into a Tellar `SKILL.json`.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `put-call-bias` bin in theta:

```bash
./scripts/put-call-bias.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer one explicit tool named `put_call_bias`.
Do not invent extra subcommands; this bin is a single configurable query.
The tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/put-call-bias.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin put-call-bias` first.
- Live data requires `LONGPORT_APP_KEY`, `LONGPORT_APP_SECRET`, and `LONGPORT_ACCESS_TOKEN`.
- `expiry` must be `YYYY-MM-DD`.

## Tool definitions

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
./scripts/put-call-bias.sh --symbol <symbol> --expiry <expiry> [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--min-otm-percent <min_otm_percent>] [--json]
```

Guidance:
- Use this tool for directional demand and positioning analysis, not for a full skew or smile read.
- Prefer `json=true` when the result will be machine-read.
