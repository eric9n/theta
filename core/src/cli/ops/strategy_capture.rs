use crate::cli::ops::strategy_monitor::{
    StrategyMonitorArgs, StrategyMonitorReport, build_report_from_args,
};
use crate::snapshot_store::{SignalSnapshotStore, StrategyMonitorSnapshotInput};
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
#[command(name = "strategy-capture")]
#[command(about = "Capture TSLA-style strategy monitor snapshots into local SQLite storage")]
pub struct Cli {
    #[command(flatten)]
    args: StrategyMonitorArgs,
    #[arg(
        long,
        help = "Path to the strategy snapshot database (default: ~/.theta/signals.db)"
    )]
    db: Option<PathBuf>,
}

pub async fn run(cli: Cli) -> Result<()> {
    let report = build_report_from_args(&cli.args).await?;
    let store = match &cli.db {
        Some(path) => SignalSnapshotStore::open(path)?,
        None => SignalSnapshotStore::open_default()?,
    };
    let input = to_snapshot_input(&report)?;
    store.record_strategy_monitor_snapshot(&input)?;

    println!(
        "{} {} near={}({}d) far={}({}d) captured",
        report.generated_at,
        report.symbol,
        report.near_expiry,
        report.near_days_to_expiry,
        report.far_expiry,
        report.far_days_to_expiry
    );
    Ok(())
}

fn to_snapshot_input(report: &StrategyMonitorReport) -> Result<StrategyMonitorSnapshotInput> {
    Ok(StrategyMonitorSnapshotInput {
        captured_at: report.generated_at.clone(),
        symbol: report.symbol.clone(),
        near_expiry: report.near_expiry.clone(),
        near_days_to_expiry: report.near_days_to_expiry,
        far_expiry: report.far_expiry.clone(),
        far_days_to_expiry: report.far_days_to_expiry,
        bull_put_short_strike: report.bull_put_spread.as_ref().map(|row| row.short_strike),
        bull_put_long_strike: report.bull_put_spread.as_ref().map(|row| row.long_strike),
        bull_put_net_credit: report
            .bull_put_spread
            .as_ref()
            .map(|row| row.net_credit_per_spread),
        bull_put_max_loss: report
            .bull_put_spread
            .as_ref()
            .map(|row| row.max_loss_per_spread),
        bull_put_breakeven: report.bull_put_spread.as_ref().map(|row| row.breakeven),
        bull_put_annualized_return: report
            .bull_put_spread
            .as_ref()
            .map(|row| row.annualized_return_on_risk),
        bear_call_short_strike: report.bear_call_spread.as_ref().map(|row| row.short_strike),
        bear_call_long_strike: report.bear_call_spread.as_ref().map(|row| row.long_strike),
        bear_call_net_credit: report
            .bear_call_spread
            .as_ref()
            .map(|row| row.net_credit_per_spread),
        bear_call_max_loss: report
            .bear_call_spread
            .as_ref()
            .map(|row| row.max_loss_per_spread),
        bear_call_breakeven: report.bear_call_spread.as_ref().map(|row| row.breakeven),
        bear_call_annualized_return: report
            .bear_call_spread
            .as_ref()
            .map(|row| row.annualized_return_on_risk),
        iron_condor_short_put_strike: report.iron_condor.as_ref().map(|row| row.short_put_strike),
        iron_condor_long_put_strike: report.iron_condor.as_ref().map(|row| row.long_put_strike),
        iron_condor_short_call_strike: report.iron_condor.as_ref().map(|row| row.short_call_strike),
        iron_condor_long_call_strike: report.iron_condor.as_ref().map(|row| row.long_call_strike),
        iron_condor_net_credit: report
            .iron_condor
            .as_ref()
            .map(|row| row.net_credit_per_spread),
        iron_condor_max_loss: report
            .iron_condor
            .as_ref()
            .map(|row| row.max_loss_per_spread),
        iron_condor_breakeven_low: report.iron_condor.as_ref().map(|row| row.breakeven_low),
        iron_condor_breakeven_high: report.iron_condor.as_ref().map(|row| row.breakeven_high),
        iron_condor_annualized_return: report
            .iron_condor
            .as_ref()
            .map(|row| row.annualized_return_on_risk),
        bull_call_long_strike: report.bull_call_spread.as_ref().map(|row| row.long_strike),
        bull_call_short_strike: report.bull_call_spread.as_ref().map(|row| row.short_strike),
        bull_call_net_debit: report
            .bull_call_spread
            .as_ref()
            .map(|row| row.net_debit_per_spread),
        bull_call_max_profit: report
            .bull_call_spread
            .as_ref()
            .map(|row| row.max_profit_per_spread),
        bull_call_max_loss: report
            .bull_call_spread
            .as_ref()
            .map(|row| row.max_loss_per_spread),
        bull_call_breakeven: report.bull_call_spread.as_ref().map(|row| row.breakeven),
        bull_call_annualized_return: report
            .bull_call_spread
            .as_ref()
            .map(|row| row.annualized_return_on_risk),
        calendar_call_strike: report
            .calendar_call_spread
            .as_ref()
            .map(|row| row.strike_price),
        calendar_call_net_debit: report
            .calendar_call_spread
            .as_ref()
            .map(|row| row.net_debit_per_spread),
        calendar_call_theta_carry_per_day: report
            .calendar_call_spread
            .as_ref()
            .map(|row| row.net_theta_carry_per_day),
        calendar_call_annualized_carry: report
            .calendar_call_spread
            .as_ref()
            .map(|row| row.annualized_theta_carry_return_on_debit),
        calendar_call_max_loss: report
            .calendar_call_spread
            .as_ref()
            .map(|row| row.max_loss_per_spread),
        pmcc_near_strike: report.pmcc.as_ref().map(|row| row.near_strike),
        pmcc_far_strike: report.pmcc.as_ref().map(|row| row.far_strike),
        pmcc_net_debit: report.pmcc.as_ref().map(|row| row.net_debit_per_spread),
        pmcc_theta_carry_per_day: report.pmcc.as_ref().map(|row| row.net_theta_carry_per_day),
        pmcc_annualized_carry: report
            .pmcc
            .as_ref()
            .map(|row| row.annualized_theta_carry_return_on_debit),
        pmcc_max_loss: report.pmcc.as_ref().map(|row| row.max_loss_per_spread),
        report_json: serde_json::to_string(report)?,
    })
}
