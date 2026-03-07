---
name: theta-snapshot
description: Use when the user wants theta's market snapshot workflows through `theta snapshot`, including quotes, expiries, option chains, option analytics, mispricing, and strategy screening.
---

# theta-snapshot

This skill is intended to compile cleanly into a Tellar `SKILL.json` with multiple explicit tools.

Use this skill only for the `theta snapshot` command:

```bash
./scripts/theta.sh snapshot --help
```

## Compilation intent

When compiling this skill into `SKILL.json`, prefer separate Tellar tools for the workflows below.
Do not collapse everything into one generic command if the compiler can preserve the tool list.
Each tool should execute the exact command template shown here.

## Shared rules

- All tool commands should call `./scripts/theta.sh snapshot ...`.
- If the release binary is missing, build with `cargo build --release --bin theta`.
- Optional `json` means append `--json` at the end of the command.
- Live data workflows require the local `theta-daemon` to be running and reachable at `${HOME}/.theta/run/theta.sock` by default.
- Set `THETA_SOCKET_PATH` to override the socket location if the daemon is configured elsewhere.
- LongPort credentials are required by `theta-daemon`, not by the `theta` CLI process itself.
- Date values use `YYYY-MM-DD`.
- Prefer structured tools below instead of inventing new subcommands or flags.

## Tool definitions

### Tool: config

Purpose: verify whether the local market-data daemon is reachable for live snapshot workflows.

Optional parameters:
- none

Command template:

```bash
./scripts/theta.sh snapshot config
```

### Tool: probe

Purpose: verify API access with a basic quote request.

Optional parameters:
- `symbol` (string, default `TSLA.US`)

Command template:

```bash
./scripts/theta.sh snapshot probe {{#symbol}}--symbol {{symbol}}{{/symbol}}
```

### Tool: stock_quote

Purpose: fetch the realtime quote for a stock.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)

Command template:

```bash
./scripts/theta.sh snapshot stock-quote --symbol {{symbol}}
```

### Tool: option_expiries

Purpose: list available option expiries for an underlying.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)

Command template:

```bash
./scripts/theta.sh snapshot option-expiries --symbol {{symbol}}
```

### Tool: option_quote

Purpose: fetch a single option quote with full provider fields.

Required parameters:
- `symbol` (string, e.g. `TSLA260320C00400000.US`)

