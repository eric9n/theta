---
name: account-monitor
description: Use when the user wants theta to run, inspect, or operate the account-monitor sampler that periodically captures account risk/equity snapshots into portfolio.db.
---

# account-monitor

Use this skill only for theta's `account-monitor` binary via wrapper:

```bash
./scripts/account-monitor.sh --help
```

## Shared rules

- Always execute through `./scripts/account-monitor.sh ...`.
- If the release binary is missing, build with `cargo build --release --bin account-monitor`.
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
./scripts/account-monitor.sh --once [--account <account>] [--db <db>] [--market-hours-only <true|false>]
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
./scripts/account-monitor.sh --loop [--every-seconds <every_seconds>] [--account <account>] [--db <db>] [--market-hours-only <true|false>]
```
