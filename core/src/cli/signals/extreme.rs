use crate::snapshot_store::{MarketExtremeMetricStat, SignalSnapshotStore};
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
#[command(name = "market-extreme")]
#[command(about = "Measure current market-tone metrics against stored history")]
pub struct Cli {
    #[arg(
        long,
        default_value = "TSLA.US",
        help = "Underlying symbol. Default: TSLA.US"
    )]
    symbol: String,
    #[arg(
        long,
        help = "Path to the signals database (default: ~/.theta/signals.db)"
    )]
    db: Option<PathBuf>,
    #[arg(
        long,
        default_value_t = 252,
        help = "Maximum number of recent samples to include"
    )]
    limit: usize,
    #[arg(long)]
    json: bool,
}

pub fn run(cli: Cli) -> Result<()> {
    let store = match &cli.db {
        Some(path) => SignalSnapshotStore::open(path)?,
        None => SignalSnapshotStore::open_default()?,
    };
    let row = store.compute_market_extreme(&cli.symbol, cli.limit)?;

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&row)?);
        return Ok(());
    }

    let Some(row) = row else {
        println!("No market extreme samples found for {}.", cli.symbol);
        return Ok(());
    };

    println!("symbol         : {}", row.symbol);
    println!("samples        : {}", row.sample_count);
    println!("current_at     : {}", row.current_captured_at);
    println!("front_expiry   : {}", row.current_front_expiry);
    println!();
    print_metric("delta_skew", row.delta_skew.as_ref());
    print_metric("otm_skew", row.otm_skew.as_ref());
    print_metric("front_atm_iv", Some(&row.front_atm_iv));
    print_metric("term_change", row.term_structure_change_from_front.as_ref());
    print_metric("oi_bias", row.open_interest_bias_ratio.as_ref());
    print_metric("otm_oi_bias", row.otm_open_interest_bias_ratio.as_ref());
    print_metric("iv_bias", row.average_iv_bias.as_ref());
    print_metric("otm_iv_bias", row.otm_average_iv_bias.as_ref());

    Ok(())
}

fn print_metric(label: &str, stat: Option<&MarketExtremeMetricStat>) {
    let Some(stat) = stat else {
        println!("{label:<14}: n/a");
        return;
    };

    let z = stat
        .z_score
        .map(|value| format!("{value:>7.3} ({})", classify_z_score(value)))
        .unwrap_or_else(|| "   n/a (flat history)".to_string());
    println!(
        "{label:<14}: current {current:>7.4} | mean {mean:>7.4} | std {std_dev:>7.4} | z {z}",
        current = stat.current,
        mean = stat.mean,
        std_dev = stat.std_dev,
        z = z,
    );
}

fn classify_z_score(z_score: f64) -> &'static str {
    let abs = z_score.abs();
    if abs >= 3.0 {
        "extreme"
    } else if abs >= 2.0 {
        "abnormal"
    } else if abs >= 1.5 {
        "elevated"
    } else {
        "normal"
    }
}
