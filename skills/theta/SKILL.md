# theta

Use this skill for the unified `theta` CLI.

Primary entrypoint:

```bash
theta --help
```

Routing:

- `theta snapshot ...`: market data, chain analysis, mispricing, and strategy screening
- `theta portfolio ...`: ledger, trades, positions, strategies, reports, and account history
- `theta signals ...`: signal capture, history, IV rank, extreme, and relative-extreme
- `theta structure ...`: skew, smile, put/call bias, market tone, and term structure
- `theta ops ...`: operational workflows such as account monitoring

General rules:

- Prefer invoking `theta ...` from `PATH`.
- If `theta` is not on `PATH`, resolve the binary location from the user's install prefix, e.g. `${PREFIX}/theta`, before assuming any default path.
- If the binary is missing, install or update it with:

```bash
curl -fsSL https://raw.githubusercontent.com/eric9n/theta/main/deploy/install.sh | sudo bash
```

- The installer default is `PREFIX=/usr/local/bin`, but users may override both `PREFIX` and `SHARE_DIR`; do not hardcode those paths unless the environment or the user explicitly confirms them.
- `scripts/remote-theta.sh` is only an optional compatibility wrapper for users who want to hand-edit a remote execution helper. Do not assume it is the default execution path.
- Market-data commands require the local `theta-daemon` to be running and reachable at `${HOME}/.theta/run/theta.sock` by default.
- Set `THETA_SOCKET_PATH` to override the socket location if the daemon is configured elsewhere.
- LongPort credentials are required by `theta-daemon`, not by the `theta` CLI process itself.
- Default config path is `~/.theta/config.json`.

Useful commands:

```bash
theta snapshot stock-quote --symbol TSLA.US
theta snapshot analyze-chain --symbol TSLA.US --expiry 2026-03-20
theta snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20
theta snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20 --return-basis premium-yield
theta snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20 --group-by-return-basis
theta portfolio positions
theta portfolio report --offline
theta portfolio account monitor-history --limit 20
theta signals history --limit 20
theta structure market-tone --symbol TSLA.US --expiry 2026-03-20
theta ops account-monitor --once --account firstrade
```

Optional remote-wrapper pattern:

```bash
# Edit scripts/remote-theta.sh or set environment variables for your own host/prefix.
scripts/remote-theta.sh --host <your-host> -- --version
scripts/remote-theta.sh --host <your-host> -- portfolio report --offline
```

Sell-opportunities notes:

- `--return-basis premium-yield` isolates covered-call style premium yield candidates.
- `--return-basis collateral-return` isolates cash-secured-put style collateral returns.
- `--return-basis max-risk-return` isolates vertical spread style max-risk returns.
- `--return-basis theta-carry-run-rate` isolates calendar/diagonal carry run-rate candidates.
- `--group-by-return-basis` summarizes the merged candidate set by comparable return semantics.
