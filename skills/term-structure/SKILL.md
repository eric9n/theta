---
name: term-structure
description: Use when the user wants theta to compute ATM IV term structure across the next several expiries for a symbol.
---

# term-structure

This skill is intended to compile cleanly into a Tellar `SKILL.json`.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `term-structure` bin in theta:

```bash
./scripts/term-structure.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer one explicit tool named `term_structure`.
Do not invent extra subcommands; this bin is a single configurable query.
The tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/term-structure.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin term-structure` first.
- Live data requires `LONGPORT_APP_KEY`, `LONGPORT_APP_SECRET`, and `LONGPORT_ACCESS_TOKEN`.

## Tool definitions

### Tool: term_structure

Purpose: compute ATM IV term structure across the next several expiries for a symbol.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)

Optional parameters:
- `expiries_limit` (integer)
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_market_price` (boolean)
- `json` (boolean)

Command template:

```bash
./scripts/term-structure.sh --symbol <symbol> [--expiries-limit <expiries_limit>] [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--json]
```

Guidance:
- Use this tool when the caller wants ATM IV across several expiries, not a single front-expiry signal.
- Prefer `json=true` when the result will be machine-read.
