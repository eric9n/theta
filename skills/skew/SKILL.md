---
name: skew
description: Use when the user wants theta to measure single-expiry option skew, including delta-based and OTM-based put/call IV asymmetry.
---

# skew

This skill is intended to compile cleanly into a Tellar `SKILL.json`.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `skew` bin in theta:

```bash
./scripts/skew.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer one explicit tool named `skew`.
Do not invent extra subcommands; this bin is a single configurable query.
The tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/skew.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin skew` first.
- Live data requires `LONGPORT_APP_KEY`, `LONGPORT_APP_SECRET`, and `LONGPORT_ACCESS_TOKEN`.
- `expiry` must be `YYYY-MM-DD`.

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
./scripts/skew.sh --symbol <symbol> --expiry <expiry> [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--target-delta <target_delta>] [--target-otm-percent <target_otm_percent>] [--json]
```

Guidance:
- Use this tool when the caller wants put/call IV asymmetry for one expiry, not a full market aggregate.
- Prefer `json=true` when the result will be machine-read.
