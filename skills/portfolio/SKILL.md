---
name: portfolio
description: Use when the user wants theta's portfolio ledger for account snapshots, trade recording, position inspection, strategy and margin reports, settlement flows, or account cash events (deposit/withdraw/dividend).
---

# portfolio

Use this skill only for theta's `portfolio` binary via wrapper:

```bash
./scripts/portfolio.sh --help
```

## Shared rules

- Always execute through `./scripts/portfolio.sh ...`.
- If the release binary is missing, build with `cargo build --release --bin portfolio`.
- Optional global flags:
  - `--db <path>` right after `./scripts/portfolio.sh`
  - `--account <account_id>` right after optional `--db`
- `strategies` and `report` require an existing account snapshot.
- `settle-expiries --apply` should be preceded by a dry-run and fails if settlement prices or position validation are incomplete.

## Tool definitions

### Tool: account_set

Purpose: append an account snapshot used by strategy/report calculations.

Required parameters:
- `trade_date_cash` (number)
- `settled_cash` (number)

Optional parameters:
- `option_buying_power` (number)
- `stock_buying_power` (number)
- `margin_loan` (number)
- `short_market_value` (number)
- `margin` (boolean flag)
- `db` (string path)
- `account` (string account id)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] account set --trade-date-cash <trade_date_cash> --settled-cash <settled_cash> [--option-buying-power <option_buying_power>] [--stock-buying-power <stock_buying_power>] [--margin-loan <margin_loan>] [--short-market-value <short_market_value>] [--margin]
```

### Tool: account_show

Purpose: show latest account snapshot.

Optional parameters:
- `json` (boolean)
- `db` (string path)
- `account` (string account id)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] account show [--json]
```

### Tool: account_history

Purpose: list account snapshot history.

Optional parameters:
- `limit` (integer)
- `json` (boolean)
- `db` (string path)
- `account` (string account id)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] account history [--limit <limit>] [--json]
```

### Tool: trade_buy

Purpose: record buy trade for stock/options.

Required parameters:
- `symbol` (string)
- `underlying` (string)
- `quantity` (integer, positive)
- `price` (number, positive)
- `side` (enum: `call`, `put`, `stock`)

Conditional required parameters:
- `strike` when `side` is option
- `expiry` when `side` is option (`YYYY-MM-DD`)

Optional parameters:
- `commission` (number)
- `date` (string `YYYY-MM-DD`)
- `notes` (string)
- `db` (string path)
- `account` (string account id)

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade buy --symbol <symbol> --underlying <underlying> --quantity <quantity> --price <price> --side <side> [--strike <strike>] [--expiry <expiry>] [--commission <commission>] [--date <date>] [--notes <notes>]
```

### Tool: trade_sell

Purpose: record sell trade for stock/options.

Required and optional parameters are the same as `trade_buy`.

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade sell --symbol <symbol> --underlying <underlying> --quantity <quantity> --price <price> --side <side> [--strike <strike>] [--expiry <expiry>] [--commission <commission>] [--date <date>] [--notes <notes>]
```

### Tool: trade_exercise

Purpose: record long option exercise and stock delivery leg.

Required parameters:
- `symbol`, `underlying`, `quantity`, `side`, `strike`, `expiry`

Optional parameters:
- `commission`, `date`, `notes`, `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade exercise --symbol <symbol> --underlying <underlying> --quantity <quantity> --side <side> --strike <strike> --expiry <expiry> [--commission <commission>] [--date <date>] [--notes <notes>]
```

### Tool: trade_assign

Purpose: record short option assignment and stock delivery leg.

Required/optional parameters are the same as `trade_exercise`.

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade assign --symbol <symbol> --underlying <underlying> --quantity <quantity> --side <side> --strike <strike> --expiry <expiry> [--commission <commission>] [--date <date>] [--notes <notes>]
```

### Tool: trade_expire

Purpose: record option expiry at zero value.

Required parameters:
- `symbol` (string)
- `underlying` (string)
- `quantity` (integer)
- `side` (enum: `call`, `put`)
- `position` (enum: `long`, `short`)
- `strike` (number)
- `expiry` (string `YYYY-MM-DD`)

Optional parameters:
- `commission`, `date`, `notes`, `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade expire --symbol <symbol> --underlying <underlying> --quantity <quantity> --side <side> --position <position> --strike <strike> --expiry <expiry> [--commission <commission>] [--date <date>] [--notes <notes>]
```

### Tool: settle_expiries

Purpose: dry-run or apply batch settlement for expired options.

Required parameters:
- `settlement_prices` (array of `SYMBOL=PRICE`)

Optional parameters:
- `date`, `underlying`, `apply`, `json`, `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade settle-expiries [--date <date>] [--underlying <underlying>] --settlement-price <symbol_price_1> [--settlement-price <symbol_price_2> ...] [--apply] [--json]
```

### Tool: trade_list

Purpose: list historical trades with optional filters.

Optional parameters:
- `underlying`, `symbol`, `from`, `to`, `json`, `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade list [--underlying <underlying>] [--symbol <symbol>] [--from <from>] [--to <to>] [--json]
```

### Tool: trade_delete

Purpose: delete one trade by id.

Required parameters:
- `id` (integer)

Optional parameters:
- `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade delete <id>
```

### Tool: trade_deposit

Purpose: record cash deposit.

Required parameters:
- `amount` (number)

Optional parameters:
- `date`, `notes`, `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade deposit --amount <amount> [--date <date>] [--notes <notes>]
```

### Tool: trade_withdraw

Purpose: record cash withdrawal.

Required parameters:
- `amount` (number)

Optional parameters:
- `date`, `notes`, `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade withdraw --amount <amount> [--date <date>] [--notes <notes>]
```

### Tool: trade_dividend

Purpose: record dividend cash event.

Required parameters:
- `underlying` (string)
- `amount` (number)

Optional parameters:
- `date`, `notes`, `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] trade dividend --underlying <underlying> --amount <amount> [--date <date>] [--notes <notes>]
```

### Tool: positions

Purpose: show reconstructed open positions.

Optional parameters:
- `underlying`, `json`, `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] positions [--underlying <underlying>] [--json]
```

### Tool: strategies

Purpose: identify strategies and margin usage.

Optional parameters:
- `underlying`, `json`, `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] strategies [--underlying <underlying>] [--json]
```

### Tool: report

Purpose: produce portfolio report with live/offline pricing and Greeks.

Optional parameters:
- `underlying`, `offline`, `json`, `db`, `account`

Command template:

```bash
./scripts/portfolio.sh [--db <db>] [--account <account>] report [--underlying <underlying>] [--offline] [--json]
```
