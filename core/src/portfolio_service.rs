use crate::analysis_service::ThetaAnalysisService;
use crate::analytics::{
    ContractSide, PricingInput, calculate_metrics, implied_volatility_from_price,
};
use crate::ledger::Position;
use crate::market_data::{OptionQuote, days_to_expiry, decimal_to_f64};
use crate::rate::RateCurve;
use crate::risk_domain::EnrichedPosition;
use anyhow::{Context, Result, bail};

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
        .context("failed to fetch underlying quotes")?;

    // Batch call 2: all option prices
    let option_quotes_vec = market
        .batch_option_quote(&option_lp_syms)
        .await
        .context("failed to fetch option quotes")?;

    // Build option price map: symbol (no .US) → last_done price
    let option_price_map = build_option_price_map(&option_quotes_vec)?;

    ensure_complete_quote_coverage(positions, &underlying_prices, &option_price_map)?;

    // Enrich each position
    let mut enriched = Vec::with_capacity(positions.len());
    for pos in positions {
        let ep = if pos.side == "stock" {
            enrich_stock_from_map(pos, &underlying_prices)
        } else {
            enrich_option_from_map(
                pos,
                &option_price_map,
                &underlying_prices,
                resolve_option_rate(
                    service.rate_curve(),
                    days_to_expiry_from_position(pos).unwrap_or(0),
                ),
                service.dividend_yield_for_symbol(&pos.underlying),
            )
        };
        enriched.push(ep);
    }

    Ok(enriched)
}

