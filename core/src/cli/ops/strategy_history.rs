use crate::snapshot_store::SignalSnapshotStore;
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
#[command(name = "strategy-history")]
#[command(about = "Inspect captured strategy monitor snapshots")]
pub struct Cli {
    #[arg(
        long,
        help = "Path to the strategy snapshot database (default: ~/.theta/signals.db)"
    )]
    db: Option<PathBuf>,
    #[arg(long, help = "Filter by symbol, e.g. TSLA.US")]
    symbol: Option<String>,
    #[arg(long, default_value_t = 20, help = "Maximum number of rows to show")]
    limit: usize,
    #[arg(long)]
    json: bool,
}

pub fn run(cli: Cli) -> Result<()> {
    let store = match &cli.db {
        Some(path) => SignalSnapshotStore::open(path)?,
        None => SignalSnapshotStore::open_default()?,
    };
    let rows = store.list_strategy_monitor_snapshots(cli.symbol.as_deref(), cli.limit)?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("No strategy snapshots found.");
        return Ok(());
    }

    println!(
        "{:<25}  {:<8}  {:<10}  {:>5}  {:<10}  {:>5}  {:>8}  {:>8}  {:>8}",
        "CAPTURED_AT", "SYMBOL", "NEAR", "N_DTE", "FAR", "F_DTE", "BPUT_CR", "BCALL_CR", "PMCC_DB"
    );
    println!("{}", "-".repeat(120));

    for row in rows {
        println!(
            "{:<25}  {:<8}  {:<10}  {:>5}  {:<10}  {:>5}  {:>8}  {:>8}  {:>8}",
            row.captured_at,
            row.symbol,
            row.near_expiry,
            row.near_days_to_expiry,
            row.far_expiry,
            row.far_days_to_expiry,
            fmt_opt(row.bull_put_net_credit),
            fmt_opt(row.bear_call_net_credit),
            fmt_opt(row.pmcc_net_debit),
        );
    }

    Ok(())
}

fn fmt_opt(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:>8.2}"))
        .unwrap_or_else(|| "   n/a ".to_string())
}
