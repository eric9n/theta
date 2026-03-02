---
name: relative-extreme
description: Use when the user wants theta to compare one symbol's current extreme metrics against a benchmark symbol such as TSLA versus QQQ.
---

# relative-extreme

This skill is intended to compile cleanly into a Tellar `SKILL.json`.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `relative-extreme` bin in theta:

```bash
./scripts/relative-extreme.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer one explicit tool named `relative_extreme`.
Do not invent extra subcommands; this bin is a single historical comparison query.
The tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/relative-extreme.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin relative-extreme` first.
- If no `db` is passed, the binary defaults to `~/.theta/signals.db`.
- If no `benchmark` is passed, the binary defaults to `QQQ.US`.
- This skill requires historical rows captured by `capture-signals`; it is not useful on an empty database.

## Tool definitions

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
./scripts/relative-extreme.sh --symbol <symbol> [--benchmark <benchmark>] [--db <db>] [--limit <limit>] [--json]
```

Guidance:
- Use the default benchmark unless the caller explicitly wants a different comparison symbol.
- Use the default `limit` unless the caller explicitly wants a different historical window.
- Prefer `json=true` when the result will be machine-read.
