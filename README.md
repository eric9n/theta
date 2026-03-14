# theta

Personal CLI toolkit for TSLA option monitoring, chain analysis, and portfolio risk.

## Daily Workflow

All market-analysis commands default to `TSLA.US`, so the normal flow is:

1. Check current positions and portfolio Greeks.
2. Check whether the current regime is interesting.
3. If needed, inspect the raw structure behind that signal.
4. Pick an expiry and inspect the chain.
5. Screen the strategy you already want to trade.

Start by reviewing the risk already on the book:

```bash
theta portfolio positions
theta portfolio report --offline
```

Start with the fast signal view:

```bash
theta signals monitor
theta signals iv-rank
theta signals extreme
```

If the signal is interesting and you want detail, drill into raw structure:

```bash
theta structure skew --expiry 2026-04-10
theta structure term-structure
theta structure put-call-bias --expiry 2026-04-10
```

Then inspect the chain you may trade:

```bash
theta snapshot option-expiries
theta snapshot analyze-chain --expiry 2026-04-10
```

Use the strategy screeners only after you already know which structure you want:

- `theta snapshot bull-put-spread --expiry ...`
- `theta snapshot bull-call-spread --expiry ...`
- `theta snapshot calendar-call-spread --near-expiry ... --far-expiry ...`
- `theta snapshot diagonal-call-spread --near-expiry ... --far-expiry ...`

Signal history is usually collected in the background by `taskd`, but you can still run it manually:

```bash
theta signals capture
```

## Command Map

- `theta signals`
  TSLA skew / IV history and extreme monitoring.
- `theta structure`
  Raw structure diagnostics when you want more detail than `signals`.
- `theta snapshot`
  Live chain inspection, single-leg analysis, and four strategy screeners.
- `theta portfolio`
  Trade journal, positions, strategies, and portfolio Greeks.
- `theta ops`
  Health checks and recurring operational tasks.

## Local Setup

Start the local market-data daemon before running live commands:

```bash
cargo build --release -p theta-daemon
./target/release/theta-daemon
```

`theta` CLI commands connect to the daemon over `${HOME}/.theta/run/theta.sock` by default.
Set `THETA_SOCKET_PATH` to override the socket location for both `theta-daemon` and the `theta` CLI.
LongPort credentials are required by `theta-daemon`, not by the `theta` CLI process itself.

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
- `/usr/local/share/theta/taskd`
- `/usr/local/share/theta/skills`
- `/etc/systemd/system/theta-daemon@.service`

Then configure credentials, enable the daemon, and install the recurring taskd jobs:

```bash
mkdir -p ~/.config/theta
sudo systemctl enable --now theta-daemon@$(whoami)
sudo /usr/local/share/theta/taskd/install-taskd.sh \
  --account firstrade
```

Run a manual live self-check any time:

```bash
/usr/local/bin/theta ops health-check
```

The old source checkout at `~/theta` or `/root/theta` is not required.
Agent integrations should use the shared files under `skills/` instead of an MCP adapter.

## Snapshot

```bash
cargo run --bin theta -- snapshot --help
```

Retained strategy screeners:

- `bull-put-spread`
- `bull-call-spread`
- `calendar-call-spread`
- `diagonal-call-spread`

Useful examples:

```bash
cargo run --bin theta -- snapshot stock-quote
cargo run --bin theta -- snapshot option-expiries
cargo run --bin theta -- snapshot analyze-chain --expiry 2026-03-20
cargo run --bin theta -- snapshot analyze-option --symbol TSLA260320C00400000.US
cargo run --bin theta -- snapshot bull-put-spread --expiry 2026-03-20
```

## Structure

```bash
cargo run --bin theta -- structure --help
```

Use `structure` as the detailed view behind `signals`:

- `skew`
  Inspect whether put wing or call wing is richer for a specific expiry.
- `term-structure`
  Inspect ATM IV across expiries.
- `put-call-bias`
  Compare put/call IV, volume, and open interest for a specific expiry.
- `market-tone`
  Summarize skew, bias, and term structure in one view.
- `smile`
  Inspect the IV curve across strikes.

Useful examples:

```bash
cargo run --bin theta -- structure skew --expiry 2026-03-20
cargo run --bin theta -- structure term-structure
cargo run --bin theta -- structure put-call-bias --expiry 2026-03-20
cargo run --bin theta -- structure market-tone --expiry 2026-03-20
```

## Capture Signals

Persist the default `TSLA.US` skew and market-tone snapshot into SQLite:

```bash
cargo run --bin theta -- signals capture
```

Optional controls:

