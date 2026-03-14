use crate::cli::signals::put_call_monitor::{
    MonitorMetricView, PutCallMonitorView, build_monitor_view,
};
use crate::snapshot_store::{FrontAtmIvRankRow, SignalSnapshotStore};
use anyhow::Result;
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Args, Debug)]
#[command(name = "alert")]
#[command(about = "Build a taskd-friendly alert payload from stored signal snapshots")]
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
    #[arg(
        long,
        default_value_t = 1.5,
        help = "Absolute z-score threshold used to flag a directional move as statistically meaningful"
    )]
    sigma_threshold: f64,
    #[arg(
        long,
        default_value_t = 1.1,
        help = "Put-dominant open-interest ratio threshold"
    )]
    put_dominance_ratio: f64,
    #[arg(
        long,
        default_value_t = 0.9,
        help = "Call-dominant open-interest ratio threshold"
    )]
    call_dominance_ratio: f64,
    #[arg(
        long,
        default_value_t = 0.01,
        help = "Minimum absolute skew needed before calling one side rich"
    )]
    skew_threshold: f64,
    #[arg(
        long,
        default_value_t = 15,
        help = "Maximum allowed sample age in minutes before suppressing notification"
    )]
    max_staleness_minutes: i64,
    #[arg(
        long,
        default_value_t = 20,
        help = "Minimum number of samples required before sending notifications"
    )]
    min_samples: usize,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct AlertPayload {
    notify: bool,
    kind: &'static str,
    version: u8,
    severity: String,
    title: String,
    summary: String,
    event_key: String,
    captured_at: String,
    symbol: String,
    signal: AlertSignal,
    context: AlertContext,
    metrics: AlertMetrics,
    rationale: Vec<String>,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct AlertSignal {
    composite: String,
    richness_state: String,
    confirmation_state: String,
    confidence: String,
}

#[derive(Debug, Serialize)]
struct AlertContext {
    sample_count: usize,
    front_expiry: String,
    days_to_expiry: Option<i64>,
    history_regime: String,
    max_staleness_minutes: i64,
    sample_age_minutes: Option<i64>,
}

#[derive(Debug, Serialize)]
struct AlertMetrics {
    richness: Option<MonitorMetricView>,
    confirmation: Option<MonitorMetricView>,
    iv_rank: Option<AlertIvRank>,
}

#[derive(Debug, Serialize)]
struct AlertIvRank {
    value: Option<f64>,
    current_front_atm_iv: f64,
    min_front_atm_iv: f64,
    max_front_atm_iv: f64,
}

pub fn run(cli: Cli) -> Result<()> {
    let store = match &cli.db {
        Some(path) => SignalSnapshotStore::open(path)?,
        None => SignalSnapshotStore::open_default()?,
    };
    let row = store.compute_market_extreme(&cli.symbol, cli.limit)?;
    let iv_rank = store.compute_front_atm_iv_rank(&cli.symbol, cli.limit)?;

    let payload = build_alert_payload(&cli.symbol, row, iv_rank, &cli);

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&payload)?);
    }

    Ok(())
}

