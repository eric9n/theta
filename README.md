# theta

Personal CLI toolkit for option market snapshots, structure signals, and portfolio tracking.

## Commands

- `theta snapshot`: market data, analytics, screening, and strategy workflows
- `theta signals capture`: persist scheduled market tone snapshots to SQLite
- `theta signals history`: inspect recent stored market tone snapshots
- `theta signals iv-rank`: compute front ATM IV rank from stored signal snapshots
- `theta structure skew`: single-expiry skew and market structure signal
- `theta structure smile`: single-expiry smile / wing shape signal
- `theta structure put-call-bias`: single-expiry put/call demand and positioning bias
- `theta structure market-tone`: aggregated front-expiry market structure overview
- `theta signals extreme`: compare current market-tone metrics against stored history
- `theta signals relative-extreme`: compare one symbol's current extremes against a benchmark
- `theta structure term-structure`: multi-expiry ATM IV term structure signal
- `theta portfolio`: trade journal and position view
- `theta ops health-check`: lightweight live self-check for daemon and option data

## Local Setup

Start the local market-data daemon before running live commands:

```bash
cargo build --release -p theta-daemon
./target/release/theta-daemon
```

`theta` CLI commands connect to the daemon over `${HOME}/.theta/run/theta.sock` by default.
Set `THETA_SOCKET_PATH` to override the socket location for both `theta-daemon` and the `theta` CLI.
LongPort credentials are required by `theta-daemon`, not by the `theta` CLI process itself.

For MCP clients, use the `theta-mcp` binary.

Optional config file:

- default path: `~/.theta/config.json`
- override path: `THETA_CONFIG=/path/to/config.json`

## Skills

Shared agent skill sources live under `skills/` in the repo and install to `/usr/local/share/theta/skills`.

## VPS Install

Install the latest GitHub release directly, without cloning the repo:

```bash
curl -fsSL https://raw.githubusercontent.com/eric9n/theta/main/deploy/install.sh | sudo bash
```

If you also want to remove the old `/root/theta` source checkout after a successful install:

```bash
curl -fsSL https://raw.githubusercontent.com/eric9n/theta/main/deploy/install.sh | sudo env REMOVE_LEGACY_ROOT=1 bash
```

This installs:

- `/usr/local/bin/theta`
- `/usr/local/bin/theta-daemon`
- `/usr/local/bin/theta-mcp`
- `/usr/local/share/theta/skills`
- `/etc/systemd/system/theta-daemon@.service`
- `/etc/systemd/system/capture-signals@.service`
- `/etc/systemd/system/account-monitor@.service`
- `/etc/systemd/system/theta-healthcheck@.service`
- `/etc/systemd/system/theta-healthcheck@.timer`
- bash/zsh/fish shell completions

Then configure credentials and enable services:

```bash
mkdir -p ~/.config/theta
sudo systemctl enable --now theta-daemon@$(whoami)
sudo systemctl enable --now capture-signals@$(whoami)
sudo systemctl enable --now account-monitor@$(whoami)
```

Run a manual live self-check any time:

```bash
/usr/local/bin/theta ops health-check
```

Update later with the same one-liner:

```bash
curl -fsSL https://raw.githubusercontent.com/eric9n/theta/main/deploy/install.sh | sudo bash
```

The old source checkout at `~/theta` or `/root/theta` is not required.

## MCP

Run the MCP adapter over stdio with:

```bash
/usr/local/bin/theta-mcp
```

Old references to `mcp-server` are no longer valid.

## Snapshot

Main analysis and strategy entrypoint:

```bash
cargo run --bin theta -- snapshot --help
```

Examples:

```bash
cargo run --bin theta -- snapshot stock-quote --symbol TSLA.US
cargo run --bin theta -- snapshot analyze-chain --symbol TSLA.US --expiry 2026-03-20
cargo run --bin theta -- snapshot cash-secured-put --symbol TSLA.US --expiry 2026-03-20
cargo run --bin theta -- snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20
cargo run --bin theta -- snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20 --return-basis premium-yield
cargo run --bin theta -- snapshot sell-opportunities --symbol TSLA.US --expiry 2026-03-20 --group-by-return-basis
```

