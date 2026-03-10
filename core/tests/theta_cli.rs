use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn theta_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_theta"))
}

#[test]
fn theta_help_lists_top_level_commands() {
    let mut cmd = theta_cmd();
    cmd.arg("--help").assert().success().stdout(
        predicate::str::contains("snapshot")
            .and(predicate::str::contains("portfolio"))
            .and(predicate::str::contains("signals"))
            .and(predicate::str::contains("structure"))
            .and(predicate::str::contains("ops"))
            .and(predicate::str::contains("completion")),
    );
}

#[test]
fn theta_version_reads_explicit_version_file() {
    let dir = tempdir().unwrap();
    let version_file = dir.path().join("VERSION");
    std::fs::write(&version_file, "v0.1.12-test\n").unwrap();

    let mut cmd = theta_cmd();
    cmd.env("THETA_VERSION_FILE", version_file.to_str().unwrap())
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::eq("v0.1.12-test\n"));
}

#[test]
fn theta_completion_bash_outputs_completion_script() {
    let mut cmd = theta_cmd();
    cmd.args(["completion", "--shell", "bash"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("_theta()")
                .and(predicate::str::contains("complete -F"))
                .and(predicate::str::contains("completion")),
        );
}

#[test]
fn theta_signals_help_lists_signal_commands() {
    let mut cmd = theta_cmd();
    cmd.args(["signals", "--help"]).assert().success().stdout(
        predicate::str::contains("capture")
            .and(predicate::str::contains("history"))
            .and(predicate::str::contains("iv-rank"))
            .and(predicate::str::contains("relative-extreme")),
    );
}

#[test]
fn theta_structure_market_tone_help_works() {
    let mut cmd = theta_cmd();
    cmd.args(["structure", "market-tone", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("--symbol")
                .and(predicate::str::contains("--expiry"))
                .and(predicate::str::contains("--smile-target-otm-percent")),
        );
}

#[test]
fn theta_ops_account_monitor_help_works() {
    let mut cmd = theta_cmd();
    cmd.args(["ops", "account-monitor", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("--account")
                .and(predicate::str::contains("--loop"))
                .and(predicate::str::contains("--once")),
        );
}

#[test]
fn theta_ops_health_check_help_works() {
    let mut cmd = theta_cmd();
    cmd.args(["ops", "health-check", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("--symbol")
                .and(predicate::str::contains("--max-otm-percent"))
                .and(predicate::str::contains("--min-contracts")),
        );
}

#[test]
fn theta_snapshot_sell_opportunities_help_lists_return_basis_flags() {
    let mut cmd = theta_cmd();
    cmd.args(["snapshot", "sell-opportunities", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("--return-basis")
                .and(predicate::str::contains("--exclude-return-basis"))
                .and(predicate::str::contains("--group-by-return-basis")),
        );
}

#[test]
fn theta_portfolio_account_rebuild_help_works() {
    let mut cmd = theta_cmd();
    cmd.args(["portfolio", "account", "rebuild", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--as-of"));
}

#[test]
fn theta_portfolio_account_monitor_history_help_works() {
    let mut cmd = theta_cmd();
    cmd.args(["portfolio", "account", "monitor-history", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--limit"));
}

#[test]
fn theta_signals_history_on_empty_db_succeeds() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("signals.db");

    let mut cmd = theta_cmd();
    cmd.args(["signals", "history", "--db", db.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("No signal snapshots found."));
}

#[test]
fn theta_signals_iv_rank_on_empty_db_succeeds() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("signals.db");

    let mut cmd = theta_cmd();
    cmd.args([
        "signals",
        "iv-rank",
        "--db",
        db.to_str().unwrap(),
        "--symbol",
        "TSLA.US",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
        "No IV rank samples found for TSLA.US.",
    ));
}

#[test]
fn theta_portfolio_account_history_on_empty_db_succeeds() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("portfolio.db");

    let mut cmd = theta_cmd();
    cmd.args([
        "portfolio",
        "--db",
        db.to_str().unwrap(),
        "account",
        "history",
    ])
    .assert()
    .success();
}

#[test]
fn theta_portfolio_positions_on_empty_db_succeeds() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("portfolio.db");

    let mut cmd = theta_cmd();
    cmd.args(["portfolio", "--db", db.to_str().unwrap(), "positions"])
        .assert()
        .success();
}

#[test]
fn theta_portfolio_account_monitor_history_on_empty_db_succeeds() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("portfolio.db");

    let mut cmd = theta_cmd();
    cmd.args([
        "portfolio",
        "--db",
        db.to_str().unwrap(),
        "account",
        "monitor-history",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains(
        "No account monitor snapshots recorded.",
    ));
}
