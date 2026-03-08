use crate::market_data::OptionChainFetchFilter;
use crate::signal_service::ThetaSignalService;
use anyhow::{Context, Result, bail};
use clap::Args;
use std::time::Instant;

#[derive(Args, Debug)]
#[command(name = "health-check")]
#[command(about = "Run a lightweight live health check against theta-daemon and option data APIs")]
pub struct Cli {
    #[arg(long, default_value = "TSLA.US", help = "Underlying symbol to probe")]
    symbol: String,
    #[arg(
        long,
        default_value_t = 0.05,
        help = "Max OTM percent for the sampled option chain slice"
    )]
    max_otm_percent: f64,
    #[arg(
        long,
        default_value_t = 2,
        help = "Minimum number of option contracts expected in the sampled chain"
    )]
    min_contracts: usize,
}

pub async fn run(cli: Cli) -> Result<()> {
    if !cli.max_otm_percent.is_finite() || cli.max_otm_percent < 0.0 {
        bail!("--max-otm-percent must be a finite non-negative number");
    }
    if cli.min_contracts == 0 {
        bail!("--min-contracts must be greater than zero");
    }

    let started = Instant::now();
    let service = ThetaSignalService::from_env().await?;
    let market = service.analysis().market();
    let symbol = cli.symbol.trim().to_uppercase();

    if symbol.is_empty() {
        bail!("--symbol must not be empty");
    }

    let underlying = market
        .fetch_underlying(&symbol)
        .await
        .with_context(|| format!("failed to fetch underlying quote for {symbol}"))?;
    if !underlying.last_done_f64.is_finite() || underlying.last_done_f64 <= 0.0 {
        bail!(
            "underlying quote for {symbol} returned invalid last_done {}",
            underlying.last_done
        );
    }

    let front_expiry = service
        .front_expiry_for_symbol(&symbol)
        .await
        .with_context(|| format!("failed to resolve front expiry for {symbol}"))?;

    let chain = market
        .fetch_option_chain_filtered(
            &symbol,
            front_expiry,
            OptionChainFetchFilter {
                min_otm_percent: Some(0.0),
                max_otm_percent: Some(cli.max_otm_percent),
                ..Default::default()
            },
        )
        .await
        .with_context(|| format!("failed to fetch sampled option chain for {symbol}"))?;

    if chain.contracts.len() < cli.min_contracts {
        bail!(
            "sampled chain returned {} contracts for {} {} (expected at least {})",
            chain.contracts.len(),
            symbol,
            chain.expiry,
            cli.min_contracts
        );
    }

    println!(
        "ok symbol={} underlying={} front_expiry={} contracts={} elapsed_ms={}",
        symbol,
        underlying.last_done,
        chain.expiry,
        chain.contracts.len(),
        started.elapsed().as_millis()
    );

    Ok(())
}
