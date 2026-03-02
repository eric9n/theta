# theta

Personal CLI toolkit for option market snapshots, structure signals, and portfolio tracking.

## Binaries

- `snapshot`: market data, analytics, screening, and strategy workflows
- `capture-signals`: persist scheduled market tone snapshots to SQLite
- `signal-history`: inspect recent stored market tone snapshots
- `iv-rank`: compute front ATM IV rank from stored signal snapshots
- `skew`: single-expiry skew and market structure signal
- `smile`: single-expiry smile / wing shape signal
- `put-call-bias`: single-expiry put/call demand and positioning bias
- `market-tone`: aggregated front-expiry market structure overview
- `market-extreme`: compare current market-tone metrics against stored history
- `relative-extreme`: compare one symbol's current extremes against a benchmark
- `term-structure`: multi-expiry ATM IV term structure signal
- `portfolio`: trade journal and position view

## Setup

Set API credentials before running:

```bash
export LONGPORT_APP_KEY=...
export LONGPORT_APP_SECRET=...
export LONGPORT_ACCESS_TOKEN=...
```

Optional config file:

- default path: `~/.theta/config.json`
- override path: `THETA_CONFIG=/path/to/config.json`

## Snapshot

Main analysis and strategy entrypoint:

```bash
cargo run --bin snapshot -- --help
```

Examples:

```bash
cargo run --bin snapshot -- stock-quote --symbol TSLA.US
cargo run --bin snapshot -- analyze-chain --symbol TSLA.US --expiry 2026-03-20
cargo run --bin snapshot -- cash-secured-put --symbol TSLA.US --expiry 2026-03-20
cargo run --bin snapshot -- sell-opportunities --symbol TSLA.US --expiry 2026-03-20
```

## Capture Signals

Persist the default `TSLA.US` and `QQQ.US` market tone set into SQLite:

```bash
cargo run --bin capture-signals --
```

Optional controls:

```bash
cargo run --bin capture-signals -- \
  --symbol TSLA.US \
  --symbol QQQ.US \
  --db ~/.theta/signals.db
```

Run as a simple 5-minute sampler:

```bash
cargo run --bin capture-signals -- \
  --loop \
  --every-seconds 300
```

Limit sampling to US regular market hours:

```bash
cargo run --bin capture-signals -- \
  --loop \
  --every-seconds 300 \
  --market-hours-only
```

## systemd Service

For VPS deployment, use the bundled unit template:

- Template file: `deploy/capture-signals.service`
- It runs `capture-signals` in loop mode every 5 minutes, limited to US regular market hours.

Recommended flow on the server:

```bash
git clone <your-private-repo> ~/theta
cd ~/theta
cargo build --release
mkdir -p ~/.config/theta
cp deploy/capture-signals.service /tmp/capture-signals.service
sudo cp /tmp/capture-signals.service /etc/systemd/system/capture-signals@$(whoami).service
sudo systemctl daemon-reload
sudo systemctl enable --now capture-signals@$(whoami)
```

Set API credentials in:

```bash
~/.config/theta/capture-signals.env
```

Example:

```bash
LONGPORT_APP_KEY=...
LONGPORT_APP_SECRET=...
LONGPORT_ACCESS_TOKEN=...
```

Check logs:

```bash
sudo journalctl -u capture-signals@$(whoami) -f
```

Update after new commits:

```bash
chmod +x deploy/update.sh
./deploy/update.sh
```

Optional overrides:

- First argument: systemd service name
- `THETA_BRANCH`: branch to deploy (default: `main`)
- `THETA_PROJECT_DIR`: project path on the server (default: `~/theta`)

Example:

```bash
THETA_BRANCH=master THETA_PROJECT_DIR=$HOME/apps/theta ./deploy/update.sh capture-signals@$(whoami)
```

## Signal History

Inspect recent stored snapshots:

```bash
cargo run --bin signal-history --
```

Optional controls:

```bash
cargo run --bin signal-history -- \
  --symbol TSLA.US \
  --limit 20 \
  --json
```

## IV Rank

Compute front ATM IV rank from stored snapshots:

```bash
cargo run --bin iv-rank -- --symbol TSLA.US
```

Optional controls:

```bash
cargo run --bin iv-rank -- \
  --symbol TSLA.US \
  --limit 252 \
  --json
```

## Skew

Single-expiry skew signal:

```bash
cargo run --bin skew -- --symbol TSLA.US --expiry 2026-03-20
```

Optional controls:

```bash
cargo run --bin skew -- \
  --symbol TSLA.US \
  --expiry 2026-03-20 \
  --target-delta 0.25 \
  --target-otm-percent 0.05 \
  --json
```

## Smile

Single-expiry smile and wing shape:

```bash
cargo run --bin smile -- --symbol TSLA.US --expiry 2026-03-20
```

Optional controls:

