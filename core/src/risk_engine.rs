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

    // Group by underlying deterministically so strategy output is stable across runs.
    let mut by_underlying: std::collections::BTreeMap<String, Vec<Position>> =
        std::collections::BTreeMap::new();
    for p in positions {
        by_underlying
            .entry(p.underlying.clone())
            .or_default()
            .push(p.clone());
    }

    for (underlying, mut legs) in by_underlying {
        let mut underlying_strategies = Vec::new();

        // 1. Iron Condor (bear call spread + bull put spread)
        identify_iron_condors(&underlying, &mut legs, &mut underlying_strategies);
        // 2. Butterfly
        identify_butterflies(&underlying, &mut legs, &mut underlying_strategies);
        // 3. Vertical spreads
        identify_vertical_spreads(&underlying, &mut legs, &mut underlying_strategies);
        // 4. Covered Call (stock + short call)
        identify_covered_calls(&underlying, &mut legs, &mut underlying_strategies);
        // 5. Straddle / Strangle
        identify_straddles_strangles(&underlying, &mut legs, &mut underlying_strategies);
        // 6. Cross-expiry debit structures
        identify_cross_expiry_spreads(&underlying, &mut legs, &mut underlying_strategies);
        // 7. Remaining single-leg options
        identify_single_legs(&underlying, &mut legs, &mut underlying_strategies);
        // 8. Remaining stock
        identify_unmatched_stock(&underlying, &mut legs, &mut underlying_strategies);

        sort_identified_strategies(&mut underlying_strategies);
        strategies.extend(underlying_strategies);
    }

    strategies
}

fn sort_identified_strategies(strategies: &mut [IdentifiedStrategy]) {
    strategies.sort_by(|a, b| {
        strategy_output_priority(&a.kind)
            .cmp(&strategy_output_priority(&b.kind))
            .then_with(|| a.underlying.cmp(&b.underlying))
            .then_with(|| strategy_symbol_key(a).cmp(&strategy_symbol_key(b)))
    });
}

fn strategy_symbol_key(strategy: &IdentifiedStrategy) -> Vec<String> {
    let mut symbols: Vec<String> = strategy.legs.iter().map(|leg| leg.symbol.clone()).collect();
    symbols.sort();
    symbols
}

fn strategy_output_priority(kind: &StrategyKind) -> u8 {
    match kind {
        StrategyKind::IronCondor => 0,
        StrategyKind::Butterfly => 1,
        StrategyKind::BullPutSpread => 2,
        StrategyKind::BearCallSpread => 3,
        StrategyKind::BullCallSpread => 4,
        StrategyKind::BearPutSpread => 5,
        StrategyKind::CoveredCall => 6,
        StrategyKind::Straddle => 7,
        StrategyKind::Strangle => 8,
        StrategyKind::CalendarCallSpread => 9,
        StrategyKind::CalendarPutSpread => 10,
        StrategyKind::DiagonalCallSpread => 11,
        StrategyKind::DiagonalPutSpread => 12,
        StrategyKind::LongCall => 13,
        StrategyKind::LongPut => 14,
        StrategyKind::CashSecuredPut => 15,
        StrategyKind::NakedPut => 16,
        StrategyKind::Unmatched => 17,
    }
}

// ---------------------------------------------------------------------------
// Iron Condor: bear call spread + bull put spread, same expiry
// ---------------------------------------------------------------------------

