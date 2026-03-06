use anyhow::{Context, Result, anyhow};
use clap::Parser;
use std::path::PathBuf;
use theta::analysis_service::ThetaAnalysisService;
use theta::ledger::{AccountMonitorSnapshotInput, Ledger};
use theta::margin_engine::{self, AccountContext};
use theta::portfolio_service;
use theta::risk_domain::EnrichedPosition;
use theta::risk_engine;
use time::{
    Date, Month, OffsetDateTime, PrimitiveDateTime, UtcOffset, Weekday,
    format_description::well_known::Rfc3339,
};
use tokio::time::{Duration, sleep};

#[derive(Parser, Debug)]
#[command(name = "account-monitor")]
#[command(about = "Capture account monitoring snapshots into local portfolio SQLite storage")]
struct Cli {
    #[arg(long, default_value = "firstrade", help = "Account ID to monitor")]
    account: String,
    #[arg(long, help = "Path to the portfolio database (default: ~/.theta/portfolio.db)")]
    db: Option<PathBuf>,
    #[arg(long, help = "Keep running and capture repeatedly")]
    r#loop: bool,
    #[arg(
        long,
        default_value_t = 300,
        help = "Capture interval in seconds when --loop is enabled"
    )]
    every_seconds: u64,
    #[arg(
        long,
        default_value_t = true,
        action = clap::ArgAction::Set,
        help = "Only capture during US regular market hours (09:30-16:00 ET, Mon-Fri)"
    )]
    market_hours_only: bool,
    #[arg(long, conflicts_with = "loop", help = "Capture one sample and exit")]
    once: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let account_id = cli.account.trim().to_lowercase();
    if account_id.is_empty() {
        return Err(anyhow!("--account must not be empty"));
    }

    let ledger = match &cli.db {
        Some(path) => Ledger::open(path)?,
        None => Ledger::open_default()?,
    };

    let run_loop = !cli.once && cli.r#loop;

    loop {
        let now_utc = OffsetDateTime::now_utc();
        let captured_at = now_utc.format(&Rfc3339)?;

        if cli.market_hours_only && !is_us_regular_market_hours(now_utc) {
            println!("{captured_at} outside US regular market hours; skipping account capture");
        } else if let Err(err) = capture_once(&ledger, &account_id, &captured_at).await {
            eprintln!("{captured_at} capture failed: {err:#}");
            if let Err(record_err) = record_error_snapshot(&ledger, &account_id, &captured_at, &err) {
                eprintln!("{captured_at} failed to persist error snapshot: {record_err:#}");
            }
        }

        if !run_loop {
            break;
        }

        println!("sleeping {}s until next account capture", cli.every_seconds);
        if !sleep_or_terminate(cli.every_seconds).await {
            println!("shutdown signal received; exiting account-monitor");
            break;
        }
    }

    Ok(())
}

async fn capture_once(ledger: &Ledger, account_id: &str, captured_at: &str) -> Result<()> {
    let positions = ledger
        .calculate_positions(account_id, None)
        .with_context(|| format!("failed to calculate positions for account {account_id}"))?;

    let account_snapshot = ledger
        .latest_account_snapshot(account_id)
        .with_context(|| format!("failed to load latest account snapshot for account {account_id}"))?
        .ok_or_else(|| anyhow!("no account snapshot found for account {account_id}"))?;

    let service = ThetaAnalysisService::from_env().await?;
    let enriched = portfolio_service::enrich_positions(&service, &positions).await?;
    let strategies = risk_engine::identify_strategies(&positions);
    let account_ctx = AccountContext {
        trade_date_cash: Some(account_snapshot.trade_date_cash),
        settled_cash: Some(account_snapshot.settled_cash),
        option_buying_power: account_snapshot.option_buying_power,
        stock_buying_power: account_snapshot.stock_buying_power,
        margin_loan: account_snapshot.margin_loan,
        short_market_value: account_snapshot.short_market_value,
        margin_enabled: account_snapshot.margin_enabled,
    };
    let evaluated_strategies = margin_engine::evaluate_strategies(&strategies, &enriched, &account_ctx);
    let portfolio_greeks = risk_engine::aggregate_greeks(&enriched);

    let positions_count = enriched.len() as i64;
    let position_market_value = position_market_value(&enriched);
    let unrealized_pnl: f64 = enriched.iter().map(|p| p.unrealized_pnl).sum();
    let total_margin_required: f64 = evaluated_strategies
        .iter()
        .map(|s| s.margin.margin_required)
        .sum();
    let equity = equity_estimate(
        account_snapshot.settled_cash,
        position_market_value,
        account_snapshot.margin_loan,
    );

    ledger.record_account_monitor_snapshot(&AccountMonitorSnapshotInput {
        captured_at: captured_at.to_string(),
        account_id: account_id.to_string(),
        status: "ok".to_string(),
        error_message: None,
        trade_date_cash: Some(account_snapshot.trade_date_cash),
        settled_cash: Some(account_snapshot.settled_cash),
        margin_loan: account_snapshot.margin_loan,
        option_buying_power: account_snapshot.option_buying_power,
        positions_count: Some(positions_count),
        position_market_value: Some(position_market_value),
        unrealized_pnl: Some(unrealized_pnl),
        total_margin_required: Some(total_margin_required),
        net_delta_shares: Some(portfolio_greeks.net_delta_shares),
        total_gamma: Some(portfolio_greeks.total_gamma),
        total_theta_per_day: Some(portfolio_greeks.total_theta_per_day),
        total_vega: Some(portfolio_greeks.total_vega),
        equity_estimate: Some(equity),
        notes: "account-monitor sample".to_string(),
    })?;

    println!(
        "{captured_at} account={account_id} positions={} equity={:.2} pnl={:.2} margin={:.2}",
        positions_count, equity, unrealized_pnl, total_margin_required
    );
    Ok(())
}

