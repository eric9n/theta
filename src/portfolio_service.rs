use anyhow::{Context, Result};
use crate::analysis_service::{AnalyzeOptionRequest, ThetaAnalysisService};
use crate::ledger::Position;
use crate::risk_domain::EnrichedPosition;

/// Enrich positions with live market data and locally computed Greeks.
///
/// All LongPort API calls are fired concurrently via `join_all` to minimise
/// total latency.
pub async fn enrich_positions(
    service: &ThetaAnalysisService,
    positions: &[Position],
) -> Result<Vec<EnrichedPosition>> {
    // Build a futures vec — each future enriches one position independently.
    let futures: Vec<_> = positions
        .iter()
        .map(|pos| {
            let service = service;
            async move {
                if pos.side == "stock" {
                    enrich_stock_no_cache(service, pos).await
                } else {
                    enrich_option(service, pos, &mut Default::default()).await
                }
            }
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let enriched = results
        .into_iter()
        .zip(positions.iter())
        .map(|(result, pos)| match result {
            Ok(ep) => ep,
            Err(e) => {
                eprintln!(
                    "Warning: failed to enrich position {}: {}. Using offline data.",
                    pos.symbol, e
                );
                fallback_enriched(pos)
            }
        })
        .collect();

    Ok(enriched)
}

async fn enrich_stock_no_cache(
    service: &ThetaAnalysisService,
    pos: &Position,
) -> Result<EnrichedPosition> {
    let mut cache = std::collections::HashMap::new();
    enrich_stock(service, pos, &mut cache).await
}

async fn enrich_stock(
    service: &ThetaAnalysisService,
    pos: &Position,
    cache: &mut std::collections::HashMap<String, f64>,
) -> Result<EnrichedPosition> {
    let spot = fetch_spot_cached(service, &pos.underlying, cache).await?;
    let pnl_per_unit = spot - pos.avg_cost;
    let multiplier = pos.net_quantity as f64;

    Ok(EnrichedPosition {
        symbol: pos.symbol.clone(),
        underlying: pos.underlying.clone(),
        underlying_spot: Some(spot),
        side: pos.side.clone(),
        strike: pos.strike,
        expiry: pos.expiry.clone(),
        net_quantity: pos.net_quantity,
        avg_cost: pos.avg_cost,
        current_price: spot,
        unrealized_pnl_per_unit: pnl_per_unit,
        unrealized_pnl: pnl_per_unit * multiplier,
        greeks: None,
    })
}

async fn enrich_option(
    service: &ThetaAnalysisService,
    pos: &Position,
    cache: &mut std::collections::HashMap<String, f64>,
) -> Result<EnrichedPosition> {
    // Build the LongPort option symbol: append .US if not already present
    let lp_symbol = if pos.symbol.contains('.') {
        pos.symbol.clone()
    } else {
        format!("{}.US", pos.symbol)
    };

    let analysis = service
        .analyze_option(AnalyzeOptionRequest {
            symbol: lp_symbol.clone(),
            rate: None,
            dividend: 0.0,
            iv: None,
            iv_from_option_price: None,
            iv_from_market_price: true,
            show_iv_diff: false,
            use_provider_greeks: false,
        })
        .await
        .with_context(|| format!("failed to analyze option {}", lp_symbol))?;

    let option_price: f64 = analysis
        .option_price
        .parse()
        .unwrap_or(pos.avg_cost);

    let underlying_price: f64 = analysis
        .underlying_price
        .parse()
        .unwrap_or(0.0);

    // Cache the underlying price
    cache.insert(pos.underlying.clone(), underlying_price);

    let sign = pos.net_quantity.signum() as f64;
    let pnl_per_unit = (option_price - pos.avg_cost) * sign;
    let multiplier = pos.net_quantity.unsigned_abs() as f64 * 100.0;

    Ok(EnrichedPosition {
        symbol: pos.symbol.clone(),
        underlying: pos.underlying.clone(),
        underlying_spot: Some(underlying_price),
        side: pos.side.clone(),
        strike: pos.strike,
        expiry: pos.expiry.clone(),
        net_quantity: pos.net_quantity,
        avg_cost: pos.avg_cost,
        current_price: option_price,
        unrealized_pnl_per_unit: pnl_per_unit,
        unrealized_pnl: pnl_per_unit * multiplier,
        greeks: Some(analysis.local_greeks),
    })
}

async fn fetch_spot_cached(
    service: &ThetaAnalysisService,
    symbol: &str,
    cache: &mut std::collections::HashMap<String, f64>,
) -> Result<f64> {
    if let Some(&price) = cache.get(symbol) {
        return Ok(price);
    }

    // Append .US if needed
    let lp_symbol = if symbol.contains('.') {
        symbol.to_string()
    } else {
        format!("{}.US", symbol)
    };

    let underlying = service.market().fetch_underlying(&lp_symbol).await?;
    let price = underlying.last_done_f64;
    cache.insert(symbol.to_string(), price);
    Ok(price)
}

/// Fallback when live data is unavailable — use cost basis, no Greeks
fn fallback_enriched(pos: &Position) -> EnrichedPosition {
    EnrichedPosition {
        symbol: pos.symbol.clone(),
        underlying: pos.underlying.clone(),
        underlying_spot: None,
        side: pos.side.clone(),
        strike: pos.strike,
        expiry: pos.expiry.clone(),
        net_quantity: pos.net_quantity,
        avg_cost: pos.avg_cost,
        current_price: pos.avg_cost,
        unrealized_pnl_per_unit: 0.0,
        unrealized_pnl: 0.0,
        greeks: None,
    }
}
