use crate::signal_service::{ExpirySelection, ThetaSignalService};
use crate::strategy_service::{StrategyMonitorRequest, StrategyMonitorView, ThetaStrategyService};
use anyhow::{Result, bail};
use clap::Args;
use serde::Serialize;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Args, Debug)]
pub struct StrategyMonitorArgs {
    #[arg(long, default_value = "TSLA.US", help = "Underlying symbol to monitor")]
    pub symbol: String,
    #[arg(long, default_value_t = 14, help = "Minimum near-expiry DTE")]
    pub near_min_dte: i64,
    #[arg(long, default_value_t = 45, help = "Maximum near-expiry DTE")]
    pub near_max_dte: i64,
    #[arg(long, default_value_t = 30, help = "Target near-expiry DTE")]
    pub near_target_dte: i64,
    #[arg(long, default_value_t = 60, help = "Minimum far-expiry DTE")]
    pub far_min_dte: i64,
    #[arg(long, default_value_t = 180, help = "Maximum far-expiry DTE")]
    pub far_max_dte: i64,
    #[arg(long, default_value_t = 90, help = "Target far-expiry DTE")]
    pub far_target_dte: i64,
    #[arg(
        long,
        help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
    )]
    pub rate: Option<f64>,
    #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
    pub dividend: f64,
    #[arg(
        long,
        help = "Manual annualized implied volatility override for all contracts"
    )]
    pub iv: Option<f64>,
    #[arg(long, help = "Solve IV from each option's provider last_done price")]
    pub iv_from_market_price: bool,
    #[arg(long, default_value_t = 100, help = "Minimum open interest")]
    pub min_open_interest: i64,
    #[arg(long, default_value_t = 10, help = "Minimum volume")]
    pub min_volume: i64,
    #[arg(
        long,
        default_value_t = 5,
        help = "Maximum candidates retained per strategy"
    )]
    pub limit_per_strategy: usize,
    #[arg(
        long,
        default_value_t = -0.35,
        help = "Bull put short-leg minimum delta"
    )]
    pub bull_put_min_short_delta: f64,
    #[arg(
        long,
        default_value_t = -0.15,
        help = "Bull put short-leg maximum delta"
    )]
    pub bull_put_max_short_delta: f64,
    #[arg(
        long,
        default_value_t = 0.03,
        help = "Bull put short-leg minimum OTM percent"
    )]
    pub bull_put_min_short_otm_percent: f64,
    #[arg(
        long,
        default_value_t = 0.15,
        help = "Bull put short-leg maximum OTM percent"
    )]
    pub bull_put_max_short_otm_percent: f64,
    #[arg(
        long,
        default_value_t = 0.15,
        help = "Bear call short-leg minimum delta"
    )]
    pub bear_call_min_short_delta: f64,
    #[arg(
        long,
        default_value_t = 0.30,
        help = "Bear call short-leg maximum delta"
    )]
    pub bear_call_max_short_delta: f64,
    #[arg(
        long,
        default_value_t = 0.03,
        help = "Bear call short-leg minimum OTM percent"
    )]
    pub bear_call_min_short_otm_percent: f64,
    #[arg(
        long,
        default_value_t = 0.15,
        help = "Bear call short-leg maximum OTM percent"
    )]
    pub bear_call_max_short_otm_percent: f64,
    #[arg(
        long,
        default_value_t = 500.0,
        help = "Minimum spread width in dollars"
    )]
    pub min_width_per_spread: f64,
    #[arg(
        long,
        default_value_t = 2000.0,
        help = "Maximum spread width in dollars"
    )]
    pub max_width_per_spread: f64,
    #[arg(
        long,
        default_value_t = 1500.0,
        help = "Maximum bull call net debit per spread in dollars"
    )]
    pub max_bull_call_net_debit_per_spread: f64,
    #[arg(
        long,
        default_value_t = 3000.0,
        help = "Maximum calendar call net debit per spread in dollars"
    )]
    pub max_calendar_net_debit_per_spread: f64,
    #[arg(
        long,
        default_value_t = 5000.0,
        help = "Maximum PMCC net debit per spread in dollars"
    )]
    pub max_diagonal_net_debit_per_spread: f64,
    #[arg(
        long,
        default_value_t = 0.0,
        help = "Minimum calendar theta carry per day"
    )]
    pub min_calendar_theta_carry_per_day: f64,
    #[arg(long, default_value_t = 0.0, help = "Minimum PMCC theta carry per day")]
    pub min_diagonal_theta_carry_per_day: f64,
    #[arg(
        long,
        default_value_t = 21,
        help = "Minimum days gap between near and far expiries"
    )]
    pub min_days_gap: i64,
    #[arg(
        long,
        default_value_t = 180,
        help = "Maximum days gap between near and far expiries"
    )]
    pub max_days_gap: i64,
    #[arg(
        long,
        default_value_t = 0.0,
        help = "Minimum strike gap for cross-expiry strategies"
    )]
    pub min_strike_gap: f64,
    #[arg(
        long,
        default_value_t = 50.0,
        help = "Maximum strike gap for cross-expiry strategies"
    )]
    pub max_strike_gap: f64,
}