fn ensure_complete_quote_coverage(
    positions: &[Position],
    underlying_prices: &std::collections::HashMap<String, f64>,
    option_price_map: &std::collections::HashMap<String, f64>,
) -> Result<()> {
    let mut missing_underlyings = std::collections::BTreeSet::new();
    let mut missing_option_quotes = std::collections::BTreeSet::new();

    for pos in positions {
        if !underlying_prices.contains_key(quote_key(&pos.underlying)) {
            missing_underlyings.insert(pos.underlying.clone());
        }
        if pos.side != "stock" && !option_price_map.contains_key(quote_key(&pos.symbol)) {
            missing_option_quotes.insert(pos.symbol.clone());
        }
    }

    if missing_underlyings.is_empty() && missing_option_quotes.is_empty() {
        return Ok(());
    }

    let mut parts = Vec::new();
    if !missing_underlyings.is_empty() {
        parts.push(format!(
            "missing underlying quotes for {}",
            missing_underlyings
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !missing_option_quotes.is_empty() {
        parts.push(format!(
            "missing option quotes for {}",
            missing_option_quotes
                .into_iter()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    bail!("incomplete live quote coverage: {}", parts.join("; "))
}

fn build_option_price_map(
    option_quotes: &[OptionQuote],
) -> Result<std::collections::HashMap<String, f64>> {
    let mut option_price_map = std::collections::HashMap::with_capacity(option_quotes.len());
    for oq in option_quotes {
        let sym = oq.symbol.trim_end_matches(".US").to_string();
        let price = decimal_to_f64(&oq.last_done, "last_done")
            .with_context(|| format!("invalid option quote price for {}", oq.symbol))?;
        option_price_map.insert(sym, price);
    }
    Ok(option_price_map)
}

fn lp_sym(s: &str) -> String {
    if s.contains('.') {
        s.to_string()
    } else {
        format!("{}.US", s)
    }
}

fn quote_key(s: &str) -> &str {
    s.trim_end_matches(".US")
}

fn enrich_stock_from_map(
    pos: &Position,
    prices: &std::collections::HashMap<String, f64>,
) -> EnrichedPosition {
    let spot = prices.get(quote_key(&pos.underlying)).copied();
    let current_price = spot.unwrap_or(pos.avg_cost);
    let sign = pos.net_quantity.signum() as f64;
    let pnl_per_unit = (current_price - pos.avg_cost) * sign;
    EnrichedPosition {
        symbol: pos.symbol.clone(),
        underlying: pos.underlying.clone(),
        underlying_spot: spot,
        side: pos.side.clone(),
        strike: pos.strike,
        expiry: pos.expiry.clone(),
        net_quantity: pos.net_quantity,
        avg_cost: pos.avg_cost,
        current_price,
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
    dividend: f64,
) -> EnrichedPosition {
    let option_price = option_map
        .get(quote_key(&pos.symbol))
        .copied()
        .unwrap_or(pos.avg_cost);
    let spot = underlying_prices.get(quote_key(&pos.underlying)).copied();

    let sign = pos.net_quantity.signum() as f64;
    let pnl_per_unit = (option_price - pos.avg_cost) * sign;
    let multiplier = pos.net_quantity.unsigned_abs() as f64 * 100.0;

    // Compute local Greeks from Black-Scholes if we have enough inputs
    let greeks = pos.expiry.as_deref().and_then(|expiry_str| {
        let expiry = crate::market_data::parse_expiry_date(expiry_str).ok()?;
        let dte = days_to_expiry(expiry) as f64;
        let strike = pos.strike?;
        let spot = spot?;
        if spot <= 0.0 || option_price <= 0.0 {
            return None;
        }
        let option_type = if pos.side == "call" {
            ContractSide::Call
        } else {
            ContractSide::Put
        };
        let iv = implied_volatility_from_price(
            spot,
            strike,
            rate,
            dte,
            dividend,
            option_type,
            option_price,
        )
        .ok()?;
        let input = PricingInput::new(spot, strike, rate, iv, dte, dividend, option_type).ok()?;
        Some(calculate_metrics(&input))
    });

    EnrichedPosition {
        symbol: pos.symbol.clone(),
        underlying: pos.underlying.clone(),
        underlying_spot: spot,
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

fn days_to_expiry_from_position(pos: &Position) -> Option<i64> {
    let expiry = crate::market_data::parse_expiry_date(pos.expiry.as_deref()?).ok()?;
    Some(days_to_expiry(expiry))
}

fn resolve_option_rate(rate_curve: RateCurve, days_to_expiry: i64) -> f64 {
    rate_curve.rate_for_days(days_to_expiry)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market_data::{OptionDirection, OptionQuote};
    use crate::rate::RateCurve;

    fn stock_position(symbol: &str) -> Position {
        Position {
            symbol: symbol.to_string(),
            underlying: symbol.to_string(),
            side: "stock".to_string(),
            strike: None,
            expiry: None,
            net_quantity: 10,
            avg_cost: 100.0,
            total_cost: 1_000.0,
        }
    }

    fn option_position(symbol: &str, underlying: &str) -> Position {
        Position {
            symbol: symbol.to_string(),
            underlying: underlying.to_string(),
            side: "call".to_string(),
            strike: Some(400.0),
            expiry: Some("2026-03-20".to_string()),
            net_quantity: 1,
            avg_cost: 5.0,
            total_cost: 5.0,
        }
    }

    #[test]
    fn rejects_missing_underlying_quotes() {
        let positions = vec![stock_position("TSLA")];
        let err = ensure_complete_quote_coverage(
            &positions,
            &std::collections::HashMap::new(),
            &std::collections::HashMap::new(),
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("missing underlying quotes for TSLA")
        );
    }

    #[test]
    fn rejects_missing_option_quotes() {
        let positions = vec![option_position("TSLA260320C00400000", "TSLA")];
        let underlying_prices = std::collections::HashMap::from([("TSLA".to_string(), 410.0)]);
        let err =
            ensure_complete_quote_coverage(&positions, &underlying_prices, &Default::default())
                .unwrap_err();

        assert!(
            err.to_string()
                .contains("missing option quotes for TSLA260320C00400000")
        );
    }

    #[test]
    fn accepts_complete_quote_coverage() {
        let positions = vec![
            stock_position("TSLA"),
            option_position("TSLA260320C00400000", "TSLA"),
        ];
        let underlying_prices = std::collections::HashMap::from([("TSLA".to_string(), 410.0)]);
        let option_quotes =
            std::collections::HashMap::from([("TSLA260320C00400000".to_string(), 12.5)]);

        ensure_complete_quote_coverage(&positions, &underlying_prices, &option_quotes).unwrap();
    }

    #[test]
    fn build_option_price_map_rejects_non_finite_prices() {
        let quotes = vec![OptionQuote {
            symbol: "TSLA260320C00400000.US".to_string(),
            underlying_symbol: "TSLA.US".to_string(),
            direction: OptionDirection::Call,
            last_done: "NaN".to_string(),
            prev_close: "5.0".to_string(),
            open: "5.0".to_string(),
            high: "5.0".to_string(),
            low: "5.0".to_string(),
            volume: 1,
            turnover: "1".to_string(),
            timestamp: "2026-03-07T09:30:00Z".to_string(),
            trade_status: serde_json::json!("Normal"),
            strike_price: "400".to_string(),
            expiry_date: time::macros::date!(2026 - 03 - 20),
            implied_volatility: "0.4".to_string(),
            open_interest: 1,
            historical_volatility: "0.3".to_string(),
            contract_multiplier: "100".to_string(),
            contract_size: "100".to_string(),
            contract_type: serde_json::json!("American"),
        }];

        let err = build_option_price_map(&quotes).unwrap_err();
        assert!(
            err.to_string()
                .contains("invalid option quote price for TSLA260320C00400000.US")
        );
    }

    #[test]
    fn resolve_option_rate_uses_curve_buckets() {
        let curve = RateCurve {
            short_rate: 0.03,
            medium_rate: 0.04,
            long_rate: 0.05,
        };

        assert_eq!(resolve_option_rate(curve, 30), 0.03);
        assert_eq!(resolve_option_rate(curve, 120), 0.04);
        assert_eq!(resolve_option_rate(curve, 240), 0.05);
    }

    #[test]
    fn option_enrichment_uses_configured_dividend_yield() {
        let position = Position {
            symbol: "SPY261218C00400000".to_string(),
            underlying: "SPY".to_string(),
            side: "call".to_string(),
            strike: Some(400.0),
            expiry: Some("2026-12-18".to_string()),
            net_quantity: 1,
            avg_cost: 20.0,
            total_cost: 20.0,
        };
        let option_quotes =
            std::collections::HashMap::from([("SPY261218C00400000".to_string(), 24.0)]);
        let underlying_prices = std::collections::HashMap::from([("SPY".to_string(), 410.0)]);

        let zero_dividend =
            enrich_option_from_map(&position, &option_quotes, &underlying_prices, 0.04, 0.0);
        let configured_dividend =
            enrich_option_from_map(&position, &option_quotes, &underlying_prices, 0.04, 0.02);

        let zero_delta = zero_dividend.greeks.expect("greeks").delta;
        let configured_delta = configured_dividend.greeks.expect("greeks").delta;
        assert!(configured_delta < zero_delta);
    }
}