fn build_alert_payload(
    symbol: &str,
    row: Option<crate::snapshot_store::MarketExtremeRow>,
    iv_rank: Option<FrontAtmIvRankRow>,
    cli: &Cli,
) -> AlertPayload {
    let Some(row) = row else {
        return AlertPayload {
            notify: false,
            kind: "theta.signal_alert",
            version: 1,
            severity: "info".to_string(),
            title: format!("{symbol} signal alert"),
            summary: "No stored signal samples found.".to_string(),
            event_key: format!("{symbol}:no_data"),
            captured_at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| "unknown".to_string()),
            symbol: symbol.to_string(),
            signal: AlertSignal {
                composite: "insufficient_data".to_string(),
                richness_state: "insufficient_data".to_string(),
                confirmation_state: "insufficient_data".to_string(),
                confidence: "low".to_string(),
            },
            context: AlertContext {
                sample_count: 0,
                front_expiry: "n/a".to_string(),
                days_to_expiry: None,
                history_regime: "unknown".to_string(),
                max_staleness_minutes: cli.max_staleness_minutes,
                sample_age_minutes: None,
            },
            metrics: AlertMetrics {
                richness: None,
                confirmation: None,
                iv_rank: None,
            },
            rationale: vec!["signals.db has no samples for this symbol".to_string()],
            reason: Some("no_samples".to_string()),
        };
    };

    let monitor = build_monitor_view(
        &row,
        cli.sigma_threshold,
        cli.put_dominance_ratio,
        cli.call_dominance_ratio,
        cli.skew_threshold,
    );
    let sample_age_minutes = compute_sample_age_minutes(&monitor.current_captured_at);
    let is_stale = sample_age_minutes.is_none_or(|age| age > cli.max_staleness_minutes);
    let history_regime = classify_history_regime(monitor.current_days_to_expiry);
    let confidence = classify_confidence(monitor.sample_count, history_regime, cli.min_samples);

    let mut notify = matches!(
        monitor.composite_signal.as_str(),
        "put_rich_extreme"
            | "put_rich_extreme_confirmed"
            | "call_rich_extreme"
            | "call_rich_extreme_confirmed"
    );
    let mut reason = None;

    if monitor.sample_count < cli.min_samples {
        notify = false;
        reason = Some("insufficient_samples".to_string());
    } else if is_stale {
        notify = false;
        reason = Some("stale_sample".to_string());
    } else if monitor.composite_signal == "balanced" {
        notify = false;
        reason = Some("balanced".to_string());
    } else if !matches!(
        monitor.composite_signal.as_str(),
        "put_rich_extreme"
            | "put_rich_extreme_confirmed"
            | "call_rich_extreme"
            | "call_rich_extreme_confirmed"
    ) {
        notify = false;
        reason = Some("below_alert_threshold".to_string());
    }

    let severity = match monitor.composite_signal.as_str() {
        "put_rich_extreme_confirmed" | "call_rich_extreme_confirmed" => "critical",
        "put_rich_extreme" | "call_rich_extreme" => "warning",
        _ => "info",
    }
    .to_string();

    let title = match monitor.composite_signal.as_str() {
        "put_rich_extreme_confirmed" => format!("{} put skew extreme confirmed", monitor.symbol),
        "call_rich_extreme_confirmed" => {
            format!("{} call skew extreme confirmed", monitor.symbol)
        }
        "put_rich_extreme" => format!("{} put skew extreme", monitor.symbol),
        "call_rich_extreme" => format!("{} call skew extreme", monitor.symbol),
        _ => format!("{} signal check", monitor.symbol),
    };

    let summary = if notify {
        build_summary(&monitor, history_regime)
    } else {
        build_suppressed_summary(&monitor, reason.as_deref().unwrap_or("suppressed"))
    };

    let mut rationale = vec![
        monitor.richness.rationale.clone(),
        monitor.confirmation.rationale.clone(),
    ];
    if history_regime == "mixed" {
        rationale.push("history currently mixes old and new expiry-selection regimes".to_string());
    }
    if monitor.sample_count < cli.min_samples {
        rationale.push(format!(
            "sample_count {} is below minimum {}",
            monitor.sample_count, cli.min_samples
        ));
    }
    if let Some(age) = sample_age_minutes
        && age > cli.max_staleness_minutes
    {
        rationale.push(format!(
            "latest sample is {} minutes old, above max staleness {}",
            age, cli.max_staleness_minutes
        ));
    }

    AlertPayload {
        notify,
        kind: "theta.signal_alert",
        version: 1,
        severity,
        title,
        summary,
        event_key: format!(
            "{}:{}:{}",
            monitor.symbol, monitor.composite_signal, monitor.current_front_expiry
        ),
        captured_at: monitor.current_captured_at.clone(),
        symbol: monitor.symbol.clone(),
        signal: AlertSignal {
            composite: monitor.composite_signal.clone(),
            richness_state: monitor.richness.state.clone(),
            confirmation_state: monitor.confirmation.state.clone(),
            confidence: confidence.to_string(),
        },
        context: AlertContext {
            sample_count: monitor.sample_count,
            front_expiry: monitor.current_front_expiry.clone(),
            days_to_expiry: monitor.current_days_to_expiry,
            history_regime: history_regime.to_string(),
            max_staleness_minutes: cli.max_staleness_minutes,
            sample_age_minutes,
        },
        metrics: AlertMetrics {
            richness: monitor.richness_metric.clone(),
            confirmation: monitor.confirmation_metric.clone(),
            iv_rank: iv_rank.map(|row| AlertIvRank {
                value: row.iv_rank,
                current_front_atm_iv: row.current_front_atm_iv,
                min_front_atm_iv: row.min_front_atm_iv,
                max_front_atm_iv: row.max_front_atm_iv,
            }),
        },
        rationale,
        reason,
    }
}

