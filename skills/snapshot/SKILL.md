---
name: snapshot
description: Use when the user wants theta's main snapshot CLI for market data lookup, option analytics, option-chain screening, mispricing scans, or multi-strategy sell opportunity workflows.
---

# snapshot

This skill is intended to compile cleanly into a Tellar `SKILL.json` with multiple explicit tools.
Use the bundled wrapper script instead of `cargo run`.

Use this skill only for the `snapshot` bin in theta:

```bash
./scripts/snapshot.sh --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer separate Tellar tools for the workflows below.
Do not collapse everything into one generic command if the compiler can preserve the tool list.
Each tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/snapshot.sh ...`.
- The wrapper script locates the theta repository root relative to the skill directory and executes the built release binary.
- If the release binary is missing, the script fails and instructs the user to build `cargo build --release --bin snapshot` first.
- Optional `json` means append `--json` at the end of the command.
- Live data workflows require `LONGPORT_APP_KEY`, `LONGPORT_APP_SECRET`, and `LONGPORT_ACCESS_TOKEN`.
- Date values use `YYYY-MM-DD`.
- Prefer structured tools below instead of inventing new subcommands or flags.

## Tool definitions

### Tool: config

Purpose: verify whether LongPort API credentials are configured.

Optional parameters:
- none

Command template:

```bash
./scripts/snapshot.sh config
```

### Tool: probe

Purpose: verify API access with a basic quote request.

Optional parameters:
- `symbol` (string, default `TSLA.US`)

Command template:

```bash
./scripts/snapshot.sh probe [--symbol <symbol>]
```

### Tool: stock_quote

Purpose: fetch the realtime quote for a stock.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)

Command template:

```bash
./scripts/snapshot.sh stock-quote --symbol <symbol>
```

### Tool: option_expiries

Purpose: list available option expiries for an underlying.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)

Command template:

```bash
./scripts/snapshot.sh option-expiries --symbol <symbol>
```

### Tool: option_quote

Purpose: fetch a single option quote with full provider fields.

Required parameters:
- `symbol` (string, e.g. `TSLA260320C00400000.US`)

Optional parameters:
- `json` (boolean)

Command template:

```bash
./scripts/snapshot.sh option-quote --symbol <symbol> [--json]
```

### Tool: option_chain

Purpose: fetch a normalized option chain for one expiry, with optional diagnostics filtering.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `only_liquid` (boolean)
- `exclude_abnormal` (boolean)
- `exclude_near_expiry` (boolean)
- `json` (boolean)

Command template:

```bash
./scripts/snapshot.sh option-chain --symbol <symbol> --expiry <expiry> [--only-liquid] [--exclude-abnormal] [--exclude-near-expiry] [--json]
```

### Tool: analyze_option

Purpose: analyze a single option contract with locally computed Greeks.

Required parameters:
- `symbol` (string, e.g. `TSLA260320C00400000.US`)

Optional parameters:
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_option_price` (number)
- `iv_from_market_price` (boolean)
- `show_iv_diff` (boolean)
- `use_provider_greeks` (boolean)
- `json` (boolean)

Command template:

```bash
./scripts/snapshot.sh analyze-option --symbol <symbol> [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-option-price <iv_from_option_price>] [--iv-from-market-price] [--show-iv-diff] [--use-provider-greeks] [--json]
```

### Tool: analyze_chain

Purpose: analyze a full option chain for one expiry using local Greeks and chain-level filtering.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_market_price` (boolean)
- `side` (string enum: `call`, `put`)
- `min_strike` (number)
- `max_strike` (number)
- `min_delta` (number)
- `max_delta` (number)
- `min_theta` (number)
- `max_theta` (number)
- `min_vega` (number)
- `max_vega` (number)
- `min_iv` (number)
- `max_iv` (number)
- `min_option_price` (number)
- `max_option_price` (number)
- `min_otm_percent` (number)
- `max_otm_percent` (number)
- `only_liquid` (boolean)
- `exclude_abnormal` (boolean)
- `exclude_near_expiry` (boolean)
- `sort_by` (string enum: `delta`, `theta`, `vega`, `iv`, `strike`)
- `limit` (integer)
- `json` (boolean)

Command template:

