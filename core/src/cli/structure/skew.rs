use crate::market_data::parse_expiry_date;
use crate::signal_service::{SkewSignalRequest, ThetaSignalService};
use anyhow::Result;
use clap::Args;

#[derive(Args, Debug)]
#[command(name = "skew")]
#[command(about = "Compare put-wing vs call-wing IV")]
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
        default_value_t = 0.25,
        help = "Target absolute delta for skew matching"
    )]
    target_delta: f64,
    #[arg(
        long,
        default_value_t = 0.05,
        help = "Target OTM percent for wing matching"
    )]
    target_otm_percent: f64,
    #[arg(long)]
    json: bool,
}

pub async fn run(cli: Cli) -> Result<()> {
    let service = ThetaSignalService::from_env().await?;
    let view = service
        .skew(SkewSignalRequest {
            symbol: cli.symbol,
            expiry: parse_expiry_date(&cli.expiry)?,
            rate: cli.rate,
            dividend: cli.dividend,
            iv: cli.iv,
            iv_from_market_price: cli.iv_from_market_price,
            target_delta: cli.target_delta,
            target_otm_percent: cli.target_otm_percent,
        })
        .await?;

    render_skew(&view, cli.json)
}

fn render_skew(view: &crate::domain::SkewSignalView, json: bool) -> Result<()> {
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
        "targets         : delta {:.3} | otm {:.2}%",
        view.target_delta,
        view.target_otm_percent * 100.0
    );
    println!(
        "atm             : strike {} | iv {:.4}",
        view.atm_strike_price, view.atm_iv
    );

    if let Some(put) = &view.delta_put {
        println!(
            "delta put       : {} @ {} | delta {:>7.4} | otm {:>6.2}% | iv {:>7.4}",
            put.option_symbol,
            put.strike_price,
            put.delta,
            put.otm_percent * 100.0,
            put.implied_volatility,
        );
    }
    if let Some(call) = &view.delta_call {
        println!(
            "delta call      : {} @ {} | delta {:>7.4} | otm {:>6.2}% | iv {:>7.4}",
            call.option_symbol,
            call.strike_price,
            call.delta,
            call.otm_percent * 100.0,
            call.implied_volatility,
        );
    }
    if let Some(skew) = view.delta_skew {
        println!(
            "delta skew      : {:>7.4} | put-atm {:>7.4} | call-atm {:>7.4}",
            skew,
            view.delta_put_wing_vs_atm.unwrap_or(0.0),
            view.delta_call_wing_vs_atm.unwrap_or(0.0),
        );
    }

    if let Some(put) = &view.otm_put {
        println!(
            "otm put         : {} @ {} | otm {:>6.2}% | iv {:>7.4}",
            put.option_symbol,
            put.strike_price,
            put.otm_percent * 100.0,
            put.implied_volatility,
        );
    }
    if let Some(call) = &view.otm_call {
        println!(
            "otm call        : {} @ {} | otm {:>6.2}% | iv {:>7.4}",
            call.option_symbol,
            call.strike_price,
            call.otm_percent * 100.0,
            call.implied_volatility,
        );
    }
    if let Some(skew) = view.otm_skew {
        println!(
            "otm skew        : {:>7.4} | put-atm {:>7.4} | call-atm {:>7.4}",
            skew,
            view.otm_put_wing_vs_atm.unwrap_or(0.0),
            view.otm_call_wing_vs_atm.unwrap_or(0.0),
        );
    }

    Ok(())
}