fn compute_sample_age_minutes(captured_at: &str) -> Option<i64> {
    let captured = OffsetDateTime::parse(captured_at, &Rfc3339).ok()?;
    let delta = OffsetDateTime::now_utc() - captured;
    Some(delta.whole_minutes())
}

fn classify_history_regime(days_to_expiry: Option<i64>) -> &'static str {
    match days_to_expiry {
        Some(days) if (14..=45).contains(&days) => "aligned",
        Some(_) => "mixed",
        None => "unknown",
    }
}

fn classify_confidence(
    sample_count: usize,
    history_regime: &str,
    min_samples: usize,
) -> &'static str {
    if history_regime != "aligned" || sample_count < min_samples {
        "low"
    } else {
        "normal"
    }
}

fn build_summary(view: &PutCallMonitorView, history_regime: &str) -> String {
    let base = match view.composite_signal.as_str() {
        "put_rich_extreme_confirmed" => {
            format!(
                "{} put wing is historically rich and confirmed by OI bias.",
                view.symbol
            )
        }
        "call_rich_extreme_confirmed" => {
            format!(
                "{} call wing is historically rich and confirmed by OI bias.",
                view.symbol
            )
        }
        "put_rich_extreme" => format!("{} put wing is historically rich.", view.symbol),
        "call_rich_extreme" => format!("{} call wing is historically rich.", view.symbol),
        other => format!("{} signal state: {}.", view.symbol, other),
    };

    if history_regime == "mixed" {
        format!("{base} History still mixes old and new sampling regimes.")
    } else {
        base
    }
}

fn build_suppressed_summary(view: &PutCallMonitorView, reason: &str) -> String {
    format!(
        "{} alert suppressed: {} (signal {}).",
        view.symbol, reason, view.composite_signal
    )
}

