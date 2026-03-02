use crate::ledger::Position;
use crate::risk_domain::*;

// ---------------------------------------------------------------------------
// Strategy identification — Firstrade-compatible
// ---------------------------------------------------------------------------

/// Identify option strategies from a list of positions.
/// Greedy matching: complex strategies first, then simple ones.
/// Modifies a working copy — matched legs are consumed.
pub fn identify_strategies(positions: &[Position]) -> Vec<IdentifiedStrategy> {
    let mut strategies = Vec::new();

    // Group by underlying
    let mut by_underlying: std::collections::HashMap<String, Vec<Position>> =
        std::collections::HashMap::new();
    for p in positions {
        by_underlying
            .entry(p.underlying.clone())
            .or_default()
            .push(p.clone());
    }

    for (underlying, mut legs) in by_underlying {
        // 1. Iron Condor (bear call spread + bull put spread)
        identify_iron_condors(&underlying, &mut legs, &mut strategies);
        // 2. Butterfly
        identify_butterflies(&underlying, &mut legs, &mut strategies);
        // 3. Vertical spreads
        identify_vertical_spreads(&underlying, &mut legs, &mut strategies);
        // 4. Covered Call (stock + short call)
        identify_covered_calls(&underlying, &mut legs, &mut strategies);
        // 5. Straddle / Strangle
        identify_straddles_strangles(&underlying, &mut legs, &mut strategies);
        // 6. Remaining single-leg options
        identify_single_legs(&underlying, &mut legs, &mut strategies);
        // 7. Remaining stock
        identify_unmatched_stock(&underlying, &mut legs, &mut strategies);
    }

    strategies
}

// ---------------------------------------------------------------------------
// Iron Condor: bear call spread + bull put spread, same expiry
// ---------------------------------------------------------------------------

fn identify_iron_condors(
    underlying: &str,
    legs: &mut Vec<Position>,
    out: &mut Vec<IdentifiedStrategy>,
) {
    // Need: short call (lower) + long call (higher) + short put (higher) + long put (lower)
    // All same expiry, same quantity magnitude
    let call_indices: Vec<usize> = legs
        .iter()
        .enumerate()
        .filter(|(_, p)| p.side == "call")
        .map(|(i, _)| i)
        .collect();
    let put_indices: Vec<usize> = legs
        .iter()
        .enumerate()
        .filter(|(_, p)| p.side == "put")
        .map(|(i, _)| i)
        .collect();

    if call_indices.len() < 2 || put_indices.len() < 2 {
        return;
    }

    let mut consumed = Vec::new();
    // Try to find bear call spread + bull put spread pairs
    for &ci1 in &call_indices {
        if consumed.contains(&ci1) {
            continue;
        }
        for &ci2 in &call_indices {
            if ci2 == ci1 || consumed.contains(&ci2) {
                continue;
            }
            let c1 = &legs[ci1];
            let c2 = &legs[ci2];
            // Bear call: short lower strike call, long higher strike call
            let (short_call_idx, long_call_idx) = if c1.net_quantity < 0
                && c2.net_quantity > 0
                && c1.strike < c2.strike
                && c1.expiry == c2.expiry
                && c1.net_quantity.unsigned_abs() == c2.net_quantity.unsigned_abs()
            {
                (ci1, ci2)
            } else if c2.net_quantity < 0
                && c1.net_quantity > 0
                && c2.strike < c1.strike
                && c1.expiry == c2.expiry
                && c1.net_quantity.unsigned_abs() == c2.net_quantity.unsigned_abs()
            {
                (ci2, ci1)
            } else {
                continue;
            };

            let expiry = legs[short_call_idx].expiry.clone();
            let qty = legs[short_call_idx].net_quantity.unsigned_abs() as i64;

            // Find matching bull put spread in same expiry
            for &pi1 in &put_indices {
                if consumed.contains(&pi1) {
                    continue;
                }
                for &pi2 in &put_indices {
                    if pi2 == pi1 || consumed.contains(&pi2) {
                        continue;
                    }
                    let p1 = &legs[pi1];
                    let p2 = &legs[pi2];
                    // Bull put: short higher strike put, long lower strike put
                    let (short_put_idx, long_put_idx) = if p1.net_quantity < 0
                        && p2.net_quantity > 0
                        && p1.strike > p2.strike
                        && p1.expiry == expiry
                        && p2.expiry == expiry
                        && p1.net_quantity.unsigned_abs() == qty as u64
                        && p2.net_quantity.unsigned_abs() == qty as u64
                    {
                        (pi1, pi2)
                    } else if p2.net_quantity < 0
                        && p1.net_quantity > 0
                        && p2.strike > p1.strike
                        && p1.expiry == expiry
                        && p2.expiry == expiry
                        && p2.net_quantity.unsigned_abs() == qty as u64
                        && p1.net_quantity.unsigned_abs() == qty as u64
                    {
                        (pi2, pi1)
                    } else {
                        continue;
                    };

                    let sc = &legs[short_call_idx];
                    let lc = &legs[long_call_idx];
                    let sp = &legs[short_put_idx];
                    let lp = &legs[long_put_idx];

                    let call_width = (lc.strike.unwrap_or(0.0) - sc.strike.unwrap_or(0.0)).abs();
                    let put_width = (sp.strike.unwrap_or(0.0) - lp.strike.unwrap_or(0.0)).abs();
                    let gross_call_spread_risk = call_width * qty as f64 * 100.0;
                    let gross_put_spread_risk = put_width * qty as f64 * 100.0;
                    let total_credit =
                        spread_credit_total(sc, lc) + spread_credit_total(sp, lp);
                    let max_side_risk = gross_call_spread_risk.max(gross_put_spread_risk);
                    let margin = (max_side_risk - total_credit).max(0.0);

                    out.push(IdentifiedStrategy {
                        kind: StrategyKind::IronCondor,
                        underlying: underlying.to_string(),
                        legs: vec![
                            pos_to_leg(sc),
                            pos_to_leg(lc),
                            pos_to_leg(sp),
                            pos_to_leg(lp),
                        ],
                        margin: StrategyMargin {
                            margin_required: margin,
                            method: format!(
                                "max_side_risk({:.2}) - total_credit({:.2}) = {:.2}",
                                max_side_risk, total_credit, margin
                            ),
                        },
                        max_profit: None,
                        max_loss: Some(margin),
                        breakeven: vec![],
                    });

                    consumed.extend([short_call_idx, long_call_idx, short_put_idx, long_put_idx]);
                    break;
                }
                if consumed.contains(&pi1) {
                    break; // already matched
                }
            }
        }
    }

    remove_consumed(legs, &consumed);
}

