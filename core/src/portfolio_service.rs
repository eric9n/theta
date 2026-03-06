use anyhow::Result;
use crate::analysis_service::ThetaAnalysisService;
use crate::analytics::{ContractSide, PricingInput, calculate_metrics, implied_volatility_from_price};
use crate::ledger::Position;
use crate::market_data::days_to_expiry;
use crate::risk_domain::EnrichedPosition;

/// Enrich positions with live market data and locally computed Greeks.
///
/// Uses 2 batch API calls regardless of portfolio size:
/// 1. One call for all unique underlying stock prices.
/// 2. One call for all option quotes.
pub async fn enrich_positions(
    service: &ThetaAnalysisService,
    positions: &[Position],
) -> Result<Vec<EnrichedPosition>> {
    if positions.is_empty() {
        return Ok(vec![]);
    }

    let market = service.market();

    // Collect unique underlying symbols and option symbols
    let underlying_syms: Vec<String> = {
        let mut set = std::collections::HashSet::new();
        for pos in positions {
            set.insert(lp_sym(&pos.underlying));
        }
        set.into_iter().collect()
    };
    let option_lp_syms: Vec<String> = positions
        .iter()
        .filter(|p| p.side != "stock")
        .map(|p| lp_sym(&p.symbol))
        .collect();

    // Batch call 1: all underlying prices
    let underlying_prices = market
        .batch_quote(&underlying_syms)
        .await
        .unwrap_or_default();

    // Batch call 2: all option prices
    let option_quotes_vec = market
        .batch_option_quote(&option_lp_syms)
        .await
        .unwrap_or_default();

    // Build option price map: symbol (no .US) → last_done price
    let mut option_price_map: std::collections::HashMap<String, f64> =
        std::collections::HashMap::new();
    for oq in &option_quotes_vec {
        let sym = oq.symbol.trim_end_matches(".US").to_string();
        let price: f64 = oq.last_done.to_string().parse().unwrap_or(0.0);
        option_price_map.insert(sym, price);
    }

    // Enrich each position
    let rate = 0.045_f64;
    let mut enriched = Vec::with_capacity(positions.len());
    for pos in positions {
        let ep = if pos.side == "stock" {
            enrich_stock_from_map(pos, &underlying_prices)
        } else {
            enrich_option_from_map(pos, &option_price_map, &underlying_prices, rate)
        };
        enriched.push(ep);
    }

    Ok(enriched)
}

fn lp_sym(s: &str) -> String {
    if s.contains('.') { s.to_string() } else { format!("{}.US", s) }
}

fn quote_key(s: &str) -> &str {
    s.trim_end_matches(".US")
}

fn enrich_stock_from_map(
    pos: &Position,
    prices: &std::collections::HashMap<String, f64>,
) -> EnrichedPosition {
    let spot = prices
        .get(quote_key(&pos.underlying))
        .copied()
        .unwrap_or(pos.avg_cost);
    let sign = pos.net_quantity.signum() as f64;
    let pnl_per_unit = (spot - pos.avg_cost) * sign;
    EnrichedPosition {
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
        unrealized_pnl: pnl_per_unit * pos.net_quantity.unsigned_abs() as f64,
        greeks: None,
    }
}

fn enrich_option_from_map(
    pos: &Position,
    option_map: &std::collections::HashMap<String, f64>,
    underlying_prices: &std::collections::HashMap<String, f64>,
    rate: f64,
) -> EnrichedPosition {
    let option_price = option_map
        .get(quote_key(&pos.symbol))
        .copied()
        .unwrap_or(pos.avg_cost);
    let spot = underlying_prices
        .get(quote_key(&pos.underlying))
        .copied()
        .unwrap_or(0.0);

    let sign = pos.net_quantity.signum() as f64;
    let pnl_per_unit = (option_price - pos.avg_cost) * sign;
    let multiplier = pos.net_quantity.unsigned_abs() as f64 * 100.0;

    // Compute local Greeks from Black-Scholes if we have enough inputs
    let greeks = pos.expiry.as_deref().and_then(|expiry_str| {
        let expiry = crate::market_data::parse_expiry_date(expiry_str).ok()?;
        let dte = days_to_expiry(expiry) as f64;
        let strike = pos.strike?;
        if spot <= 0.0 || option_price <= 0.0 { return None; }
        let option_type = if pos.side == "call" { ContractSide::Call } else { ContractSide::Put };
        let iv = implied_volatility_from_price(spot, strike, rate, dte, 0.0, option_type, option_price).ok()?;
        let input = PricingInput::new(spot, strike, rate, iv, dte, 0.0, option_type).ok()?;
        Some(calculate_metrics(&input))
    });

    EnrichedPosition {
        symbol: pos.symbol.clone(),
        underlying: pos.underlying.clone(),
        underlying_spot: Some(spot),
        side: pos.side.clone(),
        strike: pos.strike,
        expiry: pos.expiry.clone(),
        net_quantity: pos.net_quantity,
        avg_cost: pos.avg_cost,
        current_price: option_price,
        unrealized_pnl_per_unit: pnl_per_unit,
        unrealized_pnl: pnl_per_unit * multiplier,
        greeks,
    }
}

/// Fallback when live data is unavailable — use cost basis, no Greeks
#[allow(dead_code)]
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
