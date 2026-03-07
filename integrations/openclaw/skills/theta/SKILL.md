# theta

Use this skill for the unified `theta` CLI.

Primary entrypoint:

```bash
/root/theta/target/release/theta --help
```

Routing:

- `theta snapshot ...`: market data, chain analysis, mispricing, and strategy screening
- `theta portfolio ...`: ledger, trades, positions, strategies, reports, and account history
- `theta signals ...`: signal capture, history, IV rank, extreme, and relative-extreme
- `theta structure ...`: skew, smile, put/call bias, market tone, and term structure
- `theta ops ...`: operational workflows such as account monitoring

General rules:

- Always use `/root/theta/target/release/theta ...` on the VPS.
- If the binary is missing, build it with:

```bash
cd /root/theta && cargo build --release --bin theta
```

- Market-data commands require the local `theta-daemon` to be running and reachable at `${HOME}/.theta/run/theta.sock` by default.
- Set `THETA_SOCKET_PATH` to override the socket location if the daemon is configured elsewhere.
- LongPort credentials are required by `theta-daemon`, not by the `theta` CLI process itself.
- Default config path is `~/.theta/config.json`.

Useful commands:

```bash
/root/theta/target/release/theta snapshot stock-quote --symbol TSLA.US
/root/theta/target/release/theta snapshot analyze-chain --symbol TSLA.US --expiry 2026-03-20
/root/theta/target/release/theta snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20
/root/theta/target/release/theta snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20 --return-basis premium-yield
/root/theta/target/release/theta snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20 --group-by-return-basis
/root/theta/target/release/theta portfolio positions
/root/theta/target/release/theta portfolio report --offline
/root/theta/target/release/theta portfolio account monitor-history --limit 20
/root/theta/target/release/theta signals history --limit 20
/root/theta/target/release/theta structure market-tone --symbol TSLA.US --expiry 2026-03-20
/root/theta/target/release/theta ops account-monitor --once --account firstrade
```

Sell-opportunities notes:

- `--return-basis premium-yield` isolates covered-call style premium yield candidates.
- `--return-basis collateral-return` isolates cash-secured-put style collateral returns.
- `--return-basis max-risk-return` isolates vertical spread style max-risk returns.
- `--return-basis theta-carry-run-rate` isolates calendar/diagonal carry run-rate candidates.
- `--group-by-return-basis` summarizes the merged candidate set by comparable return semantics.