// ---------------------------------------------------------------------------
// Butterfly: 3 strikes, same side, same expiry
// e.g. long 1 × K1, short 2 × K2, long 1 × K3
// ---------------------------------------------------------------------------

fn identify_butterflies(
    underlying: &str,
    legs: &mut Vec<Position>,
    out: &mut Vec<IdentifiedStrategy>,
) {
    for side_str in &["call", "put"] {
        let mut indices: Vec<usize> = legs
            .iter()
            .enumerate()
            .filter(|(_, p)| p.side == *side_str && p.expiry.is_some())
            .map(|(i, _)| i)
            .collect();

        if indices.len() < 3 {
            continue;
        }

        // Sort by strike
        indices.sort_by(|&a, &b| {
            legs[a]
                .strike
                .partial_cmp(&legs[b].strike)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut consumed = Vec::new();
        let mut i = 0;
        while i + 2 < indices.len() {
            let a = indices[i];
            let b = indices[i + 1];
            let c = indices[i + 2];

            if consumed.contains(&a) || consumed.contains(&b) || consumed.contains(&c) {
                i += 1;
                continue;
            }

            let la = &legs[a];
            let lb = &legs[b];
            let lc = &legs[c];

            // Same expiry
            if la.expiry != lb.expiry || lb.expiry != lc.expiry {
                i += 1;
                continue;
            }

            // Pattern: long 1 + short 2 + long 1
            let is_long_butterfly = la.net_quantity > 0
                && lb.net_quantity < 0
                && lc.net_quantity > 0
                && lb.net_quantity.unsigned_abs() == 2 * la.net_quantity.unsigned_abs()
                && la.net_quantity == lc.net_quantity;

            if is_long_butterfly {
                out.push(IdentifiedStrategy {
                    kind: StrategyKind::Butterfly,
                    underlying: underlying.to_string(),
                    legs: vec![pos_to_leg(la), pos_to_leg(lb), pos_to_leg(lc)],
                    margin: StrategyMargin {
                        margin_required: 0.0, // long butterfly has no margin (net debit)
                        method: "Debit strategy, no margin required".to_string(),
                    },
                    max_profit: None,
                    max_loss: None,
                    breakeven: vec![],
                });
                consumed.extend([a, b, c]);
                i += 3;
            } else {
                i += 1;
            }
        }
        remove_consumed(legs, &consumed);
    }
}

// ---------------------------------------------------------------------------
// Vertical spreads: bull put, bear call, bull call, bear put
// ---------------------------------------------------------------------------

fn identify_vertical_spreads(
    underlying: &str,
    legs: &mut Vec<Position>,
    out: &mut Vec<IdentifiedStrategy>,
) {
    let mut consumed = Vec::new();

    for side_str in &["put", "call"] {
        let indices: Vec<usize> = legs
            .iter()
            .enumerate()
            .filter(|(_, p)| p.side == *side_str && !consumed.contains(&p.symbol.as_str().len()))
            .map(|(i, _)| i)
            .collect();

        // Find short + long pairs at different strikes, same expiry
        for &i in &indices {
            if consumed.contains(&i) {
                continue;
            }
            for &j in &indices {
                if j == i || consumed.contains(&j) {
                    continue;
                }

                let a = &legs[i];
                let b = &legs[j];

                if a.expiry != b.expiry {
                    continue;
                }

                let a_strike = a.strike.unwrap_or(0.0);
                let b_strike = b.strike.unwrap_or(0.0);

                if (a_strike - b_strike).abs() < 0.01 {
                    continue;
                }

                if a.net_quantity.unsigned_abs() != b.net_quantity.unsigned_abs() {
                    continue;
                }

                // One short, one long
                if a.net_quantity.signum() == b.net_quantity.signum() {
                    continue;
                }

                let qty = a.net_quantity.unsigned_abs() as f64;
                let width = (a_strike - b_strike).abs();
                let gross_spread_risk = width * qty * 100.0;

                let (short, long) = if a.net_quantity < 0 { (a, b) } else { (b, a) };
                let short_strike = short.strike.unwrap_or(0.0);
                let long_strike = long.strike.unwrap_or(0.0);

                let kind = match *side_str {
                    "put" if short_strike > long_strike => StrategyKind::BullPutSpread,
                    "put" if short_strike < long_strike => StrategyKind::BearPutSpread,
                    "call" if short_strike < long_strike => StrategyKind::BearCallSpread,
                    "call" if short_strike > long_strike => StrategyKind::BullCallSpread,
                    _ => continue,
                };

                let net_credit_total = spread_credit_total(short, long);
                let net_debit_total = spread_debit_total(short, long);
                let (margin_required, max_loss, margin_method) = match kind {
                    StrategyKind::BullPutSpread | StrategyKind::BearCallSpread => {
                        let max_loss = (gross_spread_risk - net_credit_total).max(0.0);
                        (
                            max_loss,
                            Some(max_loss),
                            format!(
                                "gross_width_risk({:.2}) - net_credit({:.2}) = {:.2}",
                                gross_spread_risk, net_credit_total, max_loss
                            ),
                        )
                    }
                    _ => (
                        0.0,
                        Some(net_debit_total.max(0.0)),
                        "Debit spread, no margin required".to_string(),
                    ),
                };

                out.push(IdentifiedStrategy {
                    kind,
                    underlying: underlying.to_string(),
                    legs: vec![pos_to_leg(short), pos_to_leg(long)],
                    margin: StrategyMargin {
                        margin_required,
                        method: margin_method,
                    },
                    max_profit: None,
                    max_loss,
                    breakeven: vec![],
                });

                consumed.extend([i, j]);
                break;
            }
        }
    }

    remove_consumed(legs, &consumed);
}

// ---------------------------------------------------------------------------
// Covered Call: long stock + short call
// ---------------------------------------------------------------------------

fn identify_covered_calls(
    underlying: &str,
    legs: &mut Vec<Position>,
    out: &mut Vec<IdentifiedStrategy>,
) {
    let stock_idx = legs.iter().position(|p| p.side == "stock" && p.net_quantity > 0);
    if stock_idx.is_none() {
        return;
    }
    let stock_idx = stock_idx.unwrap();

    let mut consumed = Vec::new();
    let stock = &legs[stock_idx];
    let mut remaining_shares = stock.net_quantity;

    // Find short calls coverable by shares (100 shares per contract)
    let short_call_indices: Vec<usize> = legs
        .iter()
        .enumerate()
        .filter(|(i, p)| *i != stock_idx && p.side == "call" && p.net_quantity < 0)
        .map(|(i, _)| i)
        .collect();

    for &ci in &short_call_indices {
        let call = &legs[ci];
        let contracts = call.net_quantity.unsigned_abs() as i64;
        let shares_needed = contracts * 100;

        if remaining_shares >= shares_needed {
            out.push(IdentifiedStrategy {
                kind: StrategyKind::CoveredCall,
                underlying: underlying.to_string(),
                legs: vec![
                    StrategyLeg {
                        symbol: stock.symbol.clone(),
                        side: "stock".to_string(),
                        strike: None,
                        expiry: None,
                        quantity: shares_needed,
                        price: stock.avg_cost,
                    },
                    pos_to_leg(call),
                ],
                margin: StrategyMargin {
                    margin_required: 0.0,
                    method: "Covered by stock position".to_string(),
                },
                max_profit: None,
                max_loss: None,
                breakeven: vec![],
            });
            remaining_shares -= shares_needed;
            consumed.push(ci);
        }
    }

    // If all stock was consumed for covered calls
    if remaining_shares == 0 {
        consumed.push(stock_idx);
    } else {
        // Update stock quantity only
        legs[stock_idx].net_quantity = remaining_shares;
    }

    remove_consumed(legs, &consumed);
}

// ---------------------------------------------------------------------------
// Straddle / Strangle: long/short call + put at same/different strikes
// ---------------------------------------------------------------------------

fn identify_straddles_strangles(
    underlying: &str,
    legs: &mut Vec<Position>,
    out: &mut Vec<IdentifiedStrategy>,
) {
    let mut consumed = Vec::new();

    let call_indices: Vec<usize> = legs
        .iter()
        .enumerate()
        .filter(|(_, p)| p.side == "call")
        .map(|(i, _)| i)
        .collect();
    let put_indices: Vec<usize> = legs
        .iter()
        .enumerate()
        .filter(|(_, p)| p.side == "put")
        .map(|(i, _)| i)
        .collect();

    for &ci in &call_indices {
        if consumed.contains(&ci) {
            continue;
        }
        for &pi in &put_indices {
            if consumed.contains(&pi) {
                continue;
            }

            let call = &legs[ci];
            let put = &legs[pi];

            if call.expiry != put.expiry {
                continue;
            }
            if call.net_quantity.signum() != put.net_quantity.signum() {
                continue;
            }
            if call.net_quantity.unsigned_abs() != put.net_quantity.unsigned_abs() {
                continue;
            }

            let same_strike = (call.strike.unwrap_or(0.0) - put.strike.unwrap_or(0.0)).abs() < 0.01;
            let kind = if same_strike {
                StrategyKind::Straddle
            } else {
                StrategyKind::Strangle
            };

            let is_short = call.net_quantity < 0;
            let qty = call.net_quantity.unsigned_abs();
            let spot_proxy = if same_strike {
                call.strike.unwrap_or(0.0)
            } else {
                (call.strike.unwrap_or(0.0) + put.strike.unwrap_or(0.0)) / 2.0
            };
            let margin = if is_short {
                let call_leg_margin =
                    naked_call_margin(spot_proxy, call.strike.unwrap_or(0.0), qty, call.avg_cost);
                let put_leg_margin =
                    naked_put_margin(spot_proxy, put.strike.unwrap_or(0.0), qty, put.avg_cost);
                let call_premium_total = option_premium_total(call);
                let put_premium_total = option_premium_total(put);
                (call_leg_margin + put_premium_total).max(put_leg_margin + call_premium_total)
            } else {
                0.0
            };

            out.push(IdentifiedStrategy {
                kind,
                underlying: underlying.to_string(),
                legs: vec![pos_to_leg(call), pos_to_leg(put)],
                margin: StrategyMargin {
                    margin_required: margin,
                    method: if is_short {
                        format!(
                            "max(short_call_req + put_premium, short_put_req + call_premium) using spot proxy {:.2}",
                            spot_proxy
                        )
                    } else {
                        "Debit strategy, no margin required".to_string()
                    },
                },
                max_profit: None,
                max_loss: None,
                breakeven: vec![],
            });
            consumed.extend([ci, pi]);
            break;
        }
    }

    remove_consumed(legs, &consumed);
}

// ---------------------------------------------------------------------------
// Single legs: Long Call, Long Put, Naked Put
// ---------------------------------------------------------------------------

fn identify_single_legs(
    underlying: &str,
    legs: &mut Vec<Position>,
    out: &mut Vec<IdentifiedStrategy>,
) {
    let mut consumed = Vec::new();

    for (i, p) in legs.iter().enumerate() {
        if p.side != "call" && p.side != "put" {
            continue;
        }

        let kind = match (p.side.as_str(), p.net_quantity.signum()) {
            ("call", 1) => StrategyKind::LongCall,
            ("put", 1) => StrategyKind::LongPut,
            ("put", -1) => StrategyKind::NakedPut,
            ("call", -1) => StrategyKind::Unmatched, // Naked call — Firstrade doesn't allow, mark as unmatched
            _ => continue,
        };

        let (margin_required, method) = match kind {
            StrategyKind::NakedPut => {
                let strike = p.strike.unwrap_or(0.0);
                let qty = p.net_quantity.unsigned_abs();
                let margin = naked_put_margin(strike, strike, qty, p.avg_cost);
                (
                    margin,
                    format!("Naked put formula using strike {:.2} as spot proxy", strike),
                )
            }
            _ => (0.0, "Debit strategy, no margin required".to_string()),
        };

        out.push(IdentifiedStrategy {
            kind,
            underlying: underlying.to_string(),
            legs: vec![pos_to_leg(p)],
            margin: StrategyMargin {
                margin_required,
                method,
            },
            max_profit: None,
            max_loss: None,
            breakeven: vec![],
        });
        consumed.push(i);
    }

    remove_consumed(legs, &consumed);
}

fn identify_unmatched_stock(
    underlying: &str,
    legs: &mut Vec<Position>,
    out: &mut Vec<IdentifiedStrategy>,
) {
    let mut consumed = Vec::new();
    for (i, p) in legs.iter().enumerate() {
        if p.side == "stock" && p.net_quantity != 0 {
            out.push(IdentifiedStrategy {
                kind: StrategyKind::Unmatched,
                underlying: underlying.to_string(),
                legs: vec![pos_to_leg(p)],
                margin: StrategyMargin {
                    margin_required: 0.0,
                    method: "Stock position".to_string(),
                },
                max_profit: None,
                max_loss: None,
                breakeven: vec![],
            });
            consumed.push(i);
        }
    }
    remove_consumed(legs, &consumed);
}

// ---------------------------------------------------------------------------
// Margin calculation — Firstrade / Apex rules
// ---------------------------------------------------------------------------

/// Calculate naked put margin: Firstrade formula
/// max(20% × spot × qty × 100 − OTM + premium, 10% × spot × qty × 100 + premium)
pub fn naked_put_margin(
    spot: f64,
    strike: f64,
    quantity: u64,
    premium_per_contract: f64,
) -> f64 {
    let qty = quantity as f64;
    let multiplier = qty * 100.0;
    let premium_total = premium_per_contract * multiplier;
    let otm_amount = if spot > strike {
        (spot - strike) * multiplier
    } else {
        0.0
    };

    let formula_a = 0.20 * spot * multiplier - otm_amount + premium_total;
    let formula_b = 0.10 * spot * multiplier + premium_total;

    formula_a.max(formula_b)
}

/// Calculate naked call margin using the call-side mirror of the naked put formula.
pub fn naked_call_margin(
    spot: f64,
    strike: f64,
    quantity: u64,
    premium_per_contract: f64,
) -> f64 {
    let qty = quantity as f64;
    let multiplier = qty * 100.0;
    let premium_total = premium_per_contract * multiplier;
    let otm_amount = if strike > spot {
        (strike - spot) * multiplier
    } else {
        0.0
    };

    let formula_a = 0.20 * spot * multiplier - otm_amount + premium_total;
    let formula_b = 0.10 * spot * multiplier + premium_total;

    formula_a.max(formula_b)
}

// ---------------------------------------------------------------------------
// Portfolio Greeks aggregation
// ---------------------------------------------------------------------------

pub fn aggregate_greeks(positions: &[EnrichedPosition]) -> PortfolioGreeks {
    let mut greeks = PortfolioGreeks::default();

    for p in positions {
        if let Some(ref g) = p.greeks {
            let sign = p.net_quantity.signum() as f64;
            let qty = p.net_quantity.unsigned_abs() as f64;
            let multiplier = if p.side == "stock" { 1.0 } else { 100.0 };

            greeks.net_delta_shares += g.delta * sign * qty * multiplier;
            greeks.total_gamma += g.gamma * sign * qty * multiplier;
            greeks.total_theta_per_day += g.theta_per_day * sign * qty * multiplier;
            greeks.total_vega += g.vega * sign * qty * multiplier;
        }
    }

    greeks
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pos_to_leg(p: &Position) -> StrategyLeg {
    StrategyLeg {
        symbol: p.symbol.clone(),
        side: p.side.clone(),
        strike: p.strike,
        expiry: p.expiry.clone(),
        quantity: p.net_quantity,
        price: p.avg_cost,
    }
}

fn option_premium_total(position: &Position) -> f64 {
    position.avg_cost * position.net_quantity.unsigned_abs() as f64 * 100.0
}

fn spread_credit_total(short: &Position, long: &Position) -> f64 {
    (short.avg_cost - long.avg_cost).max(0.0) * short.net_quantity.unsigned_abs() as f64 * 100.0
}

fn spread_debit_total(short: &Position, long: &Position) -> f64 {
    (long.avg_cost - short.avg_cost).max(0.0) * short.net_quantity.unsigned_abs() as f64 * 100.0
}

fn remove_consumed(legs: &mut Vec<Position>, consumed: &[usize]) {
    if consumed.is_empty() {
        return;
    }
    let mut sorted = consumed.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    for &idx in sorted.iter().rev() {
        legs.remove(idx);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::Position;

    fn stock_pos(underlying: &str, qty: i64, cost: f64) -> Position {
        Position {
            symbol: underlying.to_string(),
            underlying: underlying.to_string(),
            side: "stock".to_string(),
            strike: None,
            expiry: None,
            net_quantity: qty,
            avg_cost: cost,
            total_cost: cost * qty.unsigned_abs() as f64,
        }
    }

    fn option_pos(
        symbol: &str,
        underlying: &str,
        side: &str,
        strike: f64,
        expiry: &str,
        qty: i64,
        cost: f64,
    ) -> Position {
        Position {
            symbol: symbol.to_string(),
            underlying: underlying.to_string(),
            side: side.to_string(),
            strike: Some(strike),
            expiry: Some(expiry.to_string()),
            net_quantity: qty,
            avg_cost: cost,
            total_cost: cost * qty.unsigned_abs() as f64,
        }
    }

    #[test]
    fn identifies_covered_call() {
        let positions = vec![
            stock_pos("TSLA", 200, 350.0),
            option_pos("TSLA260320C00400000", "TSLA", "call", 400.0, "2026-03-20", -2, 5.30),
        ];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::CoveredCall);
        assert_eq!(strategies[0].legs.len(), 2);
    }

    #[test]
    fn identifies_bull_put_spread() {
        let positions = vec![
            option_pos("TSLA260320P00350000", "TSLA", "put", 350.0, "2026-03-20", -1, 8.0),
            option_pos("TSLA260320P00340000", "TSLA", "put", 340.0, "2026-03-20", 1, 5.0),
        ];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::BullPutSpread);
        // Width 10 => 1000 gross risk, net credit = (8 - 5) × 100 = 300, so max loss = 700
        assert!((strategies[0].margin.margin_required - 700.0).abs() < 0.01);
    }

    #[test]
    fn identifies_bear_call_spread() {
        let positions = vec![
            option_pos("TSLA260320C00400000", "TSLA", "call", 400.0, "2026-03-20", -1, 5.0),
            option_pos("TSLA260320C00410000", "TSLA", "call", 410.0, "2026-03-20", 1, 2.0),
        ];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::BearCallSpread);
        assert!((strategies[0].margin.margin_required - 700.0).abs() < 0.01);
    }

    #[test]
    fn identifies_iron_condor() {
        let positions = vec![
            // Bear call spread
            option_pos("TSLA_C400", "TSLA", "call", 400.0, "2026-03-20", -1, 5.0),
            option_pos("TSLA_C410", "TSLA", "call", 410.0, "2026-03-20", 1, 2.0),
            // Bull put spread
            option_pos("TSLA_P350", "TSLA", "put", 350.0, "2026-03-20", -1, 4.0),
            option_pos("TSLA_P340", "TSLA", "put", 340.0, "2026-03-20", 1, 2.0),
        ];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::IronCondor);
        assert_eq!(strategies[0].legs.len(), 4);
        // Max side width = 1000, total credit = 300 + 200 = 500
        assert!((strategies[0].margin.margin_required - 500.0).abs() < 0.01);
    }

    #[test]
    fn identifies_straddle() {
        let positions = vec![
            option_pos("TSLA_C380", "TSLA", "call", 380.0, "2026-03-20", 1, 10.0),
            option_pos("TSLA_P380", "TSLA", "put", 380.0, "2026-03-20", 1, 8.0),
        ];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::Straddle);
    }

    #[test]
    fn identifies_strangle() {
        let positions = vec![
            option_pos("TSLA_C400", "TSLA", "call", 400.0, "2026-03-20", 1, 5.0),
            option_pos("TSLA_P350", "TSLA", "put", 350.0, "2026-03-20", 1, 4.0),
        ];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::Strangle);
    }

    #[test]
    fn identifies_naked_put_when_cash_is_not_verified() {
        let positions = vec![
            option_pos("TSLA_P350", "TSLA", "put", 350.0, "2026-03-20", -2, 5.0),
        ];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::NakedPut);
        assert!((strategies[0].margin.margin_required - 15000.0).abs() < 0.01);
    }

    #[test]
    fn naked_put_margin_otm() {
        // TSLA at 380, short put at 350 (OTM by 30)
        let margin = naked_put_margin(380.0, 350.0, 1, 5.0);
        let formula_a: f64 = 0.20 * 380.0 * 100.0 - 30.0 * 100.0 + 5.0 * 100.0;
        let formula_b: f64 = 0.10 * 380.0 * 100.0 + 5.0 * 100.0;
        assert!((margin - formula_a.max(formula_b)).abs() < 0.01);
    }

    #[test]
    fn naked_put_margin_itm() {
        // TSLA at 340, short put at 350 (ITM)
        let margin = naked_put_margin(340.0, 350.0, 1, 12.0);
        let formula_a: f64 = 0.20 * 340.0 * 100.0 - 0.0 + 12.0 * 100.0; // no OTM deduction when ITM
        let formula_b: f64 = 0.10 * 340.0 * 100.0 + 12.0 * 100.0;
        assert!((margin - formula_a.max(formula_b)).abs() < 0.01);
    }

    #[test]
    fn mixed_portfolio_multi_strategy() {
        let positions = vec![
            // Covered call on TSLA
            stock_pos("TSLA", 100, 350.0),
            option_pos("TSLA_C400", "TSLA", "call", 400.0, "2026-03-20", -1, 5.0),
            // Long put on AAPL
            option_pos("AAPL_P170", "AAPL", "put", 170.0, "2026-04-17", 2, 3.0),
        ];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 2);

        let covered = strategies.iter().find(|s| s.kind == StrategyKind::CoveredCall);
        assert!(covered.is_some());

        let long_put = strategies.iter().find(|s| s.kind == StrategyKind::LongPut);
        assert!(long_put.is_some());
    }

    #[test]
    fn short_straddle_has_non_zero_margin() {
        let positions = vec![
            option_pos("TSLA_C380", "TSLA", "call", 380.0, "2026-03-20", -1, 10.0),
            option_pos("TSLA_P380", "TSLA", "put", 380.0, "2026-03-20", -1, 8.0),
        ];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::Straddle);
        assert!(strategies[0].margin.margin_required > 0.0);
    }
}