```bash
cargo run --bin theta -- signals capture \
  --symbol TSLA.US \
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

## Monitor Richness

Check whether puts or calls are rich, with `otm_skew` as the main signal and OI only as confirmation:

```bash
cargo run --bin theta -- signals monitor
```

JSON output:

```bash
cargo run --bin theta -- signals monitor --json
```

## systemd Daemon

For VPS deployment, the installer puts `deploy/theta-daemon.service` into `/etc/systemd/system/theta-daemon@.service`.

`theta-daemon` is the only theta component that remains under systemd. Recurring signal capture, account monitoring, and health-check runs are expected to be scheduled by `taskd`.

## taskd Scheduler

For recurring theta jobs, keep `theta-daemon` under systemd and let `taskd` trigger one-shot CLI runs on a cron schedule.

- Sample config: `deploy/taskd/tasks.yaml.example`
- Installed taskd assets: `/usr/local/share/theta/taskd/`
- Keep `theta-daemon@.service` enabled; do not move the daemon itself into `taskd`

Example migration flow:

```bash
sudo /usr/local/share/theta/taskd/install-taskd.sh \
  --account firstrade
```

If you prefer hand-editing YAML, use `deploy/taskd/tasks.yaml.example` as the starting point and merge the three `theta-*` tasks into the existing `/etc/taskd/tasks.yaml` instead of replacing unrelated tasks.

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
/opt/taskd/taskctl list
/opt/taskd/taskctl history theta-capture-signals --limit 20
/opt/taskd/taskctl history theta-account-monitor --limit 20
/opt/taskd/taskctl history theta-healthcheck --limit 20
```

## Release Bundle

The GitHub release archive is structured so it can be unpacked directly on a VPS without cloning the source tree:

- `bin/theta`
- `bin/theta-daemon`
- `deploy/`
- `skills/`
- `VERSION`

Install from an unpacked release bundle with the same script:

```bash
sudo bash deploy/install.sh
```

## Signals Reference

Common inspection commands:

```bash
cargo run --bin theta -- signals history
cargo run --bin theta -- signals iv-rank
cargo run --bin theta -- signals extreme
```

Useful options:

```bash
cargo run --bin theta -- signals history --limit 20 --json
cargo run --bin theta -- signals iv-rank --limit 252 --json
cargo run --bin theta -- signals extreme --limit 252 --json
```

## Structure Reference

Use `structure` only when you want raw diagnostics rather than the simplified `signals` workflow.

```bash
cargo run --bin theta -- structure skew --expiry 2026-03-20
cargo run --bin theta -- structure smile --expiry 2026-03-20
cargo run --bin theta -- structure put-call-bias --expiry 2026-03-20
cargo run --bin theta -- structure market-tone --expiry 2026-03-20
cargo run --bin theta -- structure term-structure
```

JSON examples:

```bash
cargo run --bin theta -- structure skew --expiry 2026-03-20 --json
cargo run --bin theta -- structure market-tone --expiry 2026-03-20 --expiries-limit 4 --json
cargo run --bin theta -- structure term-structure --expiries-limit 6 --json
```

## Portfolio Reference

Use `portfolio` for four things:

- record account state
- record trades
- inspect current positions
- inspect strategy grouping and portfolio Greeks

```bash
cargo run --bin theta -- portfolio account --help
cargo run --bin theta -- portfolio trade --help
```

Minimal workflow:

```bash
cargo run --bin theta -- portfolio account set \
  --cash-balance 50000 \
  --option-buying-power 100000
cargo run --bin theta -- portfolio trade buy \
  --symbol TSLA \
  --underlying TSLA \
  --quantity 100 \
  --price 350 \
  --side stock

cargo run --bin theta -- portfolio trade sell \
  --symbol TSLA260320P00350000 \
  --underlying TSLA \
  --quantity 1 \
  --price 5.00 \
  --side put \
  --strike 350 \
  --expiry 2026-03-20
cargo run --bin theta -- portfolio positions
cargo run --bin theta -- portfolio strategies
cargo run --bin theta -- portfolio report --offline
```

Main portfolio commands:

- `portfolio account set|show|history`
  Maintain buying power / cash snapshots.
- `portfolio trade buy|sell|list`
  Maintain the trade ledger.
- `portfolio positions`
  Reconstruct current open positions from the ledger.
- `portfolio strategies`
  Group positions into recognizable option structures.
- `portfolio report`
  Show portfolio-level Greeks and risk summary.

Advanced lifecycle actions still exist:

- `portfolio trade exercise`
- `portfolio trade assign`
- `portfolio trade expire`
- `portfolio trade settle-expiries`

Use those only when you need explicit lifecycle bookkeeping.
