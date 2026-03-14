---
name: theta
description: Use this skill for the theta CLI: TSLA signals, chain analysis, portfolio risk, and health checks.
---

# theta

Use this skill for the `theta` CLI.

Primary entrypoint:

```bash
theta --help
```

Default assumptions:

- Market-analysis commands default to `TSLA.US` unless the user says otherwise.
- `theta-daemon` must be running for live market-data commands.
- LongPort credentials belong to the daemon, not the CLI process.
- Default socket: `${HOME}/.theta/run/theta.sock`
- Default config: `~/.theta/config.json`

Daily workflow:

1. Start with `theta portfolio positions` and `theta portfolio report` to review current risk.
2. Then use `theta signals monitor`, `theta signals iv-rank`, and `theta signals extreme`.
3. If the user wants detail behind a signal, use `theta structure ...`.
4. If the user wants to inspect a tradable expiry or legs, use `theta snapshot option-expiries` and `theta snapshot analyze-chain --expiry ...`.
5. If the user already knows the structure, use one of the four retained strategy screeners.

Routing by intent:

- `theta signals ...`
  Use for the main monitoring workflow: `capture`, `history`, `monitor`, `iv-rank`, `extreme`.
- `theta snapshot ...`
  Use for live chain inspection and the four retained strategy screeners:
  `calc`, `stock-quote`, `option-expiries`, `option-chain`, `option-quote`, `analyze-option`, `analyze-chain`, `bull-put-spread`, `bull-call-spread`, `calendar-call-spread`, `diagonal-call-spread`.
- `theta portfolio ...`
  Use for account snapshots, trade recording, positions, strategy identification, and portfolio Greeks via `report`.
- `theta structure ...`
  Use when the user wants the detailed view behind `signals`, like `skew`, `smile`, `put-call-bias`, `market-tone`, or `term-structure`.
- `theta ops ...`
  Use for `health-check` and recurring account monitoring.

General rules:

- Prefer invoking `theta ...` from `PATH`.
- If `theta` is not on `PATH`, resolve the binary location from the user's install prefix, e.g. `${PREFIX}/theta`, before assuming any default path.
- If the binary is missing, install or update it with:

```bash
curl -fsSL https://raw.githubusercontent.com/eric9n/theta/main/deploy/install.sh | sudo bash
```

- The installer default is `PREFIX=/usr/local/bin`, but users may override both `PREFIX` and `SHARE_DIR`; do not hardcode those paths unless the environment or the user explicitly confirms them.
- Set `THETA_SOCKET_PATH` to override the socket location if the daemon is configured elsewhere.

Common commands:

```bash
theta signals monitor
theta signals iv-rank
theta signals extreme
theta structure skew --expiry 2026-03-20
theta structure term-structure
theta structure market-tone --expiry 2026-03-20
theta snapshot option-expiries
theta snapshot analyze-chain --expiry 2026-03-20
theta snapshot bull-put-spread --expiry 2026-03-20
theta snapshot calendar-call-spread --near-expiry 2026-03-20 --far-expiry 2026-06-18
theta portfolio account set --cash-balance 50000 --option-buying-power 100000
theta portfolio positions
theta portfolio report --offline
theta ops health-check
```
