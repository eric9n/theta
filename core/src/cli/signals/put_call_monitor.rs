use crate::market_data::parse_expiry_date;
use crate::snapshot_store::{MarketExtremeMetricStat, MarketExtremeRow, SignalSnapshotStore};
use anyhow::Result;
use clap::Args;
use serde::Serialize;
use std::path::PathBuf;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Args, Debug)]
#[command(name = "monitor")]
#[command(alias = "put-call-monitor")]
#[command(about = "Monitor whether puts or calls are rich, using skew as the primary signal")]
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
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Serialize)]
struct MonitorMetricView {
    source: String,
    current: f64,
    mean: f64,
    std_dev: f64,
    z_score: Option<f64>,
    sigma_label: String,
}

#[derive(Debug, Serialize)]
struct MonitorAssessment {
    state: String,
    source: String,
    current: Option<f64>,
    z_score: Option<f64>,
    sigma_triggered: bool,
    rationale: String,
}

#[derive(Debug, Serialize)]
struct PutCallMonitorView {
    symbol: String,
    sample_count: usize,
    current_captured_at: String,
    current_front_expiry: String,
    current_days_to_expiry: Option<i64>,
    sigma_threshold: f64,
    richness_metric: Option<MonitorMetricView>,
    confirmation_metric: Option<MonitorMetricView>,
    richness: MonitorAssessment,
    confirmation: MonitorAssessment,
    composite_signal: String,
}

pub fn run(cli: Cli) -> Result<()> {
    let store = match &cli.db {
        Some(path) => SignalSnapshotStore::open(path)?,
        None => SignalSnapshotStore::open_default()?,
    };
    let row = store.compute_market_extreme(&cli.symbol, cli.limit)?;

    let Some(row) = row else {
        println!("No market extreme samples found for {}.", cli.symbol);
        return Ok(());
    };

    let view = build_monitor_view(
        &row,
        cli.sigma_threshold,
        cli.put_dominance_ratio,
        cli.call_dominance_ratio,
        cli.skew_threshold,
    );

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&view)?);
        return Ok(());
    }

    println!("symbol             : {}", view.symbol);
    println!("samples            : {}", view.sample_count);
    println!("current_at         : {}", view.current_captured_at);
    println!("front_expiry       : {}", view.current_front_expiry);
    println!(
        "days_to_expiry     : {}",
        fmt_opt_i64(view.current_days_to_expiry)
    );
    println!("sigma_threshold    : {:.2}", view.sigma_threshold);
    println!();
    print_monitor_metric("richness_metric", view.richness_metric.as_ref());
    println!(
        "richness_state    : {} | sigma {}",
        view.richness.state,
        yes_no(view.richness.sigma_triggered)
    );
    println!("richness_note     : {}", view.richness.rationale);
    println!();
    print_monitor_metric("confirmation_metric", view.confirmation_metric.as_ref());
    println!(
        "confirm_state     : {} | sigma {}",
        view.confirmation.state,
        yes_no(view.confirmation.sigma_triggered)
    );
    println!("confirm_note      : {}", view.confirmation.rationale);
    println!();
    println!("composite_signal   : {}", view.composite_signal);

    Ok(())
}

fn build_monitor_view(
    row: &MarketExtremeRow,
    sigma_threshold: f64,
    put_dominance_ratio: f64,
    call_dominance_ratio: f64,
    skew_threshold: f64,
) -> PutCallMonitorView {
    let richness_source = choose_metric(
        "otm_skew",
        row.otm_skew.as_ref(),
        "delta_skew",
        row.delta_skew.as_ref(),
    );
    let confirmation_source = choose_metric(
        "otm_open_interest_bias_ratio",
        row.otm_open_interest_bias_ratio.as_ref(),
        "open_interest_bias_ratio",
        row.open_interest_bias_ratio.as_ref(),
    );

    let richness = classify_richness(richness_source, sigma_threshold, skew_threshold);
    let confirmation = classify_positioning(
        confirmation_source,
        sigma_threshold,
        put_dominance_ratio,
        call_dominance_ratio,
    );

    PutCallMonitorView {
        symbol: row.symbol.clone(),
        sample_count: row.sample_count,
        current_captured_at: row.current_captured_at.clone(),
        current_front_expiry: row.current_front_expiry.clone(),
        current_days_to_expiry: compute_days_to_expiry(
            &row.current_captured_at,
            &row.current_front_expiry,
        ),
        sigma_threshold,
        richness_metric: richness_source.map(build_metric_view),
        confirmation_metric: confirmation_source.map(build_metric_view),
        composite_signal: build_composite_signal(&richness.state, &confirmation.state),
        richness,
        confirmation,
    }
}