```bash
cargo run --bin smile -- \
  --symbol TSLA.US \
  --expiry 2026-03-20 \
  --target-otm-percent 0.05 \
  --target-otm-percent 0.10 \
  --target-otm-percent 0.15 \
  --json
```

## Put/Call Bias

Single-expiry directional demand and positioning bias:

```bash
cargo run --bin put-call-bias -- --symbol TSLA.US --expiry 2026-03-20
```

Optional controls:

```bash
cargo run --bin put-call-bias -- \
  --symbol TSLA.US \
  --expiry 2026-03-20 \
  --min-otm-percent 0.05 \
  --json
```

## Market Tone

Aggregate skew, smile, term structure, and put/call bias:

```bash
cargo run --bin market-tone -- --symbol TSLA.US --expiry 2026-03-20
```

Optional controls:

```bash
cargo run --bin market-tone -- \
  --symbol TSLA.US \
  --expiry 2026-03-20 \
  --expiries-limit 4 \
  --json
```

## Market Extreme

Compare current market-tone summary metrics against stored history:

```bash
cargo run --bin market-extreme -- --symbol TSLA.US
```

## Portfolio

Trade journal, account snapshots, position reconstruction, and risk reporting:

```bash
cargo run --bin portfolio -- --help
```

Initialize the current account state before using `strategies` or `report`:

```bash
cargo run --bin portfolio -- account set \
  --cash-balance 50000 \
  --option-buying-power 100000
```

For a cash account:

```bash
cargo run --bin portfolio -- account set \
  --cash-balance 50000 \
  --cash-account
```

Inspect account snapshots:

```bash
cargo run --bin portfolio -- account show
cargo run --bin portfolio -- account history
```

Record basic trades:

```bash
cargo run --bin portfolio -- trade buy \
  --symbol TSLA \
  --underlying TSLA \
  --quantity 100 \
  --price 350 \
  --side stock
```

```bash
cargo run --bin portfolio -- trade sell \
  --symbol TSLA260320P00350000 \
  --underlying TSLA \
  --quantity 1 \
  --price 5.00 \
  --side put \
  --strike 350 \
  --expiry 2026-03-20
```

Inspect the current state:

```bash
cargo run --bin portfolio -- positions
cargo run --bin portfolio -- strategies
cargo run --bin portfolio -- report --offline
```

Use live data when available:

```bash
cargo run --bin portfolio -- report
```

Record lifecycle events explicitly:

Exercise a long option:

```bash
cargo run --bin portfolio -- trade exercise \
  --symbol TSLA260320C00400000 \
  --underlying TSLA \
  --quantity 1 \
  --side call \
  --strike 400 \
  --expiry 2026-03-20
```

Record assignment on a short option:

```bash
cargo run --bin portfolio -- trade assign \
  --symbol TSLA260320P00350000 \
  --underlying TSLA \
  --quantity 1 \
  --side put \
  --strike 350 \
  --expiry 2026-03-20
```

Record an expired option:

```bash
cargo run --bin portfolio -- trade expire \
  --symbol TSLA260320C00400000 \
  --underlying TSLA \
  --quantity 1 \
  --side call \
  --position long \
  --strike 400 \
  --expiry 2026-03-20
```

Batch-handle expired open option positions:

Dry-run first:

```bash
cargo run --bin portfolio -- trade settle-expiries \
  --date 2026-03-20 \
  --settlement-price TSLA=412.50 \
  --settlement-price QQQ=518.20
```

Apply after review:

```bash
cargo run --bin portfolio -- trade settle-expiries \
  --date 2026-03-20 \
  --settlement-price TSLA=412.50 \
  --settlement-price QQQ=518.20 \
  --apply
```

Notes:

- `strategies` and `report` require a stored account snapshot.
- Lifecycle events validate that matching open option positions exist before writing.
- `settle-expiries --apply` refuses partial settlement if prices are missing or validation fails.
- Multi-step ledger updates are wrapped in SQLite transactions.

Optional controls:

```bash
cargo run --bin market-extreme -- \
  --symbol TSLA.US \
  --limit 252 \
  --json
```

## Relative Extreme

Compare one symbol's current market-tone extremes against a benchmark:

```bash
cargo run --bin relative-extreme -- --symbol TSLA.US --benchmark QQQ.US
```

Optional controls:

```bash
cargo run --bin relative-extreme -- \
  --symbol TSLA.US \
  --benchmark QQQ.US \
  --limit 252 \
  --json
```

## Term Structure

Upcoming-expiry ATM IV term structure:

```bash
cargo run --bin term-structure -- --symbol TSLA.US
```

Optional controls:

```bash
cargo run --bin term-structure -- \
  --symbol TSLA.US \
  --expiries-limit 6 \
  --json
```

## Portfolio

Trade journal and positions:

```bash
cargo run --bin portfolio -- --help
```

Examples:

```bash
cargo run --bin portfolio -- positions
cargo run --bin portfolio -- trade list
```