Optional parameters:
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh snapshot option-quote --symbol {{symbol}} {{#json}}--json{{/json}}
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
./scripts/theta.sh snapshot option-chain --symbol {{symbol}} --expiry {{expiry}} {{#only_liquid}}--only-liquid{{/only_liquid}} {{#exclude_abnormal}}--exclude-abnormal{{/exclude_abnormal}} {{#exclude_near_expiry}}--exclude-near-expiry{{/exclude_near_expiry}} {{#json}}--json{{/json}}
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
./scripts/theta.sh snapshot analyze-option --symbol {{symbol}} {{#rate}}--rate {{rate}}{{/rate}} {{#dividend}}--dividend {{dividend}}{{/dividend}} {{#iv}}--iv {{iv}}{{/iv}} {{#iv_from_option_price}}--iv-from-option-price {{iv_from_option_price}}{{/iv_from_option_price}} {{#iv_from_market_price}}--iv-from-market-price{{/iv_from_market_price}} {{#show_iv_diff}}--show-iv-diff{{/show_iv_diff}} {{#use_provider_greeks}}--use-provider-greeks{{/use_provider_greeks}} {{#json}}--json{{/json}}
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
./scripts/theta.sh snapshot analyze-chain --symbol {{symbol}} --expiry {{expiry}} {{#rate}}--rate {{rate}}{{/rate}} {{#dividend}}--dividend {{dividend}}{{/dividend}} {{#iv}}--iv {{iv}}{{/iv}} {{#iv_from_market_price}}--iv-from-market-price{{/iv_from_market_price}} {{#side}}--side {{side}}{{/side}} {{#min_strike}}--min-strike {{min_strike}}{{/min_strike}} {{#max_strike}}--max-strike {{max_strike}}{{/max_strike}} {{#min_delta}}--min-delta {{min_delta}}{{/min_delta}} {{#max_delta}}--max-delta {{max_delta}}{{/max_delta}} {{#min_theta}}--min-theta {{min_theta}}{{/min_theta}} {{#max_theta}}--max-theta {{max_theta}}{{/max_theta}} {{#min_vega}}--min-vega {{min_vega}}{{/min_vega}} {{#max_vega}}--max-vega {{max_vega}}{{/max_vega}} {{#min_iv}}--min-iv {{min_iv}}{{/min_iv}} {{#max_iv}}--max-iv {{max_iv}}{{/max_iv}} {{#min_option_price}}--min-option-price {{min_option_price}}{{/min_option_price}} {{#max_option_price}}--max-option-price {{max_option_price}}{{/max_option_price}} {{#min_otm_percent}}--min-otm-percent {{min_otm_percent}}{{/min_otm_percent}} {{#max_otm_percent}}--max-otm-percent {{max_otm_percent}}{{/max_otm_percent}} {{#only_liquid}}--only-liquid{{/only_liquid}} {{#exclude_abnormal}}--exclude-abnormal{{/exclude_abnormal}} {{#exclude_near_expiry}}--exclude-near-expiry{{/exclude_near_expiry}} {{#sort_by}}--sort-by {{sort_by}}{{/sort_by}} {{#limit}}--limit {{limit}}{{/limit}} {{#json}}--json{{/json}}
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
./scripts/theta.sh snapshot mispricing --symbol {{symbol}} --expiry {{expiry}} {{#rate}}--rate {{rate}}{{/rate}} {{#dividend}}--dividend {{dividend}}{{/dividend}} {{#iv}}--iv {{iv}}{{/iv}} {{#iv_from_market_price}}--iv-from-market-price{{/iv_from_market_price}} {{#side}}--side {{side}}{{/side}} {{#direction}}--direction {{direction}}{{/direction}} {{#iv_direction}}--iv-direction {{iv_direction}}{{/iv_direction}} {{#min_open_interest}}--min-open-interest {{min_open_interest}}{{/min_open_interest}} {{#min_volume}}--min-volume {{min_volume}}{{/min_volume}} {{#min_abs_mispricing_percent}}--min-abs-mispricing-percent {{min_abs_mispricing_percent}}{{/min_abs_mispricing_percent}} {{#min_abs_iv_diff_percent}}--min-abs-iv-diff-percent {{min_abs_iv_diff_percent}}{{/min_abs_iv_diff_percent}} {{#sort_by}}--sort-by {{sort_by}}{{/sort_by}} {{#group_by_side}}--group-by-side{{/group_by_side}} {{#summary_only}}--summary-only{{/summary_only}} {{#top_per_side}}--top-per-side {{top_per_side}}{{/top_per_side}} {{#limit}}--limit {{limit}}{{/limit}} {{#json}}--json{{/json}}
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
- `return_basis` (string enum, repeatable: `collateral-return`, `premium-yield`, `max-risk-return`, `theta-carry-run-rate`)
- `exclude_return_basis` (string enum, repeatable: `collateral-return`, `premium-yield`, `max-risk-return`, `theta-carry-run-rate`)
- `include_calendars` (boolean)
- `include_diagonals` (boolean)
- `min_days_gap` (integer)
- `max_days_gap` (integer)
- `min_strike_gap` (number)
- `max_strike_gap` (number)
- `sort_by` (string enum: `annualized-return`, `mispricing`, `iv-diff`)
- `limit_per_strategy` (integer)
- `group_by_strategy` (boolean)
- `group_by_return_basis` (boolean)
- `summary_only` (boolean)
- `limit` (integer)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh snapshot sell-opportunities --symbol {{symbol}} --expiry {{expiry}} {{#rate}}--rate {{rate}}{{/rate}} {{#dividend}}--dividend {{dividend}}{{/dividend}} {{#iv}}--iv {{iv}}{{/iv}} {{#iv_from_market_price}}--iv-from-market-price{{/iv_from_market_price}} {{#direction}}--direction {{direction}}{{/direction}} {{#iv_direction}}--iv-direction {{iv_direction}}{{/iv_direction}} {{#min_open_interest}}--min-open-interest {{min_open_interest}}{{/min_open_interest}} {{#min_volume}}--min-volume {{min_volume}}{{/min_volume}} {{#min_abs_mispricing_percent}}--min-abs-mispricing-percent {{min_abs_mispricing_percent}}{{/min_abs_mispricing_percent}} {{#min_abs_iv_diff_percent}}--min-abs-iv-diff-percent {{min_abs_iv_diff_percent}}{{/min_abs_iv_diff_percent}} {{#min_premium_or_credit}}--min-premium-or-credit {{min_premium_or_credit}}{{/min_premium_or_credit}} {{#max_risk}}--max-risk {{max_risk}}{{/max_risk}} {{#min_annualized_return}}--min-annualized-return {{min_annualized_return}}{{/min_annualized_return}} {{#max_annualized_return}}--max-annualized-return {{max_annualized_return}}{{/max_annualized_return}} {{#strategy}}--strategy '{{.}}' {{/strategy}} {{#exclude_strategy}}--exclude-strategy '{{.}}' {{/exclude_strategy}} {{#return_basis}}--return-basis {{.}} {{/return_basis}} {{#exclude_return_basis}}--exclude-return-basis {{.}} {{/exclude_return_basis}} {{#include_calendars}}--include-calendars{{/include_calendars}} {{#include_diagonals}}--include-diagonals{{/include_diagonals}} {{#min_days_gap}}--min-days-gap {{min_days_gap}}{{/min_days_gap}} {{#max_days_gap}}--max-days-gap {{max_days_gap}}{{/max_days_gap}} {{#min_strike_gap}}--min-strike-gap {{min_strike_gap}}{{/min_strike_gap}} {{#max_strike_gap}}--max-strike-gap {{max_strike_gap}}{{/max_strike_gap}} {{#sort_by}}--sort-by {{sort_by}}{{/sort_by}} {{#limit_per_strategy}}--limit-per-strategy {{limit_per_strategy}}{{/limit_per_strategy}} {{#group_by_strategy}}--group-by-strategy{{/group_by_strategy}} {{#group_by_return_basis}}--group-by-return-basis{{/group_by_return_basis}} {{#summary_only}}--summary-only{{/summary_only}} {{#limit}}--limit {{limit}}{{/limit}} {{#json}}--json{{/json}}
```

Guidance:
- Use `return_basis` when the caller wants a consistent yield basis instead of mixing premium yield, collateral return, max-risk return, and carry run-rate in one list.
- Use `group_by_return_basis` when the caller wants the merged sell opportunities summarized by comparable return semantics.
- When compiling to `SKILL.json`, preserve these three public parameters exactly:
  - `return_basis`
  - `exclude_return_basis`
  - `group_by_return_basis`
- `return_basis` and `exclude_return_basis` must remain visible in both the generated parameter schema and the command template.

Examples:

```bash
./scripts/theta.sh snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20 --return-basis premium-yield
./scripts/theta.sh snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20 --exclude-return-basis theta-carry-run-rate
./scripts/theta.sh snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20 --group-by-return-basis
```

### Tool: sell_opportunities_by_return_basis

Purpose: filter or group sell-oriented opportunities by comparable return semantics instead of mixing premium yield, collateral return, max-risk return, and theta carry run-rate in one list.

Required parameters:
- `symbol` (string, e.g. `TSLA.US`)
- `expiry` (string, YYYY-MM-DD)

Optional parameters:
- `return_basis` (string enum, repeatable: `collateral-return`, `premium-yield`, `max-risk-return`, `theta-carry-run-rate`)
- `exclude_return_basis` (string enum, repeatable: `collateral-return`, `premium-yield`, `max-risk-return`, `theta-carry-run-rate`)
- `group_by_return_basis` (boolean)
- `summary_only` (boolean)
- `limit` (integer)
- `json` (boolean)

Command template:

```bash
./scripts/theta.sh snapshot sell-opportunities --symbol {{symbol}} --expiry {{expiry}} {{#return_basis}}--return-basis {{.}} {{/return_basis}} {{#exclude_return_basis}}--exclude-return-basis {{.}} {{/exclude_return_basis}} {{#group_by_return_basis}}--group-by-return-basis{{/group_by_return_basis}} {{#summary_only}}--summary-only{{/summary_only}} {{#limit}}--limit {{limit}}{{/limit}} {{#json}}--json{{/json}}
```

Compilation note:
- Preserve `return_basis`, `exclude_return_basis`, and `group_by_return_basis` exactly in the generated `SKILL.json`.