#[derive(Args, Debug)]
#[command(name = "strategy-monitor")]
#[command(about = "Run a TSLA-style strategy monitor with lighter, strategy-specific option scans")]
pub struct Cli {
    #[command(flatten)]
    pub args: StrategyMonitorArgs,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Serialize)]
pub(crate) struct CreditSpreadSummary {
    pub candidate_count: usize,
    pub short_strike: f64,
    pub long_strike: f64,
    pub net_credit_per_spread: f64,
    pub max_loss_per_spread: f64,
    pub breakeven: f64,
    pub annualized_return_on_risk: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct DebitSpreadSummary {
    pub candidate_count: usize,
    pub long_strike: f64,
    pub short_strike: f64,
    pub net_debit_per_spread: f64,
    pub max_profit_per_spread: f64,
    pub max_loss_per_spread: f64,
    pub breakeven: f64,
    pub annualized_return_on_risk: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct CalendarSummary {
    pub candidate_count: usize,
    pub strike_price: f64,
    pub net_debit_per_spread: f64,
    pub net_theta_carry_per_day: f64,
    pub annualized_theta_carry_return_on_debit: f64,
    pub max_loss_per_spread: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct DiagonalSummary {
    pub candidate_count: usize,
    pub near_strike: f64,
    pub far_strike: f64,
    pub net_debit_per_spread: f64,
    pub net_theta_carry_per_day: f64,
    pub annualized_theta_carry_return_on_debit: f64,
    pub max_loss_per_spread: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct IronCondorSummary {
    pub short_put_strike: f64,
    pub long_put_strike: f64,
    pub short_call_strike: f64,
    pub long_call_strike: f64,
    pub net_credit_per_spread: f64,
    pub max_loss_per_spread: f64,
    pub breakeven_low: f64,
    pub breakeven_high: f64,
    pub annualized_return_on_risk: f64,
}

#[derive(Debug, Serialize)]
pub(crate) struct StrategyMonitorReport {
    pub generated_at: String,
    pub symbol: String,
    pub near_expiry: String,
    pub near_days_to_expiry: i64,
    pub far_expiry: String,
    pub far_days_to_expiry: i64,
    pub bull_put_spread: Option<CreditSpreadSummary>,
    pub bear_call_spread: Option<CreditSpreadSummary>,
    pub iron_condor: Option<IronCondorSummary>,
    pub bull_call_spread: Option<DebitSpreadSummary>,
    pub calendar_call_spread: Option<CalendarSummary>,
    pub pmcc: Option<DiagonalSummary>,
}

pub async fn run(cli: Cli) -> Result<()> {
    let report = build_report_from_args(&cli.args).await?;
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    println!("generated_at      : {}", report.generated_at);
    println!("symbol            : {}", report.symbol);
    println!(
        "near_expiry       : {} ({} DTE)",
        report.near_expiry, report.near_days_to_expiry
    );
    println!(
        "far_expiry        : {} ({} DTE)",
        report.far_expiry, report.far_days_to_expiry
    );
    println!();
    print_credit_summary("bull_put_spread", report.bull_put_spread.as_ref());
    print_credit_summary("bear_call_spread", report.bear_call_spread.as_ref());
    print_iron_condor("iron_condor", report.iron_condor.as_ref());
    print_debit_summary("bull_call_spread", report.bull_call_spread.as_ref());
    print_calendar_summary("calendar_call", report.calendar_call_spread.as_ref());
    print_diagonal_summary("pmcc", report.pmcc.as_ref());

    Ok(())
}

pub(crate) async fn build_report_from_args(
    args: &StrategyMonitorArgs,
) -> Result<StrategyMonitorReport> {
    validate_args(args)?;

    let symbol = args.symbol.trim().to_uppercase();
    let signal_service = ThetaSignalService::from_env().await?;
    let near_expiry = signal_service
        .target_expiry_for_symbol(
            &symbol,
            ExpirySelection {
                min_days_to_expiry: args.near_min_dte,
                max_days_to_expiry: args.near_max_dte,
                target_days_to_expiry: args.near_target_dte,
            },
        )
        .await?;
    let far_expiry = signal_service
        .target_expiry_for_symbol(
            &symbol,
            ExpirySelection {
                min_days_to_expiry: args.far_min_dte,
                max_days_to_expiry: args.far_max_dte,
                target_days_to_expiry: args.far_target_dte,
            },
        )
        .await?;

    if far_expiry <= near_expiry {
        bail!("far expiry must be later than near expiry");
    }

    let service = ThetaStrategyService::from_env().await?;
    let view = service
        .screen_strategy_monitor(StrategyMonitorRequest {
            symbol: symbol.clone(),
            near_expiry,
            far_expiry,
            rate: args.rate,
            dividend: args.dividend,
            iv: args.iv,
            iv_from_market_price: args.iv_from_market_price,
            min_open_interest: Some(args.min_open_interest),
            min_volume: Some(args.min_volume),
            limit_per_strategy: Some(args.limit_per_strategy),
            bull_put_min_short_delta: Some(args.bull_put_min_short_delta),
            bull_put_max_short_delta: Some(args.bull_put_max_short_delta),
            bull_put_min_short_otm_percent: Some(args.bull_put_min_short_otm_percent),
            bull_put_max_short_otm_percent: Some(args.bull_put_max_short_otm_percent),
            bear_call_min_short_delta: Some(args.bear_call_min_short_delta),
            bear_call_max_short_delta: Some(args.bear_call_max_short_delta),
            bear_call_min_short_otm_percent: Some(args.bear_call_min_short_otm_percent),
            bear_call_max_short_otm_percent: Some(args.bear_call_max_short_otm_percent),
            min_width_per_spread: Some(args.min_width_per_spread),
            max_width_per_spread: Some(args.max_width_per_spread),
            max_bull_call_net_debit_per_spread: Some(args.max_bull_call_net_debit_per_spread),
            max_calendar_net_debit_per_spread: Some(args.max_calendar_net_debit_per_spread),
            max_diagonal_net_debit_per_spread: Some(args.max_diagonal_net_debit_per_spread),
            min_calendar_theta_carry_per_day: Some(args.min_calendar_theta_carry_per_day),
            min_diagonal_theta_carry_per_day: Some(args.min_diagonal_theta_carry_per_day),
            min_days_gap: Some(args.min_days_gap),
            max_days_gap: Some(args.max_days_gap),
            min_strike_gap: Some(args.min_strike_gap),
            max_strike_gap: Some(args.max_strike_gap),
        })
        .await?;

    build_report(view)
}

fn validate_args(args: &StrategyMonitorArgs) -> Result<()> {
    if args.near_min_dte > args.near_max_dte {
        bail!("near DTE min must be <= max");
    }
    if args.far_min_dte > args.far_max_dte {
        bail!("far DTE min must be <= max");
    }
    if args.limit_per_strategy == 0 {
        bail!("--limit-per-strategy must be greater than zero");
    }
    Ok(())
}

pub(crate) fn build_report(view: StrategyMonitorView) -> Result<StrategyMonitorReport> {
    let generated_at = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let bull_put_spread = view
        .bull_put_spread
        .candidates
        .first()
        .map(|row| -> Result<CreditSpreadSummary> {
            Ok(CreditSpreadSummary {
                candidate_count: view.bull_put_spread.candidates.len(),
                short_strike: parse_number(&row.short_strike_price)?,
                long_strike: parse_number(&row.long_strike_price)?,
                net_credit_per_spread: row.net_credit_per_spread,
                max_loss_per_spread: row.max_loss_per_spread,
                breakeven: row.breakeven,
                annualized_return_on_risk: row.annualized_return_on_risk,
            })
        })
        .transpose()?;
    let bear_call_spread = view
        .bear_call_spread
        .candidates
        .first()
        .map(|row| -> Result<CreditSpreadSummary> {
            Ok(CreditSpreadSummary {
                candidate_count: view.bear_call_spread.candidates.len(),
                short_strike: parse_number(&row.short_strike_price)?,
                long_strike: parse_number(&row.long_strike_price)?,
                net_credit_per_spread: row.net_credit_per_spread,
                max_loss_per_spread: row.max_loss_per_spread,
                breakeven: row.breakeven,
                annualized_return_on_risk: row.annualized_return_on_risk,
            })
        })
        .transpose()?;
    let bull_call_spread = view
        .bull_call_spread
        .candidates
        .first()
        .map(|row| -> Result<DebitSpreadSummary> {
            Ok(DebitSpreadSummary {
                candidate_count: view.bull_call_spread.candidates.len(),
                long_strike: parse_number(&row.long_strike_price)?,
                short_strike: parse_number(&row.short_strike_price)?,
                net_debit_per_spread: row.net_debit_per_spread,
                max_profit_per_spread: row.max_profit_per_spread,
                max_loss_per_spread: row.max_loss_per_spread,
                breakeven: row.breakeven,
                annualized_return_on_risk: row.annualized_return_on_risk,
            })
        })
        .transpose()?;
    let calendar_call_spread = view
        .calendar_call_spread
        .candidates
        .first()
        .map(|row| -> Result<CalendarSummary> {
            Ok(CalendarSummary {
                candidate_count: view.calendar_call_spread.candidates.len(),
                strike_price: parse_number(&row.strike_price)?,
                net_debit_per_spread: row.net_debit_per_spread,
                net_theta_carry_per_day: row.net_theta_carry_per_day,
                annualized_theta_carry_return_on_debit: row.annualized_theta_carry_return_on_debit,
                max_loss_per_spread: row.max_loss_per_spread,
            })
        })
        .transpose()?;
    let pmcc = view
        .diagonal_call_spread
        .candidates
        .first()
        .map(|row| -> Result<DiagonalSummary> {
            Ok(DiagonalSummary {
                candidate_count: view.diagonal_call_spread.candidates.len(),
                near_strike: parse_number(&row.near_strike_price)?,
                far_strike: parse_number(&row.far_strike_price)?,
                net_debit_per_spread: row.net_debit_per_spread,
                net_theta_carry_per_day: row.net_theta_carry_per_day,
                annualized_theta_carry_return_on_debit: row.annualized_theta_carry_return_on_debit,
                max_loss_per_spread: row.max_loss_per_spread,
            })
        })
        .transpose()?;

    let iron_condor = match (bull_put_spread.as_ref(), bear_call_spread.as_ref()) {
        (Some(put), Some(call)) if put.short_strike < call.short_strike => {
            let net_credit_per_spread = put.net_credit_per_spread + call.net_credit_per_spread;
            let put_width = put.short_strike - put.long_strike;
            let call_width = call.long_strike - call.short_strike;
            let max_loss_per_spread =
                (put_width.max(call_width) * 100.0 - net_credit_per_spread).max(0.0);
            let return_on_risk = if max_loss_per_spread > 0.0 {
                net_credit_per_spread / max_loss_per_spread
            } else {
                0.0
            };
            Some(IronCondorSummary {
                short_put_strike: put.short_strike,
                long_put_strike: put.long_strike,
                short_call_strike: call.short_strike,
                long_call_strike: call.long_strike,
                net_credit_per_spread,
                max_loss_per_spread,
                breakeven_low: put.short_strike - net_credit_per_spread / 100.0,
                breakeven_high: call.short_strike + net_credit_per_spread / 100.0,
                annualized_return_on_risk: if view.near_days_to_expiry > 0 {
                    return_on_risk * (365.0 / view.near_days_to_expiry as f64)
                } else {
                    0.0
                },
            })
        }
        _ => None,
    };

    Ok(StrategyMonitorReport {
        generated_at,
        symbol: view.symbol,
        near_expiry: view.near_expiry,
        near_days_to_expiry: view.near_days_to_expiry,
        far_expiry: view.far_expiry,
        far_days_to_expiry: view.far_days_to_expiry,
        bull_put_spread,
        bear_call_spread,
        iron_condor,
        bull_call_spread,
        calendar_call_spread,
        pmcc,
    })
}

fn parse_number(value: &str) -> Result<f64> {
    Ok(value.trim().parse::<f64>()?)
}

fn print_credit_summary(label: &str, summary: Option<&CreditSpreadSummary>) {
    let Some(summary) = summary else {
        println!("{label:<18}: none");
        return;
    };
    println!(
        "{label:<18}: {count} candidates | {short:.2}/{long:.2} | credit {credit:>7.2} | risk {risk:>7.2} | breakeven {breakeven:>7.2} | ann {annualized:>7.2}%",
        count = summary.candidate_count,
        short = summary.short_strike,
        long = summary.long_strike,
        credit = summary.net_credit_per_spread,
        risk = summary.max_loss_per_spread,
        breakeven = summary.breakeven,
        annualized = summary.annualized_return_on_risk * 100.0,
    );
}

fn print_debit_summary(label: &str, summary: Option<&DebitSpreadSummary>) {
    let Some(summary) = summary else {
        println!("{label:<18}: none");
        return;
    };
    println!(
        "{label:<18}: {count} candidates | {long:.2}/{short:.2} | debit {debit:>7.2} | max_profit {profit:>7.2} | breakeven {breakeven:>7.2} | ann {annualized:>7.2}%",
        count = summary.candidate_count,
        long = summary.long_strike,
        short = summary.short_strike,
        debit = summary.net_debit_per_spread,
        profit = summary.max_profit_per_spread,
        breakeven = summary.breakeven,
        annualized = summary.annualized_return_on_risk * 100.0,
    );
}

fn print_calendar_summary(label: &str, summary: Option<&CalendarSummary>) {
    let Some(summary) = summary else {
        println!("{label:<18}: none");
        return;
    };
    println!(
        "{label:<18}: {count} candidates | strike {strike:.2} | debit {debit:>7.2} | theta/day {theta:>7.4} | ann carry {annualized:>7.2}%",
        count = summary.candidate_count,
        strike = summary.strike_price,
        debit = summary.net_debit_per_spread,
        theta = summary.net_theta_carry_per_day,
        annualized = summary.annualized_theta_carry_return_on_debit * 100.0,
    );
}

fn print_diagonal_summary(label: &str, summary: Option<&DiagonalSummary>) {
    let Some(summary) = summary else {
        println!("{label:<18}: none");
        return;
    };
    println!(
        "{label:<18}: {count} candidates | near {near:.2} far {far:.2} | debit {debit:>7.2} | theta/day {theta:>7.4} | ann carry {annualized:>7.2}%",
        count = summary.candidate_count,
        near = summary.near_strike,
        far = summary.far_strike,
        debit = summary.net_debit_per_spread,
        theta = summary.net_theta_carry_per_day,
        annualized = summary.annualized_theta_carry_return_on_debit * 100.0,
    );
}

fn print_iron_condor(label: &str, summary: Option<&IronCondorSummary>) {
    let Some(summary) = summary else {
        println!("{label:<18}: none");
        return;
    };
    println!(
        "{label:<18}: {lp:.2}/{sp:.2} :: {sc:.2}/{lc:.2} | credit {credit:>7.2} | risk {risk:>7.2} | BE [{be_low:.2}, {be_high:.2}] | ann {annualized:>7.2}%",
        lp = summary.long_put_strike,
        sp = summary.short_put_strike,
        sc = summary.short_call_strike,
        lc = summary.long_call_strike,
        credit = summary.net_credit_per_spread,
        risk = summary.max_loss_per_spread,
        be_low = summary.breakeven_low,
        be_high = summary.breakeven_high,
        annualized = summary.annualized_return_on_risk * 100.0,
    );
}