## Capture Signals

Persist the default `TSLA.US` and `QQQ.US` market tone set into SQLite:

```bash
cargo run --bin theta -- signals capture
```

Optional controls:

```bash
cargo run --bin theta -- signals capture \
  --symbol TSLA.US \
  --symbol QQQ.US \
  --db ~/.theta/signals.db
```

Run as a simple 5-minute sampler:

```bash
cargo run --bin theta -- signals capture \
  --loop \
  --every-seconds 300
```

Limit sampling to US regular market hours:

```bash
cargo run --bin theta -- signals capture \
  --loop \
  --every-seconds 300 \
  --market-hours-only
```

## systemd Service

For VPS deployment, the installer puts these unit templates in `/etc/systemd/system`:

- `deploy/theta-daemon.service`: runs the LongPort-backed local socket daemon.
- `deploy/capture-signals.service`: runs `theta signals capture` every 5 minutes during US regular market hours.
- `deploy/account-monitor.service`: runs `theta ops account-monitor` every 5 minutes during US regular market hours.
- `deploy/theta-healthcheck.service`: runs a lightweight live health check against daemon, underlying quote, expiries, and a sampled option chain.
- `deploy/theta-healthcheck.timer`: optionally runs the live health check once per day.

`capture-signals` and `account-monitor` depend on `theta-daemon`, and each unit waits for the daemon socket before starting work.
Only `theta-daemon` needs LongPort credentials; the other units only read optional runtime overrides from `config.env`.

Set LongPort API credentials for the daemon in:

```bash
~/.config/theta/capture-signals.env
```

Optional runtime overrides such as `THETA_SOCKET_PATH` can live in:

```bash
~/.config/theta/config.env
```

Example:

```bash
LONGPORT_APP_KEY=...
LONGPORT_APP_SECRET=...
LONGPORT_ACCESS_TOKEN=...
```

Check logs:

```bash
sudo journalctl -u theta-daemon@$(whoami) -f
sudo journalctl -u capture-signals@$(whoami) -f
sudo journalctl -u account-monitor@$(whoami) -f
sudo journalctl -u theta-healthcheck@$(whoami) -f
```

Enable the optional daily self-check timer:

```bash
sudo systemctl enable --now theta-healthcheck@$(whoami).timer
```

Update to the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/eric9n/theta/main/deploy/install.sh | sudo bash
```

Optional overrides:

- `THETA_REPO`: GitHub repo in `owner/name` form
- `THETA_VERSION`: release tag or `latest` (default)
- `PREFIX`: install prefix for binaries (default: `/usr/local/bin`)
- `SHARE_DIR`: shared data dir for installed skills (default: `/usr/local/share/theta`)
- `SYSTEMD_DIR`: systemd unit dir (default: `/etc/systemd/system`)
- `REMOVE_LEGACY_ROOT=1`: remove `/root/theta` after successful install

## Release Bundle

The GitHub release archive is structured so it can be unpacked directly on a VPS without cloning the source tree:

- `bin/theta`
- `bin/theta-daemon`
- `bin/theta-mcp`
- `deploy/`
- `scripts/theta.sh`
- `skills/`
- `VERSION`

Install from an unpacked release bundle with the same script:

```bash
sudo bash deploy/install.sh
```

## Signal History

Inspect recent stored snapshots:

```bash
cargo run --bin theta -- signals history
```

Optional controls:

```bash
cargo run --bin theta -- signals history \
  --symbol TSLA.US \
  --limit 20 \
  --json
```

## IV Rank

Compute front ATM IV rank from stored snapshots:

```bash
cargo run --bin theta -- signals iv-rank --symbol TSLA.US
```

Optional controls:

```bash
cargo run --bin theta -- signals iv-rank \
  --symbol TSLA.US \
  --limit 252 \
  --json
```

## Skew

Single-expiry skew signal:

```bash
cargo run --bin theta -- structure skew --symbol TSLA.US --expiry 2026-03-20
```

Optional controls:

```bash
cargo run --bin theta -- structure skew \
  --symbol TSLA.US \
  --expiry 2026-03-20 \
  --target-delta 0.25 \
  --target-otm-percent 0.05 \
  --json