#[cfg(test)]
mod tests {
    use super::{
        Cli, build_alert_payload, classify_confidence, classify_history_regime,
        compute_sample_age_minutes,
    };
    use crate::snapshot_store::{FrontAtmIvRankRow, MarketExtremeMetricStat, MarketExtremeRow};
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};

    #[test]
    fn classifies_history_regime_from_days_to_expiry() {
        assert_eq!(classify_history_regime(Some(27)), "aligned");
        assert_eq!(classify_history_regime(Some(3)), "mixed");
        assert_eq!(classify_history_regime(None), "unknown");
    }

    #[test]
    fn lowers_confidence_for_mixed_history() {
        assert_eq!(classify_confidence(252, "mixed", 20), "low");
        assert_eq!(classify_confidence(252, "aligned", 20), "normal");
    }

    #[test]
    fn builds_notify_true_for_confirmed_extreme() {
        let captured_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .expect("rfc3339 timestamp");
        let row = MarketExtremeRow {
            symbol: "TSLA.US".to_string(),
            sample_count: 120,
            current_captured_at: captured_at.clone(),
            current_front_expiry: "2026-04-10".to_string(),
            delta_skew: None,
            otm_skew: Some(MarketExtremeMetricStat {
                current: 0.18,
                mean: 0.05,
                std_dev: 0.04,
                z_score: Some(3.25),
                sample_count: 120,
            }),
            front_atm_iv: MarketExtremeMetricStat {
                current: 0.62,
                mean: 0.45,
                std_dev: 0.05,
                z_score: Some(3.4),
                sample_count: 120,
            },
            term_structure_change_from_front: None,
            open_interest_bias_ratio: None,
            otm_open_interest_bias_ratio: Some(MarketExtremeMetricStat {
                current: 1.31,
                mean: 1.02,
                std_dev: 0.12,
                z_score: Some(2.42),
                sample_count: 120,
            }),
            average_iv_bias: None,
            otm_average_iv_bias: None,
        };
        let iv_rank = Some(FrontAtmIvRankRow {
            symbol: "TSLA.US".to_string(),
            sample_count: 120,
            current_captured_at: captured_at,
            current_front_expiry: "2026-04-10".to_string(),
            current_front_atm_iv: 0.62,
            min_front_atm_iv: 0.25,
            max_front_atm_iv: 0.75,
            iv_rank: Some(0.74),
        });
        let cli = Cli {
            symbol: "TSLA.US".to_string(),
            db: None,
            limit: 252,
            sigma_threshold: 1.5,
            put_dominance_ratio: 1.1,
            call_dominance_ratio: 0.9,
            skew_threshold: 0.01,
            max_staleness_minutes: 15,
            min_samples: 20,
            json: true,
        };

        let payload = build_alert_payload("TSLA.US", Some(row), iv_rank, &cli);
        assert!(payload.notify);
        assert_eq!(payload.signal.composite, "put_rich_extreme_confirmed");
        assert_eq!(payload.severity, "critical");
    }

    #[test]
    fn suppresses_stale_samples() {
        let payload = build_alert_payload(
            "TSLA.US",
            Some(MarketExtremeRow {
                symbol: "TSLA.US".to_string(),
                sample_count: 120,
                current_captured_at: "2026-03-10T04:00:00Z".to_string(),
                current_front_expiry: "2026-04-10".to_string(),
                delta_skew: None,
                otm_skew: Some(MarketExtremeMetricStat {
                    current: 0.18,
                    mean: 0.05,
                    std_dev: 0.04,
                    z_score: Some(3.25),
                    sample_count: 120,
                }),
                front_atm_iv: MarketExtremeMetricStat {
                    current: 0.62,
                    mean: 0.45,
                    std_dev: 0.05,
                    z_score: Some(3.4),
                    sample_count: 120,
                },
                term_structure_change_from_front: None,
                open_interest_bias_ratio: None,
                otm_open_interest_bias_ratio: Some(MarketExtremeMetricStat {
                    current: 1.31,
                    mean: 1.02,
                    std_dev: 0.12,
                    z_score: Some(2.42),
                    sample_count: 120,
                }),
                average_iv_bias: None,
                otm_average_iv_bias: None,
            }),
            None,
            &Cli {
                symbol: "TSLA.US".to_string(),
                db: None,
                limit: 252,
                sigma_threshold: 1.5,
                put_dominance_ratio: 1.1,
                call_dominance_ratio: 0.9,
                skew_threshold: 0.01,
                max_staleness_minutes: 15,
                min_samples: 20,
                json: true,
            },
        );
        assert!(!payload.notify);
        assert_eq!(payload.reason.as_deref(), Some("stale_sample"));
    }

    #[test]
    fn computes_sample_age_when_rfc3339_is_valid() {
        assert!(compute_sample_age_minutes("2026-03-14T04:00:00Z").is_some());
    }
}
