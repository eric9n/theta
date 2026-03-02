---
name: iv-rank
description: Use when the user wants theta to compute front ATM IV Rank from stored signal snapshots for a symbol.
---

# iv-rank

This skill is intended to compile cleanly into a Tellar `SKILL.json`.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `iv-rank` bin in theta:

```bash
./scripts/iv-rank.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer one explicit tool named `iv_rank`.
Do not invent extra subcommands; this bin is a single historical query.
The tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/iv-rank.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin iv-rank` first.
- If no `db` is passed, the binary defaults to `~/.theta/signals.db`.
- This skill requires historical rows captured by `capture-signals`; it is not useful on an empty database.

## Tool definitions

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
./scripts/iv-rank.sh --symbol <symbol> [--db <db>] [--limit <limit>] [--json]
```

Guidance:
- Use the default `limit` unless the caller explicitly wants a different historical window.
- Prefer `json=true` when the result will be machine-read.
- If the database has no stored rows for the symbol, the command returns no result rather than inventing a rank.
