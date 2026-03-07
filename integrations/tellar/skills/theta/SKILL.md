---
name: theta
description: Use when the user wants help choosing or using theta's unified CLI entrypoint, including routing a task to `theta snapshot`, `theta portfolio`, `theta signals`, `theta structure`, or `theta ops`.
---

# theta

Use this skill as the router for theta's unified CLI.

Primary entrypoint:

```bash
./scripts/theta.sh --help
```

## Routing

- `theta snapshot ...`: market data, chain analysis, screening, and strategy ideas
  - `snapshot sell-opportunities` also supports filtering and grouping by return basis (`collateral-return`, `premium-yield`, `max-risk-return`, `theta-carry-run-rate`)
- `theta portfolio ...`: ledger, account snapshots, trades, positions, strategies, and reports
- `theta signals ...`: capture snapshots, inspect history, IV rank, extreme, and relative-extreme views
- `theta structure ...`: skew, smile, put/call bias, market tone, and term structure
- `theta ops ...`: recurring operational workflows such as account monitoring

## Shared rules

- Always execute through `./scripts/theta.sh ...`.
- If the release binary is missing, build with `cargo build --release --bin theta`.
- Prefer a more specific theta sub-skill when the user intent clearly maps to one domain.

## Tool definitions

### Tool: theta_help

Purpose: show the unified theta command tree and use it for routing when the correct domain command is unclear.

Optional parameters:
- none

Command template:

```bash
./scripts/theta.sh --help
```

Guidance:
- Use this tool when the caller asks where a workflow lives in theta.
- After identifying the correct area, prefer one of the domain skills instead of repeatedly calling the root help.
