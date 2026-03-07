use crate::snapshot_store::SignalSnapshotStore;
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
#[command(name = "signal-history")]
#[command(about = "Inspect recently captured market tone snapshots")]
pub struct Cli {
    #[arg(
        long,
        help = "Path to the signals database (default: ~/.theta/signals.db)"
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
    let rows = store.list_market_tone_snapshots(cli.symbol.as_deref(), cli.limit)?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }

    if rows.is_empty() {
        println!("No signal snapshots found.");
        return Ok(());
    }

    println!(
        "{:<25}  {:<8}  {:<10}  {:>8}  {:>8}  {:>9}  {:<12}  {}",
        "CAPTURED_AT", "SYMBOL", "EXPIRY", "D_SKEW", "O_SKEW", "ATM_IV", "TONE", "SUMMARY"
    );
    println!("{}", "-".repeat(120));

    for row in rows {
        println!(
            "{:<25}  {:<8}  {:<10}  {:>8}  {:>8}  {:>9.4}  {:<12}  {}",
            row.captured_at,
            row.symbol,
            row.front_expiry,
            fmt_opt(row.delta_skew),
            fmt_opt(row.otm_skew),
            row.front_atm_iv,
            row.overall_tone,
            row.summary_sentence,
        );
    }

    Ok(())
}

fn fmt_opt(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:>8.4}"))
        .unwrap_or_else(|| "   n/a ".to_string())
}
