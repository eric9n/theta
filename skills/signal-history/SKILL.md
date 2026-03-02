---
name: signal-history
description: Use when the user wants to inspect recent stored market-tone snapshots from theta's SQLite signal store.
---

# signal-history

This skill is intended to compile cleanly into a Tellar `SKILL.json`.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `signal-history` bin in theta:

```bash
./scripts/signal-history.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer one explicit tool named `signal_history`.
Do not invent extra subcommands; this bin is a single historical query.
The tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/signal-history.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin signal-history` first.
- If no `db` is passed, the binary defaults to `~/.theta/signals.db`.

## Tool definitions

### Tool: signal_history

Purpose: inspect recent stored market-tone snapshots from theta's SQLite signal store.

Optional parameters:
- `db` (string path)
- `symbol` (string)
- `limit` (integer)
- `json` (boolean)

Command template:

```bash
./scripts/signal-history.sh [--db <db>] [--symbol <symbol>] [--limit <limit>] [--json]
```

Guidance:
- Use this tool to verify that `capture-signals` is writing expected rows.
- Prefer `json=true` when the result will be machine-read.
