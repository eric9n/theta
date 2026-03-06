use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use theta::snapshot_store::SignalSnapshotStore;

#[derive(Parser, Debug)]
#[command(name = "iv-rank")]
#[command(about = "Compute front ATM IV rank from stored signal snapshots")]
struct Cli {
    #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
    symbol: String,
    #[arg(long, help = "Path to the signals database (default: ~/.theta/signals.db)")]
    db: Option<PathBuf>,
    #[arg(long, default_value_t = 252, help = "Maximum number of recent samples to include")]
    limit: usize,
    #[arg(long)]
    json: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = match &cli.db {
        Some(path) => SignalSnapshotStore::open(path)?,
        None => SignalSnapshotStore::open_default()?,
    };
    let row = store.compute_front_atm_iv_rank(&cli.symbol, cli.limit)?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&row)?);
        return Ok(());
    }

    let Some(row) = row else {
        println!("No IV rank samples found for {}.", cli.symbol);
        return Ok(());
    };

    println!("symbol         : {}", row.symbol);
    println!("samples        : {}", row.sample_count);
    println!("current_at     : {}", row.current_captured_at);
    println!("front_expiry   : {}", row.current_front_expiry);
    println!("current_atm_iv : {:.4}", row.current_front_atm_iv);
    println!("min_atm_iv     : {:.4}", row.min_front_atm_iv);
    println!("max_atm_iv     : {:.4}", row.max_front_atm_iv);
    match row.iv_rank {
        Some(iv_rank) => println!("iv_rank        : {:.2}%", iv_rank * 100.0),
        None => println!("iv_rank        : n/a"),
    }

    Ok(())
}
