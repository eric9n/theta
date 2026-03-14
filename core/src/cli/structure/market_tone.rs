use crate::market_data::parse_expiry_date;
use crate::signal_service::{MarketToneRequest, ThetaSignalService};
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
#[command(name = "market-tone")]
#[command(about = "Summarize skew, bias, and term structure")]
pub struct Cli {
    #[arg(
        long,
        default_value = "TSLA.US",
        help = "Underlying symbol. Default: TSLA.US"
    )]
    symbol: String,
    #[arg(long, help = "Front expiry date in YYYY-MM-DD")]
    expiry: String,
    #[arg(
        long,
        default_value_t = 1,
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
    #[arg(long)]
    json: bool,
}

pub async fn run(cli: Cli) -> Result<()> {
    let service = ThetaSignalService::from_env().await?;
    let smile_targets = if cli.smile_target_otm_percents.is_empty() {
        vec![0.05, 0.10, 0.15]
    } else {
        cli.smile_target_otm_percents
    };

    let view = service
        .market_tone(MarketToneRequest {
            symbol: cli.symbol,
            expiry: parse_expiry_date(&cli.expiry)?,
            expiries_limit: cli.expiries_limit,
            rate: cli.rate,
            dividend: cli.dividend,
            iv: cli.iv,
            iv_from_market_price: cli.iv_from_market_price,
            target_delta: cli.target_delta,
            target_otm_percent: cli.target_otm_percent,
            smile_target_otm_percents: smile_targets,
            bias_min_otm_percent: cli.bias_min_otm_percent,
        })
        .await?;

    render_market_tone(&view, cli.json)
}

fn render_market_tone(view: &crate::domain::MarketToneView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!("underlying      : {}", view.underlying_symbol);
    println!("front expiry    : {}", view.front_expiry);
    println!(
        "summary         : delta_skew {:>7} | otm_skew {:>7} | front_atm {:>7.4}",
        fmt_opt(view.summary.delta_skew),
        fmt_opt(view.summary.otm_skew),
        view.summary.front_atm_iv,
    );
    println!(
        "term            : far_atm {:>7} | change {:>7}",
        fmt_opt(view.summary.farthest_atm_iv),
        fmt_opt(view.summary.term_structure_change_from_front),
    );
    println!(
        "wings           : put {:>7} | call {:>7}",
        fmt_opt(view.summary.put_wing_slope),
        fmt_opt(view.summary.call_wing_slope),
    );
    println!(
        "positioning      : oi {:>7} | otm_oi {:>7}",
        fmt_opt(view.summary.open_interest_bias_ratio),
        fmt_opt(view.summary.otm_open_interest_bias_ratio),
    );
    println!(
        "iv bias         : all {:>7} | otm {:>7}",
        fmt_opt(view.summary.average_iv_bias),
        fmt_opt(view.summary.otm_average_iv_bias),
    );
    println!(
        "labels          : protection {} | term {} | wings {} | positioning {}",
        view.summary.downside_protection,
        view.summary.term_structure_shape,
        view.summary.wing_shape,
        view.summary.positioning_bias,
    );
    println!("overall tone    : {}", view.summary.overall_tone);
    println!("takeaway        : {}", view.summary.summary_sentence);

    Ok(())
}

fn fmt_opt(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:>7.4}"))
        .unwrap_or_else(|| "   n/a ".to_string())
}
