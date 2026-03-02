---
name: market-extreme
description: Use when the user wants theta to compare current market-tone metrics against stored history using mean, standard deviation, and z-score style extreme detection.
---

# market-extreme

This skill is intended to compile cleanly into a Tellar `SKILL.json`.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `market-extreme` bin in theta:

```bash
./scripts/market-extreme.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer one explicit tool named `market_extreme`.
Do not invent extra subcommands; this bin is a single historical query.
The tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/market-extreme.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin market-extreme` first.
- If no `db` is passed, the binary defaults to `~/.theta/signals.db`.
- This skill requires historical rows captured by `capture-signals`; it is not useful on an empty database.

## Tool definitions

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
./scripts/market-extreme.sh --symbol <symbol> [--db <db>] [--limit <limit>] [--json]
```

Guidance:
- Use the default `limit` unless the caller explicitly wants a shorter or longer comparison window.
- Prefer `json=true` when the result will be machine-read.
- If the database has no stored rows for the symbol, the command returns no result rather than synthesizing one.