fn choose_metric<'a>(
    preferred_name: &'static str,
    preferred: Option<&'a MarketExtremeMetricStat>,
    fallback_name: &'static str,
    fallback: Option<&'a MarketExtremeMetricStat>,
) -> Option<(&'static str, &'a MarketExtremeMetricStat)> {
    preferred
        .map(|stat| (preferred_name, stat))
        .or_else(|| fallback.map(|stat| (fallback_name, stat)))
}

fn build_metric_view((source, stat): (&str, &MarketExtremeMetricStat)) -> MonitorMetricView {
    MonitorMetricView {
        source: source.to_string(),
        current: stat.current,
        mean: stat.mean,
        std_dev: stat.std_dev,
        z_score: stat.z_score,
        sigma_label: classify_z_score(stat.z_score).to_string(),
    }
}

fn classify_positioning(
    metric: Option<(&str, &MarketExtremeMetricStat)>,
    sigma_threshold: f64,
    put_dominance_ratio: f64,
    call_dominance_ratio: f64,
) -> MonitorAssessment {
    let Some((source, stat)) = metric else {
        return MonitorAssessment {
            state: "insufficient_data".to_string(),
            source: "n/a".to_string(),
            current: None,
            z_score: None,
            sigma_triggered: false,
            rationale: "missing open-interest bias history".to_string(),
        };
    };

    if stat.current >= put_dominance_ratio {
        let sigma_triggered = stat.z_score.is_some_and(|z| z >= sigma_threshold);
        return MonitorAssessment {
            state: if sigma_triggered {
                "put_dominant_sigma".to_string()
            } else {
                "put_dominant".to_string()
            },
            source: source.to_string(),
            current: Some(stat.current),
            z_score: stat.z_score,
            sigma_triggered,
            rationale: format!(
                "{} {:.4} >= {:.4}; puts carry more open interest than calls",
                source, stat.current, put_dominance_ratio
            ),
        };
    }

    if stat.current <= call_dominance_ratio {
        let sigma_triggered = stat.z_score.is_some_and(|z| z <= -sigma_threshold);
        return MonitorAssessment {
            state: if sigma_triggered {
                "call_dominant_sigma".to_string()
            } else {
                "call_dominant".to_string()
            },
            source: source.to_string(),
            current: Some(stat.current),
            z_score: stat.z_score,
            sigma_triggered,
            rationale: format!(
                "{} {:.4} <= {:.4}; calls carry more open interest than puts",
                source, stat.current, call_dominance_ratio
            ),
        };
    }

    MonitorAssessment {
        state: "balanced".to_string(),
        source: source.to_string(),
        current: Some(stat.current),
        z_score: stat.z_score,
        sigma_triggered: false,
        rationale: format!(
            "{} {:.4} is between {:.4} and {:.4}",
            source, stat.current, call_dominance_ratio, put_dominance_ratio
        ),
    }
}

fn classify_richness(
    metric: Option<(&str, &MarketExtremeMetricStat)>,
    sigma_threshold: f64,
    skew_threshold: f64,
) -> MonitorAssessment {
    let Some((source, stat)) = metric else {
        return MonitorAssessment {
            state: "insufficient_data".to_string(),
            source: "n/a".to_string(),
            current: None,
            z_score: None,
            sigma_triggered: false,
            rationale: "missing skew history".to_string(),
        };
    };

    if stat.current >= skew_threshold {
        let sigma_triggered = stat.z_score.is_some_and(|z| z >= sigma_threshold);
        return MonitorAssessment {
            state: if sigma_triggered {
                "put_rich_sigma".to_string()
            } else {
                "put_rich".to_string()
            },
            source: source.to_string(),
            current: Some(stat.current),
            z_score: stat.z_score,
            sigma_triggered,
            rationale: format!(
                "{} {:.4} >= {:.4}; put wing trades richer than call wing",
                source, stat.current, skew_threshold
            ),
        };
    }

    if stat.current <= -skew_threshold {
        let sigma_triggered = stat.z_score.is_some_and(|z| z <= -sigma_threshold);
        return MonitorAssessment {
            state: if sigma_triggered {
                "call_rich_sigma".to_string()
            } else {
                "call_rich".to_string()
            },
            source: source.to_string(),
            current: Some(stat.current),
            z_score: stat.z_score,
            sigma_triggered,
            rationale: format!(
                "{} {:.4} <= -{:.4}; call wing trades richer than put wing",
                source, stat.current, skew_threshold
            ),
        };
    }

    MonitorAssessment {
        state: "balanced".to_string(),
        source: source.to_string(),
        current: Some(stat.current),
        z_score: stat.z_score,
        sigma_triggered: false,
        rationale: format!(
            "{} {:.4} stays within +/-{:.4}",
            source, stat.current, skew_threshold
        ),
    }
}