```bash
./scripts/snapshot.sh analyze-chain --symbol <symbol> --expiry <expiry> [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--side <side>] [--min-strike <min_strike>] [--max-strike <max_strike>] [--min-delta <min_delta>] [--max-delta <max_delta>] [--min-theta <min_theta>] [--max-theta <max_theta>] [--min-vega <min_vega>] [--max-vega <max_vega>] [--min-iv <min_iv>] [--max-iv <max_iv>] [--min-option-price <min_option_price>] [--max-option-price <max_option_price>] [--min-otm-percent <min_otm_percent>] [--max-otm-percent <max_otm_percent>] [--only-liquid] [--exclude-abnormal] [--exclude-near-expiry] [--sort-by <sort_by>] [--limit <limit>] [--json]
```

### Tool: mispricing

Purpose: scan one expiry for fair-value and implied-volatility deviations.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_market_price` (boolean)
- `side` (string enum: `call`, `put`)
- `direction` (string enum: `overpriced`, `underpriced`)
- `iv_direction` (string enum: `higher`, `lower`)
- `min_open_interest` (integer)
- `min_volume` (integer)
- `min_abs_mispricing_percent` (number)
- `min_abs_iv_diff_percent` (number)
- `sort_by` (string enum: `mispricing`, `iv-diff`)
- `group_by_side` (boolean)
- `summary_only` (boolean)
- `top_per_side` (integer)
- `limit` (integer)
- `json` (boolean)

Command template:

```bash
./scripts/snapshot.sh mispricing --symbol <symbol> --expiry <expiry> [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--side <side>] [--direction <direction>] [--iv-direction <iv_direction>] [--min-open-interest <min_open_interest>] [--min-volume <min_volume>] [--min-abs-mispricing-percent <min_abs_mispricing_percent>] [--min-abs-iv-diff-percent <min_abs_iv_diff_percent>] [--sort-by <sort_by>] [--group-by-side] [--summary-only] [--top-per-side <top_per_side>] [--limit <limit>] [--json]
```

### Tool: sell_opportunities

Purpose: aggregate sell-oriented strategy candidates across single-leg, vertical, and optional cross-expiry setups.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `rate` (number)
- `dividend` (number)
- `iv` (number)
- `iv_from_market_price` (boolean)
- `direction` (string enum: `overpriced`, `underpriced`)
- `iv_direction` (string enum: `higher`, `lower`)
- `min_open_interest` (integer)
- `min_volume` (integer)
- `min_abs_mispricing_percent` (number)
- `min_abs_iv_diff_percent` (number)
- `min_premium_or_credit` (number)
- `max_risk` (number)
- `min_annualized_return` (number)
- `max_annualized_return` (number)
- `strategy` (string, repeatable)
- `exclude_strategy` (string, repeatable)
- `include_calendars` (boolean)
- `include_diagonals` (boolean)
- `min_days_gap` (integer)
- `max_days_gap` (integer)
- `min_strike_gap` (number)
- `max_strike_gap` (number)
- `sort_by` (string enum: `annualized-return`, `mispricing`, `iv-diff`)
- `limit_per_strategy` (integer)
- `group_by_strategy` (boolean)
- `summary_only` (boolean)
- `limit` (integer)
- `json` (boolean)

Command template:

```bash
./scripts/snapshot.sh sell-opportunities --symbol <symbol> --expiry <expiry> [--rate <rate>] [--dividend <dividend>] [--iv <iv>] [--iv-from-market-price] [--direction <direction>] [--iv-direction <iv_direction>] [--min-open-interest <min_open_interest>] [--min-volume <min_volume>] [--min-abs-mispricing-percent <min_abs_mispricing_percent>] [--min-abs-iv-diff-percent <min_abs_iv_diff_percent>] [--min-premium-or-credit <min_premium_or_credit>] [--max-risk <max_risk>] [--min-annualized-return <min_annualized_return>] [--max-annualized-return <max_annualized_return>] [--strategy <strategy>]... [--exclude-strategy <exclude_strategy>]... [--include-calendars] [--include-diagonals] [--min-days-gap <min_days_gap>] [--max-days-gap <max_days_gap>] [--min-strike-gap <min_strike_gap>] [--max-strike-gap <max_strike_gap>] [--sort-by <sort_by>] [--limit-per-strategy <limit_per_strategy>] [--group-by-strategy] [--summary-only] [--limit <limit>] [--json]
```
