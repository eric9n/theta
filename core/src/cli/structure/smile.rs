use crate::market_data::parse_expiry_date;
use crate::signal_service::{SmileSignalRequest, ThetaSignalService};
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
#[command(name = "smile")]
#[command(about = "CLI for option smile and wing structure signals")]
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
        long = "target-otm-percent",
        help = "Target OTM percent(s); repeatable, e.g. --target-otm-percent 0.05 --target-otm-percent 0.10"
    )]
    target_otm_percents: Vec<f64>,
    #[arg(long)]
    json: bool,
}

pub async fn run(cli: Cli) -> Result<()> {
    let service = ThetaSignalService::from_env().await?;
    let targets = if cli.target_otm_percents.is_empty() {
        vec![0.05, 0.10, 0.15]
    } else {
        cli.target_otm_percents
    };

    let view = service
        .smile(SmileSignalRequest {
            symbol: cli.symbol,
            expiry: parse_expiry_date(&cli.expiry)?,
            rate: cli.rate,
            dividend: cli.dividend,
            iv: cli.iv,
            iv_from_market_price: cli.iv_from_market_price,
            target_otm_percents: targets,
        })
        .await?;

    render_smile(&view, cli.json)
}

fn render_smile(view: &crate::domain::SmileSignalView, json: bool) -> Result<()> {
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
    println!(
        "atm             : strike {} | iv {:.4}",
        view.atm_strike_price, view.atm_iv
    );
    println!(
        "wing slopes     : put {:>8} | call {:>8}",
        view.put_wing_slope
            .map(|v| format!("{v:>8.4}"))
            .unwrap_or_else(|| "    n/a ".to_string()),
        view.call_wing_slope
            .map(|v| format!("{v:>8.4}"))
            .unwrap_or_else(|| "    n/a ".to_string()),
    );

    println!("put wing:");
    for point in &view.put_points {
        println!(
            "  target {:>6.2}% | {} @ {} | delta {:>7.4} | actual {:>6.2}% | iv {:>7.4} | vs_atm {:>7.4}",
            point.target_otm_percent * 100.0,
            point.option_symbol,
            point.strike_price,
            point.delta,
            point.otm_percent * 100.0,
            point.implied_volatility,
            point.iv_vs_atm,
        );
    }

    println!("call wing:");
    for point in &view.call_points {
        println!(
            "  target {:>6.2}% | {} @ {} | delta {:>7.4} | actual {:>6.2}% | iv {:>7.4} | vs_atm {:>7.4}",
            point.target_otm_percent * 100.0,
            point.option_symbol,
            point.strike_price,
            point.delta,
            point.otm_percent * 100.0,
            point.implied_volatility,
            point.iv_vs_atm,
        );
    }

    Ok(())
}
