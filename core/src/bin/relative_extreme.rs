use anyhow::Result;
use clap::Parser;
use serde::Serialize;
use std::path::PathBuf;
use theta::snapshot_store::{MarketExtremeMetricStat, MarketExtremeRow, SignalSnapshotStore};

#[derive(Parser, Debug)]
#[command(name = "relative-extreme")]
#[command(about = "Compare one symbol's market-tone extremes against a benchmark symbol")]
struct Cli {
    #[arg(long, help = "Primary symbol, e.g. TSLA.US")]
    symbol: String,
    #[arg(long, default_value = "QQQ.US", help = "Benchmark symbol, e.g. QQQ.US")]
    benchmark: String,
    #[arg(long, help = "Path to the signals database (default: ~/.theta/signals.db)")]
    db: Option<PathBuf>,
    #[arg(long, default_value_t = 252, help = "Maximum number of recent samples to include")]
    limit: usize,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct RelativeMetricView {
    primary_current: f64,
    benchmark_current: f64,
    current_spread: f64,
    primary_z_score: Option<f64>,
    benchmark_z_score: Option<f64>,
    z_score_spread: Option<f64>,
}

#[derive(Debug, Serialize)]
struct RelativeExtremeView {
    primary_symbol: String,
    benchmark_symbol: String,
    sample_limit: usize,
    primary_captured_at: String,
    benchmark_captured_at: String,
    delta_skew: Option<RelativeMetricView>,
    otm_skew: Option<RelativeMetricView>,
    front_atm_iv: RelativeMetricView,
    term_structure_change_from_front: Option<RelativeMetricView>,
    open_interest_bias_ratio: Option<RelativeMetricView>,
    otm_open_interest_bias_ratio: Option<RelativeMetricView>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = match &cli.db {
        Some(path) => SignalSnapshotStore::open(path)?,
        None => SignalSnapshotStore::open_default()?,
    };

    let primary = store.compute_market_extreme(&cli.symbol, cli.limit)?;
    let benchmark = store.compute_market_extreme(&cli.benchmark, cli.limit)?;

    let Some(primary) = primary else {
        println!("No market extreme samples found for {}.", cli.symbol);
        return Ok(());
    };
    let Some(benchmark) = benchmark else {
        println!("No market extreme samples found for {}.", cli.benchmark);
        return Ok(());
    };

    let view = build_relative_extreme_view(&primary, &benchmark, cli.limit);

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&view)?);
        return Ok(());
    }

    println!("primary        : {}", view.primary_symbol);
    println!("benchmark      : {}", view.benchmark_symbol);
    println!("sample_limit   : {}", view.sample_limit);
    println!("primary_at     : {}", view.primary_captured_at);
    println!("benchmark_at   : {}", view.benchmark_captured_at);
    println!();
    print_metric("delta_skew", view.delta_skew.as_ref());
    print_metric("otm_skew", view.otm_skew.as_ref());
    print_metric("front_atm_iv", Some(&view.front_atm_iv));
    print_metric(
        "term_change",
        view.term_structure_change_from_front.as_ref(),
    );
    print_metric("oi_bias", view.open_interest_bias_ratio.as_ref());
    print_metric(
        "otm_oi_bias",
        view.otm_open_interest_bias_ratio.as_ref(),
    );

    Ok(())
}

fn build_relative_extreme_view(
    primary: &MarketExtremeRow,
    benchmark: &MarketExtremeRow,
    sample_limit: usize,
) -> RelativeExtremeView {
    RelativeExtremeView {
        primary_symbol: primary.symbol.clone(),
        benchmark_symbol: benchmark.symbol.clone(),
        sample_limit,
        primary_captured_at: primary.current_captured_at.clone(),
        benchmark_captured_at: benchmark.current_captured_at.clone(),
        delta_skew: combine_optional_metric(primary.delta_skew.as_ref(), benchmark.delta_skew.as_ref()),
        otm_skew: combine_optional_metric(primary.otm_skew.as_ref(), benchmark.otm_skew.as_ref()),
        front_atm_iv: combine_required_metric(&primary.front_atm_iv, &benchmark.front_atm_iv),
        term_structure_change_from_front: combine_optional_metric(
            primary.term_structure_change_from_front.as_ref(),
            benchmark.term_structure_change_from_front.as_ref(),
        ),
        open_interest_bias_ratio: combine_optional_metric(
            primary.open_interest_bias_ratio.as_ref(),
            benchmark.open_interest_bias_ratio.as_ref(),
        ),
        otm_open_interest_bias_ratio: combine_optional_metric(
            primary.otm_open_interest_bias_ratio.as_ref(),
            benchmark.otm_open_interest_bias_ratio.as_ref(),
        ),
    }
}

fn combine_optional_metric(
    primary: Option<&MarketExtremeMetricStat>,
    benchmark: Option<&MarketExtremeMetricStat>,
) -> Option<RelativeMetricView> {
    Some(combine_required_metric(primary?, benchmark?))
}

fn combine_required_metric(
    primary: &MarketExtremeMetricStat,
    benchmark: &MarketExtremeMetricStat,
) -> RelativeMetricView {
    let z_score_spread = match (primary.z_score, benchmark.z_score) {
        (Some(left), Some(right)) => Some(left - right),
        _ => None,
    };

    RelativeMetricView {
        primary_current: primary.current,
        benchmark_current: benchmark.current,
        current_spread: primary.current - benchmark.current,
        primary_z_score: primary.z_score,
        benchmark_z_score: benchmark.z_score,
        z_score_spread,
    }
}

fn print_metric(label: &str, metric: Option<&RelativeMetricView>) {
    let Some(metric) = metric else {
        println!("{label:<14}: n/a");
        return;
    };

    println!(
        "{label:<14}: primary {primary:>7.4} | bench {bench:>7.4} | spread {spread:>7.4} | z_spread {z_spread}",
        primary = metric.primary_current,
        bench = metric.benchmark_current,
        spread = metric.current_spread,
        z_spread = fmt_opt(metric.z_score_spread),
    );
}

fn fmt_opt(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:>7.3}"))
        .unwrap_or_else(|| "   n/a ".to_string())
}
