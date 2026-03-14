use crate::market_data::parse_expiry_date;
use crate::signal_service::{PutCallBiasRequest, ThetaSignalService};
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
#[command(name = "put-call-bias")]
#[command(about = "CLI for put/call directional demand and positioning bias")]
pub struct Cli {
    #[arg(
        long,
        default_value = "TSLA.US",
        help = "Underlying symbol. Default: TSLA.US"
    )]
    symbol: String,
    #[arg(long, help = "Expiry date in YYYY-MM-DD")]
    expiry: String,
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
        default_value_t = 0.05,
        help = "Minimum OTM percent for OTM-only bias buckets"
    )]
    min_otm_percent: f64,
    #[arg(long)]
    json: bool,
}

pub async fn run(cli: Cli) -> Result<()> {
    let service = ThetaSignalService::from_env().await?;
    let view = service
        .put_call_bias(PutCallBiasRequest {
            symbol: cli.symbol,
            expiry: parse_expiry_date(&cli.expiry)?,
            rate: cli.rate,
            dividend: cli.dividend,
            iv: cli.iv,
            iv_from_market_price: cli.iv_from_market_price,
            min_otm_percent: cli.min_otm_percent,
        })
        .await?;

    render_put_call_bias(&view, cli.json)
}

fn render_put_call_bias(view: &crate::domain::PutCallBiasView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "expiry          : {} ({}d)",
        view.expiry, view.days_to_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("otm threshold   : {:.2}%", view.min_otm_percent * 100.0);
    println!(
        "all volume/OI   : put {} / call {} | put {} / call {}",
        view.all_puts.total_volume,
        view.all_calls.total_volume,
        view.all_puts.total_open_interest,
        view.all_calls.total_open_interest,
    );
    println!(
        "all bias        : vol {:>7} | oi {:>7} | iv {:>7}",
        fmt_opt(view.volume_bias_ratio),
        fmt_opt(view.open_interest_bias_ratio),
        fmt_opt(view.average_iv_bias),
    );
    println!(
        "otm volume/OI   : put {} / call {} | put {} / call {}",
        view.otm_puts.total_volume,
        view.otm_calls.total_volume,
        view.otm_puts.total_open_interest,
        view.otm_calls.total_open_interest,
    );
    println!(
        "otm bias        : vol {:>7} | oi {:>7} | iv {:>7}",
        fmt_opt(view.otm_volume_bias_ratio),
        fmt_opt(view.otm_open_interest_bias_ratio),
        fmt_opt(view.otm_average_iv_bias),
    );

    Ok(())
}

fn fmt_opt(value: Option<f64>) -> String {
    value
        .map(|v| format!("{v:>7.4}"))
        .unwrap_or_else(|| "   n/a ".to_string())
}
