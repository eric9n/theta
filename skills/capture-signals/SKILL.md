---
name: capture-signals
description: Use when the user wants theta to persist recurring market-tone snapshots into SQLite, including one-shot capture, looped sampling, or market-hours-only scheduling.
---

# capture-signals

This skill is intended to compile cleanly into a Tellar `SKILL.json`.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `capture-signals` bin in theta:

```bash
./scripts/capture-signals.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer one explicit tool named `capture_signals`.
Do not invent extra subcommands; this bin is a single configurable runner.
The tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/capture-signals.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin capture-signals` first.
- If no `symbol` values are passed, the binary defaults to `TSLA.US` and `QQQ.US`.
- If no `smile_target_otm_percent` values are passed, the binary defaults to `0.05`, `0.10`, and `0.15`.
- If no `db` is passed, the binary defaults to `~/.theta/signals.db`.
- `market_hours_only` should be used for long-running samplers unless the caller explicitly wants pre/post-market samples.

## Tool definitions

### Tool: capture_signals

Purpose: capture and persist market tone snapshots for one or more symbols into SQLite, either once or in a recurring loop.

Optional parameters:
- `symbol` (string, repeatable)
- `db` (string path)
- `expiries_limit` (integer)
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_market_price` (boolean)
- `target_delta` (number)
- `target_otm_percent` (number)
- `smile_target_otm_percent` (number, repeatable)
- `bias_min_otm_percent` (number)
- `loop` (boolean)
- `every_seconds` (integer)
- `market_hours_only` (boolean)

Command template:

```bash
./scripts/capture-signals.sh [--symbol <symbol>]... [--db <db>] [--expiries-limit <expiries_limit>] [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--target-delta <target_delta>] [--target-otm-percent <target_otm_percent>] [--smile-target-otm-percent <smile_target_otm_percent>]... [--bias-min-otm-percent <bias_min_otm_percent>] [--loop] [--every-seconds <every_seconds>] [--market-hours-only]
```

Guidance:
- For one-shot default capture, call the tool with no parameters.
- For a 5-minute recurring sampler, use `loop=true`, `every_seconds=300`, and `market_hours_only=true`.
- Use repeated `symbol` values to override the default symbol set.
- Use repeated `smile_target_otm_percent` values only when the caller explicitly wants non-default smile sampling buckets.
