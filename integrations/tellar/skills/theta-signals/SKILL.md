---
name: theta-signals
description: Use when the user wants theta's snapshot-capture and historical signal workflows through `theta signals`, including capture, history, IV rank, extreme, and relative-extreme commands.
---

# theta-signals

This skill is intended to compile cleanly into a Tellar `SKILL.json` with multiple explicit tools.

Use this skill only for the `theta signals` command group:

```bash
./scripts/theta.sh signals --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, preserve the explicit tools below instead of collapsing everything into one generic command.
Each tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/theta.sh signals ...`.
- If the release binary is missing, build with `cargo build --release --bin theta`.
- If no `db` is passed, the commands default to `~/.theta/signals.db`.
- Historical tools depend on earlier `capture` runs; they are not useful on an empty database.

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
./scripts/theta.sh signals capture [--symbol <symbol>]... [--db <db>] [--expiries-limit <expiries_limit>] [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--target-delta <target_delta>] [--target-otm-percent <target_otm_percent>] [--smile-target-otm-percent <smile_target_otm_percent>]... [--bias-min-otm-percent <bias_min_otm_percent>] [--loop] [--every-seconds <every_seconds>] [--market-hours-only]
```

Guidance:
- For one-shot default capture, call the tool with no parameters.
- For a 5-minute recurring sampler, use `loop=true`, `every_seconds=300`, and `market_hours_only=true`.
- Use repeated `symbol` values to override the default symbol set.

### Tool: signal_history

Purpose: inspect recent stored market-tone snapshots from theta's SQLite signal store.

Optional parameters:
- `db` (string path)
- `symbol` (string)
- `limit` (integer)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh signals history [--db <db>] [--symbol <symbol>] [--limit <limit>] [--json]
```

Guidance:
- Use this tool to verify that `capture_signals` is writing expected rows.
- Prefer `json=true` when the result will be machine-read.

### Tool: iv_rank

Purpose: compute front ATM IV Rank for a symbol from stored signal snapshots.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)

Optional parameters:
- `db` (string path)
- `limit` (integer)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh signals iv-rank --symbol <symbol> [--db <db>] [--limit <limit>] [--json]
```

### Tool: market_extreme

Purpose: measure the latest market-tone metrics for a symbol against recent stored history and return z-score style extreme readings.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)

Optional parameters:
- `db` (string path)
- `limit` (integer)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh signals extreme --symbol <symbol> [--db <db>] [--limit <limit>] [--json]
```

### Tool: relative_extreme

Purpose: compare one symbol's latest extreme metrics against a benchmark symbol and return both current spreads and z-score spreads.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)

Optional parameters:
- `benchmark` (string, default `QQQ.US`)
- `db` (string path)
- `limit` (integer)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh signals relative-extreme --symbol <symbol> [--benchmark <benchmark>] [--db <db>] [--limit <limit>] [--json]
```