fn identify_iron_condors(
    underlying: &str,
    legs: &mut Vec<Position>,
    out: &mut Vec<IdentifiedStrategy>,
) {
    #[derive(Clone)]
    struct IronCondorCandidate {
        short_call_idx: usize,
        long_call_idx: usize,
        short_put_idx: usize,
        long_put_idx: usize,
        expiry: String,
        max_side_risk: f64,
        total_credit: f64,
        symbol_key: [String; 4],
    }

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
    let mut candidates = Vec::new();

    for &ci1 in &call_indices {
        for &ci2 in &call_indices {
            if ci2 <= ci1 {
                continue;
            }
            let c1 = &legs[ci1];
            let c2 = &legs[ci2];
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

            for &pi1 in &put_indices {
                for &pi2 in &put_indices {
                    if pi2 <= pi1 {
                        continue;
                    }
                    let p1 = &legs[pi1];
                    let p2 = &legs[pi2];
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
                    let total_credit = spread_credit_total(sc, lc) + spread_credit_total(sp, lp);
                    let max_side_risk = gross_call_spread_risk.max(gross_put_spread_risk);

                    let mut symbol_key = vec![
                        sc.symbol.clone(),
                        lc.symbol.clone(),
                        sp.symbol.clone(),
                        lp.symbol.clone(),
                    ];
                    symbol_key.sort();

                    candidates.push(IronCondorCandidate {
                        short_call_idx,
                        long_call_idx,
                        short_put_idx,
                        long_put_idx,
                        expiry: expiry.clone().unwrap_or_default(),
                        max_side_risk,
                        total_credit,
                        symbol_key: [
                            symbol_key[0].clone(),
                            symbol_key[1].clone(),
                            symbol_key[2].clone(),
                            symbol_key[3].clone(),
                        ],
                    });
                }
            }
        }
    }

    candidates.sort_by(|a, b| {
        a.expiry
            .cmp(&b.expiry)
            .then_with(|| a.max_side_risk.total_cmp(&b.max_side_risk))
            .then_with(|| b.total_credit.total_cmp(&a.total_credit))
            .then_with(|| a.symbol_key.cmp(&b.symbol_key))
    });

    for candidate in candidates {
        if consumed.contains(&candidate.short_call_idx)
            || consumed.contains(&candidate.long_call_idx)
            || consumed.contains(&candidate.short_put_idx)
            || consumed.contains(&candidate.long_put_idx)
        {
            continue;
        }

        let sc = &legs[candidate.short_call_idx];
        let lc = &legs[candidate.long_call_idx];
        let sp = &legs[candidate.short_put_idx];
        let lp = &legs[candidate.long_put_idx];
        let margin = (candidate.max_side_risk - candidate.total_credit).max(0.0);

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
                    candidate.max_side_risk, candidate.total_credit, margin
                ),
            },
            max_profit: None,
            max_loss: Some(margin),
            breakeven: vec![],
        });

        consumed.extend([
            candidate.short_call_idx,
            candidate.long_call_idx,
            candidate.short_put_idx,
            candidate.long_put_idx,
        ]);
    }

    remove_consumed(legs, &consumed);
}

// ---------------------------------------------------------------------------
// Cross-expiry debit structures: calendar and diagonal spreads
// ---------------------------------------------------------------------------

fn identify_cross_expiry_spreads(
    underlying: &str,
    legs: &mut Vec<Position>,
    out: &mut Vec<IdentifiedStrategy>,
) {
    #[derive(Clone)]
    struct CrossExpiryCandidate {
        short_idx: usize,
        long_idx: usize,
        kind: StrategyKind,
        strike_gap: f64,
        short_expiry: String,
        long_expiry: String,
        short_symbol: String,
        long_symbol: String,
    }

    let short_indices: Vec<usize> = legs
        .iter()
        .enumerate()
        .filter(|(_, p)| (p.side == "put" || p.side == "call") && p.net_quantity < 0)
        .map(|(i, _)| i)
        .collect();

    let mut consumed = Vec::new();
    let mut candidates = Vec::new();

    for short_idx in short_indices {
        let short = &legs[short_idx];
        let Some(short_expiry) = short.expiry.as_deref() else {
            continue;
        };

        for (long_idx, long) in legs.iter().enumerate() {
            if long_idx == short_idx {
                continue;
            }
            if long.side != short.side || long.net_quantity <= 0 {
                continue;
            }
            if short.net_quantity.unsigned_abs() != long.net_quantity.unsigned_abs() {
                continue;
            }

            let Some(long_expiry) = long.expiry.as_deref() else {
                continue;
            };
            if long_expiry <= short_expiry {
                continue;
            }

            let Some(kind) = classify_cross_expiry_spread(short, long) else {
                continue;
            };

            candidates.push(CrossExpiryCandidate {
                short_idx,
                long_idx,
                kind,
                strike_gap: (short.strike.unwrap_or(0.0) - long.strike.unwrap_or(0.0)).abs(),
                short_expiry: short_expiry.to_string(),
                long_expiry: long_expiry.to_string(),
                short_symbol: short.symbol.clone(),
                long_symbol: long.symbol.clone(),
            });
        }
    }

    candidates.sort_by(|a, b| {
        cross_expiry_kind_priority(&a.kind)
            .cmp(&cross_expiry_kind_priority(&b.kind))
            .then_with(|| a.strike_gap.total_cmp(&b.strike_gap))
            .then_with(|| a.short_expiry.cmp(&b.short_expiry))
            .then_with(|| a.long_expiry.cmp(&b.long_expiry))
            .then_with(|| a.short_symbol.cmp(&b.short_symbol))
            .then_with(|| a.long_symbol.cmp(&b.long_symbol))
    });

    for candidate in candidates {
        if consumed.contains(&candidate.short_idx) || consumed.contains(&candidate.long_idx) {
            continue;
        }

        let short = &legs[candidate.short_idx];
        let long = &legs[candidate.long_idx];
        let net_debit = spread_debit_total(short, long);
        let method = match candidate.kind {
            StrategyKind::CalendarCallSpread | StrategyKind::CalendarPutSpread => {
                format!("calendar spread max loss equals net debit ({net_debit:.2})")
            }
            StrategyKind::DiagonalCallSpread | StrategyKind::DiagonalPutSpread => {
                format!("diagonal spread max loss equals net debit ({net_debit:.2})")
            }
            _ => unreachable!("cross-expiry matcher only emits calendar/diagonal kinds"),
        };

        out.push(IdentifiedStrategy {
            kind: candidate.kind,
            underlying: underlying.to_string(),
            legs: vec![pos_to_leg(short), pos_to_leg(long)],
            margin: StrategyMargin {
                margin_required: 0.0,
                method,
            },
            max_profit: None,
            max_loss: Some(net_debit),
            breakeven: vec![],
        });

        consumed.extend([candidate.short_idx, candidate.long_idx]);
    }

    remove_consumed(legs, &consumed);
}