fn build_composite_signal(richness: &str, confirmation: &str) -> String {
    match (richness, confirmation) {
        ("put_rich_sigma", "put_dominant_sigma") => "put_rich_extreme_confirmed".to_string(),
        ("put_rich", "put_dominant") | ("put_rich_sigma", "put_dominant") => {
            "put_rich_confirmed".to_string()
        }
        ("call_rich_sigma", "call_dominant_sigma") => "call_rich_extreme_confirmed".to_string(),
        ("call_rich", "call_dominant") | ("call_rich_sigma", "call_dominant") => {
            "call_rich_confirmed".to_string()
        }
        ("put_rich_sigma", _) => "put_rich_extreme".to_string(),
        ("call_rich_sigma", _) => "call_rich_extreme".to_string(),
        ("put_rich", _) => "put_rich".to_string(),
        ("call_rich", _) => "call_rich".to_string(),
        (left, right) if left.starts_with("put_rich") && right.starts_with("call_dominant") => {
            "put_rich_without_oi_confirmation".to_string()
        }
        (left, right) if left.starts_with("call_rich") && right.starts_with("put_dominant") => {
            "call_rich_without_oi_confirmation".to_string()
        }
        _ => "balanced".to_string(),
    }
}

fn print_monitor_metric(label: &str, metric: Option<&MonitorMetricView>) {
    let Some(metric) = metric else {
        println!("{label:<18}: n/a");
        return;
    };

    println!(
        "{label:<18}: {source} | current {current:>7.4} | mean {mean:>7.4} | std {std_dev:>7.4} | z {z} ({sigma})",
        source = metric.source,
        current = metric.current,
        mean = metric.mean,
        std_dev = metric.std_dev,
        z = fmt_opt(metric.z_score),
        sigma = metric.sigma_label,
    );
}

fn fmt_opt(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:>7.3}"))
        .unwrap_or_else(|| "   n/a ".to_string())
}

fn fmt_opt_i64(value: Option<i64>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "n/a".to_string())
}

fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn classify_z_score(z_score: Option<f64>) -> &'static str {
    let Some(z_score) = z_score else {
        return "flat_history";
    };

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

fn compute_days_to_expiry(captured_at: &str, expiry: &str) -> Option<i64> {
    let captured_date = OffsetDateTime::parse(captured_at, &Rfc3339).ok()?.date();
    let expiry = parse_expiry_date(expiry).ok()?;
    let days = (expiry - captured_date).whole_days();

    Some(if days <= 0 { 1 } else { days })
}

#[cfg(test)]
mod tests {
    use super::{
        build_composite_signal, classify_positioning, classify_richness, compute_days_to_expiry,
    };
    use crate::snapshot_store::MarketExtremeMetricStat;

    #[test]
    fn classifies_put_dominance_with_sigma() {
        let stat = MarketExtremeMetricStat {
            current: 1.6,
            mean: 1.2,
            std_dev: 0.1,
            z_score: Some(4.0),
            sample_count: 20,
        };

        let assessment =
            classify_positioning(Some(("otm_open_interest_bias_ratio", &stat)), 1.5, 1.1, 0.9);
        assert_eq!(assessment.state, "put_dominant_sigma");
        assert!(assessment.sigma_triggered);
    }

    #[test]
    fn classifies_call_rich_with_sigma() {
        let stat = MarketExtremeMetricStat {
            current: -0.04,
            mean: 0.00,
            std_dev: 0.01,
            z_score: Some(-4.0),
            sample_count: 20,
        };

        let assessment = classify_richness(Some(("otm_skew", &stat)), 1.5, 0.01);
        assert_eq!(assessment.state, "call_rich_sigma");
        assert!(assessment.sigma_triggered);
    }

    #[test]
    fn flags_rich_signal_without_confirmation() {
        assert_eq!(
            build_composite_signal("put_rich_sigma", "call_dominant_sigma"),
            "put_rich_extreme"
        );
    }

    #[test]
    fn computes_days_to_expiry_from_snapshot_timestamp() {
        assert_eq!(
            compute_days_to_expiry("2026-03-13T19:55:00.215246102Z", "2026-03-20"),
            Some(7)
        );
    }
}
