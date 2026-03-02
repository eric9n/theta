use anyhow::Result;
use clap::Parser;
use theta::signal_service::{TermStructureRequest, ThetaSignalService};

#[derive(Parser, Debug)]
#[command(name = "term-structure")]
#[command(about = "CLI for option term structure signals")]
struct Cli {
    #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
    symbol: String,
    #[arg(long, default_value_t = 4, help = "Number of upcoming expiries to analyze")]
    expiries_limit: usize,
    #[arg(long, help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping")]
    rate: Option<f64>,
    #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
    dividend: f64,
    #[arg(long, help = "Manual annualized implied volatility override for all contracts")]
    iv: Option<f64>,
    #[arg(long, help = "Solve IV from each option's provider last_done price")]
    iv_from_market_price: bool,
    #[arg(long)]
    json: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let service = ThetaSignalService::from_env().await?;
    let view = service
        .term_structure(TermStructureRequest {
            symbol: cli.symbol,
            expiries_limit: cli.expiries_limit,
            rate: cli.rate,
            dividend: cli.dividend,
            iv: cli.iv,
            iv_from_market_price: cli.iv_from_market_price,
        })
        .await?;

    render_term_structure(&view, cli.json)
}

fn render_term_structure(view: &theta::domain::TermStructureView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!("underlying      : {}", view.underlying_symbol);
    println!("target expiries : {}", view.target_expiries);
    println!("points          : {}", view.points.len());

    for point in &view.points {
        println!(
            "{} ({:>3}d) | atm {} | call {:>7} | put {:>7} | atm_iv {:>7.4} | d_prev {:>7} | d_front {:>7}",
            point.expiry,
            point.days_to_expiry,
            point.atm_strike_price,
            point
                .atm_call_iv
                .map(|v| format!("{v:>7.4}"))
                .unwrap_or_else(|| "   n/a ".to_string()),
            point
                .atm_put_iv
                .map(|v| format!("{v:>7.4}"))
                .unwrap_or_else(|| "   n/a ".to_string()),
            point.atm_iv,
            point
                .iv_change_from_prev
                .map(|v| format!("{v:>7.4}"))
                .unwrap_or_else(|| "   n/a ".to_string()),
            point
                .iv_change_from_front
                .map(|v| format!("{v:>7.4}"))
                .unwrap_or_else(|| "   n/a ".to_string()),
        );
    }

    Ok(())
}