fn cross_expiry_kind_priority(kind: &StrategyKind) -> u8 {
    match kind {
        StrategyKind::CalendarCallSpread | StrategyKind::CalendarPutSpread => 0,
        StrategyKind::DiagonalCallSpread | StrategyKind::DiagonalPutSpread => 1,
        _ => 2,
    }
}

fn same_expiry_kind_priority(kind: &StrategyKind) -> u8 {
    match kind {
        StrategyKind::Straddle => 0,
        StrategyKind::Strangle => 1,
        StrategyKind::BullPutSpread => 2,
        StrategyKind::BearCallSpread => 3,
        StrategyKind::BullCallSpread => 4,
        StrategyKind::BearPutSpread => 5,
        _ => 6,
    }
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
    #[derive(Clone)]
    struct ButterflyCandidate {
        left_idx: usize,
        body_idx: usize,
        right_idx: usize,
        expiry: String,
        width: f64,
        side_priority: u8,
        symbol_key: [String; 3],
    }

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
        let mut candidates = Vec::new();

        for left in 0..indices.len() {
            for body in (left + 1)..indices.len() {
                for right in (body + 1)..indices.len() {
                    let a = indices[left];
                    let b = indices[body];
                    let c = indices[right];
                    let la = &legs[a];
                    let lb = &legs[b];
                    let lc = &legs[c];

                    if la.expiry != lb.expiry || lb.expiry != lc.expiry {
                        continue;
                    }

                    let left_qty = la.net_quantity;
                    let body_qty = lb.net_quantity;
                    let right_qty = lc.net_quantity;
                    let is_long_butterfly = left_qty > 0
                        && body_qty < 0
                        && right_qty > 0
                        && body_qty.unsigned_abs() == 2 * left_qty.unsigned_abs()
                        && left_qty == right_qty;
                    if !is_long_butterfly {
                        continue;
                    }

                    let left_strike = la.strike.unwrap_or(0.0);
                    let body_strike = lb.strike.unwrap_or(0.0);
                    let right_strike = lc.strike.unwrap_or(0.0);
                    let width = ((body_strike - left_strike).abs()
                        + (right_strike - body_strike).abs())
                        / 2.0;

                    let mut symbol_key =
                        vec![la.symbol.clone(), lb.symbol.clone(), lc.symbol.clone()];
                    symbol_key.sort();

                    candidates.push(ButterflyCandidate {
                        left_idx: a,
                        body_idx: b,
                        right_idx: c,
                        expiry: la.expiry.clone().unwrap_or_default(),
                        width,
                        side_priority: if *side_str == "call" { 0 } else { 1 },
                        symbol_key: [
                            symbol_key[0].clone(),
                            symbol_key[1].clone(),
                            symbol_key[2].clone(),
                        ],
                    });
                }
            }
        }

        candidates.sort_by(|a, b| {
            a.expiry
                .cmp(&b.expiry)
                .then_with(|| a.side_priority.cmp(&b.side_priority))
                .then_with(|| a.width.total_cmp(&b.width))
                .then_with(|| a.symbol_key.cmp(&b.symbol_key))
        });

        for candidate in candidates {
            if consumed.contains(&candidate.left_idx)
                || consumed.contains(&candidate.body_idx)
                || consumed.contains(&candidate.right_idx)
            {
                continue;
            }

            let la = &legs[candidate.left_idx];
            let lb = &legs[candidate.body_idx];
            let lc = &legs[candidate.right_idx];

            out.push(IdentifiedStrategy {
                kind: StrategyKind::Butterfly,
                underlying: underlying.to_string(),
                legs: vec![pos_to_leg(la), pos_to_leg(lb), pos_to_leg(lc)],
                margin: StrategyMargin {
                    margin_required: 0.0,
                    method: "Debit strategy, no margin required".to_string(),
                },
                max_profit: None,
                max_loss: None,
                breakeven: vec![],
            });
            consumed.extend([candidate.left_idx, candidate.body_idx, candidate.right_idx]);
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
    #[derive(Clone)]
    struct VerticalCandidate {
        first_idx: usize,
        second_idx: usize,
        kind: StrategyKind,
        width: f64,
        expiry: String,
        short_symbol: String,
        long_symbol: String,
    }

    let mut consumed = Vec::new();
    let mut candidates = Vec::new();

    for side_str in &["put", "call"] {
        let indices: Vec<usize> = legs
            .iter()
            .enumerate()
            .filter(|(i, p)| p.side == *side_str && !consumed.contains(i))
            .map(|(i, _)| i)
            .collect();

        for &i in &indices {
            for &j in &indices {
                if j <= i {
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

                let width = (a_strike - b_strike).abs();

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

                let Some(expiry) = a.expiry.clone() else {
                    continue;
                };

                candidates.push(VerticalCandidate {
                    first_idx: i,
                    second_idx: j,
                    kind,
                    width,
                    expiry,
                    short_symbol: short.symbol.clone(),
                    long_symbol: long.symbol.clone(),
                });
            }
        }
    }

    candidates.sort_by(|a, b| {
        same_expiry_kind_priority(&a.kind)
            .cmp(&same_expiry_kind_priority(&b.kind))
            .then_with(|| a.width.total_cmp(&b.width))
            .then_with(|| a.expiry.cmp(&b.expiry))
            .then_with(|| a.short_symbol.cmp(&b.short_symbol))
            .then_with(|| a.long_symbol.cmp(&b.long_symbol))
    });

    for candidate in candidates {
        if consumed.contains(&candidate.first_idx) || consumed.contains(&candidate.second_idx) {
            continue;
        }

        let a = &legs[candidate.first_idx];
        let b = &legs[candidate.second_idx];
        let qty = a.net_quantity.unsigned_abs() as f64;
        let gross_spread_risk = candidate.width * qty * 100.0;

        let (short, long) = if a.net_quantity < 0 { (a, b) } else { (b, a) };
        let net_credit_total = spread_credit_total(short, long);
        let net_debit_total = spread_debit_total(short, long);
        let (margin_required, max_loss, margin_method) = match candidate.kind {
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
            kind: candidate.kind,
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

        consumed.extend([candidate.first_idx, candidate.second_idx]);
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
    #[derive(Clone)]
    struct CoveredCallCandidate {
        call_idx: usize,
        shares_needed: i64,
        expiry: String,
        strike: f64,
        symbol: String,
    }

    let stock_idx = legs
        .iter()
        .position(|p| p.side == "stock" && p.net_quantity > 0);
    if stock_idx.is_none() {
        return;
    }
    let stock_idx = stock_idx.unwrap();

    let mut consumed = Vec::new();
    let stock = &legs[stock_idx];
    let mut remaining_shares = stock.net_quantity;
    let mut candidates = Vec::new();

    for (ci, call) in legs.iter().enumerate() {
        if ci == stock_idx || call.side != "call" || call.net_quantity >= 0 {
            continue;
        }
        let contracts = call.net_quantity.unsigned_abs() as i64;
        let shares_needed = contracts * 100;
        let Some(expiry) = call.expiry.clone() else {
            continue;
        };
        candidates.push(CoveredCallCandidate {
            call_idx: ci,
            shares_needed,
            expiry,
            strike: call.strike.unwrap_or(0.0),
            symbol: call.symbol.clone(),
        });
    }

    candidates.sort_by(|a, b| {
        a.expiry
            .cmp(&b.expiry)
            .then_with(|| a.strike.total_cmp(&b.strike))
            .then_with(|| a.symbol.cmp(&b.symbol))
    });

    for candidate in candidates {
        if remaining_shares < candidate.shares_needed {
            continue;
        }

        let call = &legs[candidate.call_idx];
        out.push(IdentifiedStrategy {
            kind: StrategyKind::CoveredCall,
            underlying: underlying.to_string(),
            legs: vec![
                StrategyLeg {
                    symbol: stock.symbol.clone(),
                    side: "stock".to_string(),
                    strike: None,
                    expiry: None,
                    quantity: candidate.shares_needed,
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
        remaining_shares -= candidate.shares_needed;
        consumed.push(candidate.call_idx);
    }

    if remaining_shares == 0 {
        consumed.push(stock_idx);
    } else {
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
    #[derive(Clone)]
    struct StraddleCandidate {
        call_idx: usize,
        put_idx: usize,
        kind: StrategyKind,
        strike_gap: f64,
        expiry: String,
        call_symbol: String,
        put_symbol: String,
    }

    let mut consumed = Vec::new();
    let mut candidates = Vec::new();

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
        for &pi in &put_indices {
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
            let Some(expiry) = call.expiry.clone() else {
                continue;
            };

            candidates.push(StraddleCandidate {
                call_idx: ci,
                put_idx: pi,
                kind,
                strike_gap: (call.strike.unwrap_or(0.0) - put.strike.unwrap_or(0.0)).abs(),
                expiry,
                call_symbol: call.symbol.clone(),
                put_symbol: put.symbol.clone(),
            });
        }
    }

    candidates.sort_by(|a, b| {
        same_expiry_kind_priority(&a.kind)
            .cmp(&same_expiry_kind_priority(&b.kind))
            .then_with(|| a.strike_gap.total_cmp(&b.strike_gap))
            .then_with(|| a.expiry.cmp(&b.expiry))
            .then_with(|| a.call_symbol.cmp(&b.call_symbol))
            .then_with(|| a.put_symbol.cmp(&b.put_symbol))
    });

    for candidate in candidates {
        if consumed.contains(&candidate.call_idx) || consumed.contains(&candidate.put_idx) {
            continue;
        }

        let call = &legs[candidate.call_idx];
        let put = &legs[candidate.put_idx];
        let is_short = call.net_quantity < 0;
        let qty = call.net_quantity.unsigned_abs();
        let same_strike = matches!(candidate.kind, StrategyKind::Straddle);
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
            kind: candidate.kind,
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
        consumed.extend([candidate.call_idx, candidate.put_idx]);
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

fn classify_cross_expiry_spread(short: &Position, long: &Position) -> Option<StrategyKind> {
    let short_strike = short.strike?;
    let long_strike = long.strike?;
    let same_strike = (short_strike - long_strike).abs() < 0.01;

    if same_strike {
        return match short.side.as_str() {
            "call" => Some(StrategyKind::CalendarCallSpread),
            "put" => Some(StrategyKind::CalendarPutSpread),
            _ => None,
        };
    }

    match short.side.as_str() {
        "call" if long_strike < short_strike => Some(StrategyKind::DiagonalCallSpread),
        "put" if long_strike > short_strike => Some(StrategyKind::DiagonalPutSpread),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Margin calculation — Firstrade / Apex rules
// ---------------------------------------------------------------------------

/// Calculate naked put margin: Firstrade formula
/// max(20% × spot × qty × 100 − OTM + premium, 10% × spot × qty × 100 + premium)
pub fn naked_put_margin(spot: f64, strike: f64, quantity: u64, premium_per_contract: f64) -> f64 {
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
pub fn naked_call_margin(spot: f64, strike: f64, quantity: u64, premium_per_contract: f64) -> f64 {
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
        if p.side == "stock" {
            greeks.net_delta_shares += p.net_quantity as f64;
            continue;
        }

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
            option_pos(
                "TSLA260320C00400000",
                "TSLA",
                "call",
                400.0,
                "2026-03-20",
                -2,
                5.30,
            ),
        ];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::CoveredCall);
        assert_eq!(strategies[0].legs.len(), 2);
    }

    #[test]
    fn covered_call_matching_is_stable_across_input_order() {
        let ordered = vec![
            stock_pos("TSLA", 200, 350.0),
            option_pos(
                "TSLA_C390_NEAR",
                "TSLA",
                "call",
                390.0,
                "2026-03-20",
                -1,
                8.0,
            ),
            option_pos(
                "TSLA_C400_FAR",
                "TSLA",
                "call",
                400.0,
                "2026-04-17",
                -1,
                6.0,
            ),
            option_pos(
                "TSLA_C380_NEAR",
                "TSLA",
                "call",
                380.0,
                "2026-03-20",
                -1,
                9.0,
            ),
        ];
        let reversed: Vec<Position> = ordered.iter().cloned().rev().collect();

        let ordered_kinds: Vec<(StrategyKind, Vec<String>)> = identify_strategies(&ordered)
            .into_iter()
            .map(|strategy| {
                let mut legs: Vec<String> =
                    strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                legs.sort();
                (strategy.kind, legs)
            })
            .collect();
        let reversed_kinds: Vec<(StrategyKind, Vec<String>)> = identify_strategies(&reversed)
            .into_iter()
            .map(|strategy| {
                let mut legs: Vec<String> =
                    strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                legs.sort();
                (strategy.kind, legs)
            })
            .collect();

        assert_eq!(ordered_kinds, reversed_kinds);
    }

    #[test]
    fn identifies_bull_put_spread() {
        let positions = vec![
            option_pos(
                "TSLA260320P00350000",
                "TSLA",
                "put",
                350.0,
                "2026-03-20",
                -1,
                8.0,
            ),
            option_pos(
                "TSLA260320P00340000",
                "TSLA",
                "put",
                340.0,
                "2026-03-20",
                1,
                5.0,
            ),
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
            option_pos(
                "TSLA260320C00400000",
                "TSLA",
                "call",
                400.0,
                "2026-03-20",
                -1,
                5.0,
            ),
            option_pos(
                "TSLA260320C00410000",
                "TSLA",
                "call",
                410.0,
                "2026-03-20",
                1,
                2.0,
            ),
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
    fn iron_condor_matching_is_stable_across_input_order() {
        let ordered = vec![
            option_pos("TSLA_C400", "TSLA", "call", 400.0, "2026-03-20", -1, 5.0),
            option_pos("TSLA_C410", "TSLA", "call", 410.0, "2026-03-20", 1, 2.0),
            option_pos("TSLA_C420", "TSLA", "call", 420.0, "2026-03-20", 1, 1.0),
            option_pos("TSLA_P350", "TSLA", "put", 350.0, "2026-03-20", -1, 4.0),
            option_pos("TSLA_P340", "TSLA", "put", 340.0, "2026-03-20", 1, 2.0),
            option_pos("TSLA_P330", "TSLA", "put", 330.0, "2026-03-20", 1, 1.0),
        ];
        let reversed: Vec<Position> = ordered.iter().cloned().rev().collect();

        let ordered_kinds: Vec<(StrategyKind, Vec<String>)> = identify_strategies(&ordered)
            .into_iter()
            .map(|strategy| {
                let mut legs: Vec<String> =
                    strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                legs.sort();
                (strategy.kind, legs)
            })
            .collect();
        let reversed_kinds: Vec<(StrategyKind, Vec<String>)> = identify_strategies(&reversed)
            .into_iter()
            .map(|strategy| {
                let mut legs: Vec<String> =
                    strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                legs.sort();
                (strategy.kind, legs)
            })
            .collect();

        assert_eq!(ordered_kinds, reversed_kinds);
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
        let positions = vec![option_pos(
            "TSLA_P350",
            "TSLA",
            "put",
            350.0,
            "2026-03-20",
            -2,
            5.0,
        )];
        let strategies = identify_strategies(&positions);
        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::NakedPut);
        assert!((strategies[0].margin.margin_required - 15000.0).abs() < 0.01);
    }

    #[test]
    fn identifies_calendar_put_spread_from_matching_strikes() {
        let positions = vec![
            option_pos(
                "TSLA_P385_NEAR",
                "TSLA",
                "put",
                385.0,
                "2026-03-13",
                -1,
                5.94,
            ),
            option_pos(
                "TSLA_P385_FAR",
                "TSLA",
                "put",
                385.0,
                "2026-03-27",
                1,
                10.54,
            ),
        ];

        let strategies = identify_strategies(&positions);

        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::CalendarPutSpread);
        assert_eq!(strategies[0].legs.len(), 2);
        assert!((strategies[0].max_loss.unwrap_or_default() - 460.0).abs() < 0.01);
    }

    #[test]
    fn identifies_diagonal_call_spread_from_cross_expiry_pair() {
        let positions = vec![
            option_pos(
                "TSLA_C450_NEAR",
                "TSLA",
                "call",
                450.0,
                "2026-04-10",
                -1,
                7.49,
            ),
            option_pos(
                "TSLA_C430_FAR",
                "TSLA",
                "call",
                430.0,
                "2026-05-15",
                1,
                23.79,
            ),
        ];

        let strategies = identify_strategies(&positions);

        assert_eq!(strategies.len(), 1);
        assert_eq!(strategies[0].kind, StrategyKind::DiagonalCallSpread);
        assert_eq!(strategies[0].legs.len(), 2);
        assert!((strategies[0].max_loss.unwrap_or_default() - 1_630.0).abs() < 0.01);
    }

    #[test]
    fn cross_expiry_matching_is_stable_across_input_order() {
        let ordered = vec![
            option_pos(
                "TSLA_C450_NEAR",
                "TSLA",
                "call",
                450.0,
                "2026-04-10",
                -1,
                7.49,
            ),
            option_pos(
                "TSLA_C440_NEAR",
                "TSLA",
                "call",
                440.0,
                "2026-04-17",
                -1,
                12.45,
            ),
            option_pos(
                "TSLA_C430_FAR",
                "TSLA",
                "call",
                430.0,
                "2026-05-15",
                1,
                23.79,
            ),
            option_pos(
                "TSLA_C450_FAR",
                "TSLA",
                "call",
                450.0,
                "2026-06-18",
                1,
                26.52,
            ),
        ];
        let reversed = vec![
            option_pos(
                "TSLA_C450_FAR",
                "TSLA",
                "call",
                450.0,
                "2026-06-18",
                1,
                26.52,
            ),
            option_pos(
                "TSLA_C430_FAR",
                "TSLA",
                "call",
                430.0,
                "2026-05-15",
                1,
                23.79,
            ),
            option_pos(
                "TSLA_C440_NEAR",
                "TSLA",
                "call",
                440.0,
                "2026-04-17",
                -1,
                12.45,
            ),
            option_pos(
                "TSLA_C450_NEAR",
                "TSLA",
                "call",
                450.0,
                "2026-04-10",
                -1,
                7.49,
            ),
        ];

        let ordered_kinds: Vec<(StrategyKind, Vec<String>)> = identify_strategies(&ordered)
            .into_iter()
            .map(|strategy| {
                let mut legs: Vec<String> =
                    strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                legs.sort();
                (strategy.kind, legs)
            })
            .collect();
        let reversed_kinds: Vec<(StrategyKind, Vec<String>)> = identify_strategies(&reversed)
            .into_iter()
            .map(|strategy| {
                let mut legs: Vec<String> =
                    strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                legs.sort();
                (strategy.kind, legs)
            })
            .collect();

        assert_eq!(ordered_kinds, reversed_kinds);
    }

    #[test]
    fn butterfly_matching_is_stable_across_input_order() {
        let ordered = vec![
            option_pos("TSLA_C380", "TSLA", "call", 380.0, "2026-03-20", 1, 12.0),
            option_pos("TSLA_C390", "TSLA", "call", 390.0, "2026-03-20", -2, 7.0),
            option_pos("TSLA_C400", "TSLA", "call", 400.0, "2026-03-20", 1, 3.0),
            option_pos("TSLA_C410", "TSLA", "call", 410.0, "2026-03-20", 1, 1.5),
        ];
        let reversed: Vec<Position> = ordered.iter().cloned().rev().collect();

        let ordered_kinds: Vec<(StrategyKind, Vec<String>)> = identify_strategies(&ordered)
            .into_iter()
            .map(|strategy| {
                let mut legs: Vec<String> =
                    strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                legs.sort();
                (strategy.kind, legs)
            })
            .collect();
        let reversed_kinds: Vec<(StrategyKind, Vec<String>)> = identify_strategies(&reversed)
            .into_iter()
            .map(|strategy| {
                let mut legs: Vec<String> =
                    strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                legs.sort();
                (strategy.kind, legs)
            })
            .collect();

        assert_eq!(ordered_kinds, reversed_kinds);
    }

    #[test]
    fn vertical_matching_is_stable_across_input_order() {
        let ordered = vec![
            option_pos(
                "TSLA_P360_SHORT",
                "TSLA",
                "put",
                360.0,
                "2026-03-20",
                -1,
                7.0,
            ),
            option_pos("TSLA_P350_LONG", "TSLA", "put", 350.0, "2026-03-20", 1, 4.0),
            option_pos(
                "TSLA_P340_SHORT",
                "TSLA",
                "put",
                340.0,
                "2026-03-20",
                -1,
                3.5,
            ),
            option_pos("TSLA_P330_LONG", "TSLA", "put", 330.0, "2026-03-20", 1, 1.5),
        ];
        let reversed: Vec<Position> = ordered.iter().cloned().rev().collect();

        let ordered_kinds: Vec<(StrategyKind, Vec<String>)> = identify_strategies(&ordered)
            .into_iter()
            .map(|strategy| {
                let mut legs: Vec<String> =
                    strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                legs.sort();
                (strategy.kind, legs)
            })
            .collect();
        let reversed_kinds: Vec<(StrategyKind, Vec<String>)> = identify_strategies(&reversed)
            .into_iter()
            .map(|strategy| {
                let mut legs: Vec<String> =
                    strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                legs.sort();
                (strategy.kind, legs)
            })
            .collect();

        assert_eq!(ordered_kinds, reversed_kinds);
    }

    #[test]
    fn straddle_matching_prefers_same_strike_pair_over_strangle() {
        let positions = vec![
            option_pos("TSLA_C380", "TSLA", "call", 380.0, "2026-03-20", -1, 10.0),
            option_pos("TSLA_P380", "TSLA", "put", 380.0, "2026-03-20", -1, 8.0),
            option_pos("TSLA_P360", "TSLA", "put", 360.0, "2026-03-20", -1, 4.0),
        ];

        let strategies = identify_strategies(&positions);

        assert!(
            strategies
                .iter()
                .any(|strategy| strategy.kind == StrategyKind::Straddle)
        );
        assert!(
            !strategies
                .iter()
                .any(|strategy| strategy.kind == StrategyKind::Strangle)
        );
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

        let covered = strategies
            .iter()
            .find(|s| s.kind == StrategyKind::CoveredCall);
        assert!(covered.is_some());

        let long_put = strategies.iter().find(|s| s.kind == StrategyKind::LongPut);
        assert!(long_put.is_some());
    }

    #[test]
    fn identify_strategies_orders_underlyings_stably() {
        let ordered = vec![
            option_pos("TSLA_P350", "TSLA", "put", 350.0, "2026-03-20", -1, 5.0),
            option_pos("AAPL_P170", "AAPL", "put", 170.0, "2026-04-17", 1, 3.0),
        ];
        let reversed: Vec<Position> = ordered.iter().cloned().rev().collect();

        let ordered_result: Vec<(String, StrategyKind)> = identify_strategies(&ordered)
            .into_iter()
            .map(|strategy| (strategy.underlying, strategy.kind))
            .collect();
        let reversed_result: Vec<(String, StrategyKind)> = identify_strategies(&reversed)
            .into_iter()
            .map(|strategy| (strategy.underlying, strategy.kind))
            .collect();

        assert_eq!(
            ordered_result,
            vec![
                ("AAPL".to_string(), StrategyKind::LongPut),
                ("TSLA".to_string(), StrategyKind::NakedPut),
            ]
        );
        assert_eq!(ordered_result, reversed_result);
    }

    #[test]
    fn identify_strategies_orders_single_leg_residuals_stably() {
        let ordered = vec![
            option_pos("TSLA_P350", "TSLA", "put", 350.0, "2026-03-20", -1, 5.0),
            option_pos("TSLA_C420", "TSLA", "call", 420.0, "2026-04-17", 1, 4.0),
        ];
        let reversed: Vec<Position> = ordered.iter().cloned().rev().collect();

        let ordered_result: Vec<(String, StrategyKind, Vec<String>)> =
            identify_strategies(&ordered)
                .into_iter()
                .map(|strategy| {
                    let mut legs: Vec<String> =
                        strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                    legs.sort();
                    (strategy.underlying, strategy.kind, legs)
                })
                .collect();
        let reversed_result: Vec<(String, StrategyKind, Vec<String>)> =
            identify_strategies(&reversed)
                .into_iter()
                .map(|strategy| {
                    let mut legs: Vec<String> =
                        strategy.legs.into_iter().map(|leg| leg.symbol).collect();
                    legs.sort();
                    (strategy.underlying, strategy.kind, legs)
                })
                .collect();

        assert_eq!(
            ordered_result,
            vec![
                (
                    "TSLA".to_string(),
                    StrategyKind::LongCall,
                    vec!["TSLA_C420".to_string()],
                ),
                (
                    "TSLA".to_string(),
                    StrategyKind::NakedPut,
                    vec!["TSLA_P350".to_string()],
                ),
            ]
        );
        assert_eq!(ordered_result, reversed_result);
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

    #[test]
    fn aggregate_greeks_includes_stock_delta() {
        let positions = vec![EnrichedPosition {
            symbol: "TSLA".to_string(),
            underlying: "TSLA".to_string(),
            underlying_spot: Some(400.0),
            side: "stock".to_string(),
            strike: None,
            expiry: None,
            net_quantity: 100,
            avg_cost: 350.0,
            current_price: 400.0,
            unrealized_pnl_per_unit: 50.0,
            unrealized_pnl: 5_000.0,
            greeks: None,
        }];

        let greeks = aggregate_greeks(&positions);
        assert_eq!(greeks.net_delta_shares, 100.0);
        assert_eq!(greeks.total_gamma, 0.0);
    }

    #[test]
    fn identifies_call_vertical_even_after_put_spread_consumes_indices() {
        let positions = vec![
            Position {
                symbol: "SHORT0".to_string(),
                underlying: "TSLA".to_string(),
                side: "call".to_string(),
                strike: Some(400.0),
                expiry: Some("2026-03-20".to_string()),
                net_quantity: -1,
                avg_cost: 5.0,
                total_cost: 5.0,
            },
            Position {
                symbol: "AA".to_string(),
                underlying: "TSLA".to_string(),
                side: "call".to_string(),
                strike: Some(420.0),
                expiry: Some("2026-04-17".to_string()),
                net_quantity: -1,
                avg_cost: 5.0,
                total_cost: 5.0,
            },
            Position {
                symbol: "LONG0".to_string(),
                underlying: "TSLA".to_string(),
                side: "call".to_string(),
                strike: Some(410.0),
                expiry: Some("2026-03-20".to_string()),
                net_quantity: 1,
                avg_cost: 2.0,
                total_cost: 2.0,
            },
            Position {
                symbol: "BBB".to_string(),
                underlying: "TSLA".to_string(),
                side: "call".to_string(),
                strike: Some(430.0),
                expiry: Some("2026-04-17".to_string()),
                net_quantity: 1,
                avg_cost: 2.0,
                total_cost: 2.0,
            },
        ];

        let strategies = identify_strategies(&positions);
        let bear_call_spreads = strategies
            .iter()
            .filter(|s| s.kind == StrategyKind::BearCallSpread)
            .count();
        assert_eq!(bear_call_spreads, 2);
    }
}