```

## Smile

Single-expiry smile and wing shape:

```bash
cargo run --bin theta -- structure smile --symbol TSLA.US --expiry 2026-03-20
```

Optional controls:

```bash
cargo run --bin theta -- structure smile \
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
cargo run --bin theta -- structure put-call-bias --symbol TSLA.US --expiry 2026-03-20
```

Optional controls:

```bash
cargo run --bin theta -- structure put-call-bias \
  --symbol TSLA.US \
  --expiry 2026-03-20 \
  --min-otm-percent 0.05 \
  --json
```

## Market Tone

Aggregate skew, smile, term structure, and put/call bias:

```bash
cargo run --bin theta -- structure market-tone --symbol TSLA.US --expiry 2026-03-20
```

Optional controls:

```bash
cargo run --bin theta -- structure market-tone \
  --symbol TSLA.US \
  --expiry 2026-03-20 \
  --expiries-limit 4 \
  --json
```

## Market Extreme

Compare current market-tone summary metrics against stored history:

```bash
cargo run --bin theta -- signals extreme --symbol TSLA.US
```

## Portfolio

Trade journal, account snapshots, position reconstruction, and risk reporting:

```bash
cargo run --bin theta -- portfolio --help
```

Initialize the current account state before using `strategies` or `report`:

```bash
cargo run --bin theta -- portfolio account set \
  --cash-balance 50000 \
  --option-buying-power 100000
```

For a cash account:

```bash
cargo run --bin theta -- portfolio account set \
  --cash-balance 50000 \
  --cash-account
```

Inspect account snapshots:

```bash
cargo run --bin theta -- portfolio account show
cargo run --bin theta -- portfolio account history
```

Record basic trades:

```bash
cargo run --bin theta -- portfolio trade buy \
  --symbol TSLA \
  --underlying TSLA \
  --quantity 100 \
  --price 350 \
  --side stock
```

```bash
cargo run --bin theta -- portfolio trade sell \
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
cargo run --bin theta -- portfolio positions
cargo run --bin theta -- portfolio strategies
cargo run --bin theta -- portfolio report --offline
```

Use live data when available:

```bash
cargo run --bin theta -- portfolio report
```

Record lifecycle events explicitly:

Exercise a long option:

```bash
cargo run --bin theta -- portfolio trade exercise \
  --symbol TSLA260320C00400000 \
  --underlying TSLA \
  --quantity 1 \
  --side call \
  --strike 400 \
  --expiry 2026-03-20
```

Record assignment on a short option:

```bash
cargo run --bin theta -- portfolio trade assign \
  --symbol TSLA260320P00350000 \
  --underlying TSLA \
  --quantity 1 \
  --side put \
  --strike 350 \
  --expiry 2026-03-20
```

Record an expired option:

```bash
cargo run --bin theta -- portfolio trade expire \
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
cargo run --bin theta -- portfolio trade settle-expiries \
  --date 2026-03-20 \
  --settlement-price TSLA=412.50 \
  --settlement-price QQQ=518.20
```

Apply after review:

```bash
cargo run --bin theta -- portfolio trade settle-expiries \
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
cargo run --bin theta -- signals extreme \
  --symbol TSLA.US \
  --limit 252 \
  --json
```

## Relative Extreme

Compare one symbol's current market-tone extremes against a benchmark:

```bash
cargo run --bin theta -- signals relative-extreme --symbol TSLA.US --benchmark QQQ.US
```

Optional controls:

```bash
cargo run --bin theta -- signals relative-extreme \
  --symbol TSLA.US \
  --benchmark QQQ.US \
  --limit 252 \
  --json
```

## Term Structure

Upcoming-expiry ATM IV term structure:

```bash
cargo run --bin theta -- structure term-structure --symbol TSLA.US
```

Optional controls:

```bash
cargo run --bin theta -- structure term-structure \
  --symbol TSLA.US \
  --expiries-limit 6 \
  --json
```

## Portfolio

Trade journal and positions:

```bash
cargo run --bin theta -- portfolio --help
```

Examples:

```bash
cargo run --bin theta -- portfolio positions
cargo run --bin theta -- portfolio trade list
```
