---
name: theta-ops
description: Use when the user wants theta's operational commands through `theta ops`, currently focused on `account-monitor` for recurring portfolio and risk snapshot capture.
---

# theta-ops

This skill is intended to compile cleanly into a Tellar `SKILL.json` with explicit tools.

Use this skill only for the `theta ops` command group:

```bash
./scripts/theta.sh ops --help
```

## Shared rules

- Always execute through `./scripts/theta.sh ops ...`.
- If the release binary is missing, build with `cargo build --release --bin theta`.
- Default account is `firstrade`.
- Default interval is 300 seconds.
- `--market-hours-only` takes explicit boolean value (`true` or `false`).
- `--once` is for one-shot capture and should not be combined with `--loop`.
- Default DB is `~/.theta/portfolio.db`.

## Tool definitions

### Tool: account_monitor_once

Purpose: run one immediate monitoring sample and persist one row.

Optional parameters:
- `account` (string)
- `db` (string path)
- `market_hours_only` (boolean; pass explicitly)

Command template:

```bash
./scripts/theta.sh ops account-monitor --once [--account <account>] [--db <db>] [--market-hours-only <true|false>]
```

### Tool: account_monitor_loop

Purpose: run recurring monitoring sampler.

Optional parameters:
- `account` (string)
- `db` (string path)
- `every_seconds` (integer)
- `market_hours_only` (boolean; pass explicitly)

Command template:

```bash
./scripts/theta.sh ops account-monitor --loop [--every-seconds <every_seconds>] [--account <account>] [--db <db>] [--market-hours-only <true|false>]
```
