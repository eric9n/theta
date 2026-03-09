use crate::daemon_protocol::is_transient_quote_rate_limit_error as is_transient_quote_limit_error;
use crate::signal_service::{MarketToneRequest, ThetaSignalService};
use crate::snapshot_store::SignalSnapshotStore;
use anyhow::{Error, Result};
use clap::Args;
use std::path::PathBuf;
use time::{
    Date, Month, OffsetDateTime, PrimitiveDateTime, UtcOffset, Weekday,
    format_description::well_known::Rfc3339,
};
use tokio::time::{Duration, sleep};

#[derive(Args, Debug)]
#[command(name = "capture-signals")]
#[command(about = "Capture market tone snapshots into local SQLite storage")]
pub struct Cli {
    #[arg(
        long = "symbol",
        help = "Underlying symbol(s); repeatable. Defaults to TSLA.US and QQQ.US"
    )]
    symbols: Vec<String>,
    #[arg(
        long,
        help = "Path to the signals database (default: ~/.theta/signals.db)"
    )]
    db: Option<PathBuf>,
    #[arg(
        long,
        default_value_t = 4,
        help = "Number of upcoming expiries to include in term structure"
    )]
    expiries_limit: usize,
    #[arg(
        long,
        help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
    )]
    rate: Option<f64>,
    #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
    dividend: f64,
    #[arg(
        long,
        help = "Manual annualized implied volatility override for all contracts"
    )]
    iv: Option<f64>,
    #[arg(long, help = "Solve IV from each option's provider last_done price")]
    iv_from_market_price: bool,
    #[arg(
        long,
        default_value_t = 0.25,
        help = "Target absolute delta for skew matching"
    )]
    target_delta: f64,
    #[arg(
        long,
        default_value_t = 0.05,
        help = "Target OTM percent for skew matching"
    )]
    target_otm_percent: f64,
    #[arg(
        long = "smile-target-otm-percent",
        help = "Target OTM percent(s) for smile sampling; repeatable"
    )]
    smile_target_otm_percents: Vec<f64>,
    #[arg(
        long,
        default_value_t = 0.05,
        help = "Minimum OTM percent for put/call bias OTM buckets"
    )]
    bias_min_otm_percent: f64,
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
        help = "Only capture during US regular market hours (09:30-16:00 ET, Mon-Fri)"
    )]
    market_hours_only: bool,
}

pub async fn run(cli: Cli) -> Result<()> {
    let symbols = if cli.symbols.is_empty() {
        vec!["TSLA.US".to_string(), "QQQ.US".to_string()]
    } else {
        cli.symbols
    };
    let smile_targets = if cli.smile_target_otm_percents.is_empty() {
        vec![0.05, 0.10, 0.15]
    } else {
        cli.smile_target_otm_percents
    };

    let store = match &cli.db {
        Some(path) => SignalSnapshotStore::open(path)?,
        None => SignalSnapshotStore::open_default()?,
    };
    let service = ThetaSignalService::from_env().await?;
    let loop_sleep_seconds = symbol_loop_sleep_seconds(symbols.len(), cli.every_seconds);

    loop {
        for (index, symbol) in symbols.iter().enumerate() {
            let now_utc = OffsetDateTime::now_utc();
            let captured_at = now_utc.format(&Rfc3339)?;

            if cli.market_hours_only && !is_us_regular_market_hours(now_utc) {
                println!("{captured_at} outside US regular market hours; skipping capture");
                if !cli.r#loop {
                    return Ok(());
                }
                if should_sleep_after_symbol(index, symbols.len(), cli.r#loop) {
                    println!("sleeping {}s until next capture", loop_sleep_seconds);
                    sleep(Duration::from_secs(loop_sleep_seconds)).await;
                }
                continue;
            }

            let capture = async {
                let front_expiry = service.front_expiry_for_symbol(symbol).await?;
                let view = service
                    .market_tone(MarketToneRequest {
                        symbol: symbol.clone(),
                        expiry: front_expiry,
                        expiries_limit: cli.expiries_limit,
                        rate: cli.rate,
                        dividend: cli.dividend,
                        iv: cli.iv,
                        iv_from_market_price: cli.iv_from_market_price,
                        target_delta: cli.target_delta,
                        target_otm_percent: cli.target_otm_percent,
                        smile_target_otm_percents: smile_targets.clone(),
                        bias_min_otm_percent: cli.bias_min_otm_percent,
                    })
                    .await?;
                store.record_market_tone(&captured_at, &view)?;
                println!(
                    "{} {} {} {}",
                    captured_at,
                    view.underlying_symbol,
                    view.front_expiry,
                    view.summary.overall_tone
                );
                Ok::<(), Error>(())
            }
            .await;

            if let Err(err) = capture {
                if cli.r#loop && is_transient_quote_limit_error(&err) {
                    eprintln!(
                        "{captured_at} transient quote rate limit while capturing {symbol}: {err}"
                    );
                } else {
                    return Err(err);
                }
            }

            if should_sleep_after_symbol(index, symbols.len(), cli.r#loop) {
                println!("sleeping {}s until next capture", loop_sleep_seconds);
                sleep(Duration::from_secs(loop_sleep_seconds)).await;
            }
        }

        if !cli.r#loop {
            break;
        }
    }

    Ok(())
}

fn symbol_loop_sleep_seconds(symbol_count: usize, every_seconds: u64) -> u64 {
    if symbol_count <= 1 {
        return every_seconds;
    }

    let symbol_count = symbol_count as u64;
    let spacing = every_seconds / symbol_count;
    spacing.max(1)
}

fn should_sleep_after_symbol(index: usize, symbol_count: usize, loop_enabled: bool) -> bool {
    if !loop_enabled {
        return false;
    }

    index + 1 <= symbol_count
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
    use super::{
        is_transient_quote_limit_error, should_sleep_after_symbol, symbol_loop_sleep_seconds,
    };
    use anyhow::anyhow;

    #[test]
    fn detects_transient_quote_rate_limit_errors() {
        assert!(is_transient_quote_limit_error(&anyhow!(
            "SDK Proxy Error [option_quote]: response error: 7: detail:Some(WsResponseErrorDetail {{ code: 301607, msg: \"Too many option securities request within one minute\" }})"
        )));
        assert!(is_transient_quote_limit_error(&anyhow!(
            "SDK Proxy Error [option_quote]: response error: 301606 Request rate limit"
        )));
        assert!(is_transient_quote_limit_error(&anyhow!(
            "theta daemon Provider error [option_quote]: local option_quote cooldown active for 42s after upstream rate limit"
        )));
    }

    #[test]
    fn ignores_non_rate_limit_errors() {
        assert!(!is_transient_quote_limit_error(&anyhow!(
            "target_price is outside solvable range for current model assumptions"
        )));
    }

    #[test]
    fn spreads_loop_sleep_evenly_across_symbols() {
        assert_eq!(symbol_loop_sleep_seconds(1, 300), 300);
        assert_eq!(symbol_loop_sleep_seconds(2, 300), 150);
        assert_eq!(symbol_loop_sleep_seconds(3, 300), 100);
        assert_eq!(symbol_loop_sleep_seconds(4, 3), 1);
    }

    #[test]
    fn only_sleeps_between_symbols_when_looping() {
        assert!(should_sleep_after_symbol(0, 2, true));
        assert!(should_sleep_after_symbol(1, 2, true));
        assert!(!should_sleep_after_symbol(0, 2, false));
    }
}
