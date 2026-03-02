---
name: portfolio
description: Use when the user wants theta's portfolio ledger for account snapshots, trade recording, position inspection, strategy and margin reports, or expiry settlement workflows.
---

# portfolio

This skill is intended to compile cleanly into a Tellar `SKILL.json` with multiple explicit tools.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `portfolio` bin in theta:

```bash
./scripts/portfolio.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer a separate Tellar tool for each workflow below.
Do not collapse everything into one generic tool if the compiler can preserve the tool list.
Each tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/portfolio.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin portfolio` first.
- Optional `db` means append `--db <path>` immediately after `./scripts/portfolio.sh`.
- `strategies` and `report` require at least one stored account snapshot.
- Lifecycle writes validate that matching open option positions exist before writing.
- Multi-step writes are wrapped in SQLite transactions.
- `settle-expiries --apply` refuses partial settlement if settlement prices are missing or validation fails.

## Tool definitions

### Tool: account_set

Purpose: create or append an account snapshot used by `strategies` and `report`.

Required parameters:
- `cash_balance` (number)

Optional parameters:
- `option_buying_power` (number)
- `cash_account` (boolean)
- `at` (string, RFC3339 timestamp)
- `notes` (string)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] account set --cash-balance <cash_balance> [--option-buying-power <option_buying_power>] [--cash-account] [--at <at>] [--notes <notes>]
```

### Tool: account_show

Purpose: show the latest account snapshot.

Optional parameters:
- `json` (boolean)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] account show [--json]
```

### Tool: account_history

Purpose: list recent account snapshots.

Optional parameters:
- `limit` (integer)
- `json` (boolean)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] account history [--limit <limit>] [--json]
```

### Tool: trade_buy

Purpose: record an opening or increasing buy trade for stock or options.

Required parameters:
- `symbol` (string)
- `underlying` (string)
- `quantity` (integer, positive)
- `price` (number, positive)
- `side` (string enum: `call`, `put`, `stock`)

Conditional required parameters:
- `strike` (number) when `side` is `call` or `put`
- `expiry` (string, YYYY-MM-DD) when `side` is `call` or `put`

Optional parameters:
- `commission` (number, non-negative)
- `date` (string, YYYY-MM-DD)
- `notes` (string)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] trade buy --symbol <symbol> --underlying <underlying> --quantity <quantity> --price <price> --side <side> [--strike <strike>] [--expiry <expiry>] [--commission <commission>] [--date <date>] [--notes <notes>]
```

### Tool: trade_sell

Purpose: record an opening or reducing sell trade for stock or options.

Required parameters:
- `symbol` (string)
- `underlying` (string)
- `quantity` (integer, positive)
- `price` (number, positive)
- `side` (string enum: `call`, `put`, `stock`)

Conditional required parameters:
- `strike` (number) when `side` is `call` or `put`
- `expiry` (string, YYYY-MM-DD) when `side` is `call` or `put`

Optional parameters:
- `commission` (number, non-negative)
- `date` (string, YYYY-MM-DD)
- `notes` (string)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] trade sell --symbol <symbol> --underlying <underlying> --quantity <quantity> --price <price> --side <side> [--strike <strike>] [--expiry <expiry>] [--commission <commission>] [--date <date>] [--notes <notes>]
```

### Tool: positions

Purpose: reconstruct and display current open positions from the ledger.

Optional parameters:
- `underlying` (string)
- `json` (boolean)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] positions [--underlying <underlying>] [--json]
```

### Tool: strategies

Purpose: identify strategies and compute margin-aware strategy output using the latest account snapshot.

Optional parameters:
- `underlying` (string)
- `json` (boolean)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] strategies [--underlying <underlying>] [--json]
```

### Tool: report

Purpose: produce the full portfolio report, including positions, strategies, margin, and Greeks.

Optional parameters:
- `underlying` (string)
- `offline` (boolean)
- `json` (boolean)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] report [--underlying <underlying>] [--offline] [--json]
```

### Tool: trade_exercise

Purpose: record a long option exercise. This closes the option and inserts the stock delivery leg.

Required parameters:
- `symbol` (string)
- `underlying` (string)
- `quantity` (integer, positive)
- `side` (string enum: `call`, `put`)
- `strike` (number)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `commission` (number, non-negative)
- `date` (string, YYYY-MM-DD)
- `notes` (string)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] trade exercise --symbol <symbol> --underlying <underlying> --quantity <quantity> --side <side> --strike <strike> --expiry <expiry> [--commission <commission>] [--date <date>] [--notes <notes>]
```

### Tool: trade_assign

Purpose: record a short option assignment. This closes the option and inserts the stock delivery leg.

Required parameters:
- `symbol` (string)
- `underlying` (string)
- `quantity` (integer, positive)
- `side` (string enum: `call`, `put`)
- `strike` (number)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `commission` (number, non-negative)
- `date` (string, YYYY-MM-DD)
- `notes` (string)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] trade assign --symbol <symbol> --underlying <underlying> --quantity <quantity> --side <side> --strike <strike> --expiry <expiry> [--commission <commission>] [--date <date>] [--notes <notes>]
```

### Tool: trade_expire

Purpose: record an option expiry at zero value.

Required parameters:
- `symbol` (string)
- `underlying` (string)
- `quantity` (integer, positive)
- `side` (string enum: `call`, `put`)
- `position` (string enum: `long`, `short`)
- `strike` (number)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `commission` (number, non-negative)
- `date` (string, YYYY-MM-DD)
- `notes` (string)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] trade expire --symbol <symbol> --underlying <underlying> --quantity <quantity> --side <side> --position <position> --strike <strike> --expiry <expiry> [--commission <commission>] [--date <date>] [--notes <notes>]
```

### Tool: settle_expiries

Purpose: scan expired open option positions and classify them as `expire`, `exercise`, or `assignment` using provided settlement prices. Dry-run by default; `apply` writes the generated settlement events.

Required parameters:
- `settlement_prices` (array of strings in `SYMBOL=PRICE` form)

Optional parameters:
- `date` (string, YYYY-MM-DD)
- `underlying` (string)
- `apply` (boolean)
- `json` (boolean)
- `db` (string path)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] trade settle-expiries [--date <date>] [--underlying <underlying>] --settlement-price <symbol_price_1> [--settlement-price <symbol_price_2> ...] [--apply] [--json]
```

Validation rules:
- Dry-run should be preferred before `--apply`.
- If any settlement price is missing, `--apply` fails.
- If any matching position is missing or quantity is insufficient, `--apply` fails.
- `--apply` performs the entire batch in one SQLite transaction.