fn record_error_snapshot(
    ledger: &Ledger,
    account_id: &str,
    captured_at: &str,
    err: &anyhow::Error,
) -> Result<()> {
    ledger.record_account_monitor_snapshot(&AccountMonitorSnapshotInput {
        captured_at: captured_at.to_string(),
        account_id: account_id.to_string(),
        status: "error".to_string(),
        error_message: Some(format!("{err:#}")),
        notes: "account-monitor sample".to_string(),
        ..Default::default()
    })?;
    Ok(())
}

fn position_market_value(enriched: &[EnrichedPosition]) -> f64 {
    enriched
        .iter()
        .map(|p| {
            let multiplier = if p.side == "stock" { 1.0 } else { 100.0 };
            p.current_price * p.net_quantity as f64 * multiplier
        })
        .sum()
}

fn equity_estimate(settled_cash: f64, position_market_value: f64, margin_loan: Option<f64>) -> f64 {
    settled_cash + position_market_value - margin_loan.unwrap_or(0.0)
}

async fn sleep_or_terminate(seconds: u64) -> bool {
    tokio::select! {
        _ = sleep(Duration::from_secs(seconds)) => true,
        _ = shutdown_signal() => false,
    }
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut sigterm) = signal(SignalKind::terminate()) {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = sigterm.recv() => {},
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

fn is_us_regular_market_hours(now_utc: OffsetDateTime) -> bool {
    let eastern_offset = if is_us_daylight_saving_time(now_utc) {
        UtcOffset::from_hms(-4, 0, 0).expect("valid EDT offset")
    } else {
        UtcOffset::from_hms(-5, 0, 0).expect("valid EST offset")
    };
    let eastern_now = now_utc.to_offset(eastern_offset);

    match eastern_now.weekday() {
        Weekday::Saturday | Weekday::Sunday => return false,
        _ => {}
    }

    let minutes = u16::from(eastern_now.hour()) * 60 + u16::from(eastern_now.minute());
    let open_minutes: u16 = 9 * 60 + 30;
    let close_minutes: u16 = 16 * 60;
    minutes >= open_minutes && minutes < close_minutes
}

fn is_us_daylight_saving_time(now_utc: OffsetDateTime) -> bool {
    let year = now_utc.year();
    let dst_start_day = nth_weekday_of_month(year, Month::March, Weekday::Sunday, 2);
    let dst_end_day = nth_weekday_of_month(year, Month::November, Weekday::Sunday, 1);

    let dst_start_utc = PrimitiveDateTime::new(
        Date::from_calendar_date(year, Month::March, dst_start_day).expect("valid DST start date"),
        time::macros::time!(7:00),
    )
    .assume_utc();
    let dst_end_utc = PrimitiveDateTime::new(
        Date::from_calendar_date(year, Month::November, dst_end_day).expect("valid DST end date"),
        time::macros::time!(6:00),
    )
    .assume_utc();

    now_utc >= dst_start_utc && now_utc < dst_end_utc
}

fn nth_weekday_of_month(year: i32, month: Month, weekday: Weekday, nth: u8) -> u8 {
    let mut count = 0;
    for day in 1..=31 {
        let Ok(date) = Date::from_calendar_date(year, month, day) else {
            break;
        };
        if date.weekday() == weekday {
            count += 1;
            if count == nth {
                return day;
            }
        }
    }
    panic!("failed to resolve weekday occurrence for {month:?} {year}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_position(side: &str, current_price: f64, net_quantity: i64) -> EnrichedPosition {
        EnrichedPosition {
            symbol: "T".to_string(),
            underlying: "TSLA".to_string(),
            underlying_spot: Some(400.0),
            side: side.to_string(),
            strike: None,
            expiry: None,
            net_quantity,
            avg_cost: current_price,
            current_price,
            unrealized_pnl_per_unit: 0.0,
            unrealized_pnl: 0.0,
            greeks: None,
        }
    }

    #[test]
    fn position_market_value_handles_stock_and_options() {
        let positions = vec![
            make_position("stock", 100.0, 20), // +2000
            make_position("call", 2.0, -3),   // -600
            make_position("put", 1.5, 1),     // +150
        ];
        let mv = position_market_value(&positions);
        assert!((mv - 1550.0).abs() < 0.0001);
    }

    #[test]
    fn equity_estimate_subtracts_margin_loan() {
        let equity = equity_estimate(10_000.0, 2_500.0, Some(1_200.0));
        assert!((equity - 11_300.0).abs() < 0.0001);

        let equity_without_loan = equity_estimate(10_000.0, -500.0, None);
        assert!((equity_without_loan - 9_500.0).abs() < 0.0001);
    }
}
