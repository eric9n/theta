use crate::risk_domain::{EnrichedPosition, IdentifiedStrategy, StrategyKind, StrategyMargin};
use crate::risk_engine::{naked_call_margin, naked_put_margin};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct AccountContext {
    pub cash_balance: Option<f64>,
    pub option_buying_power: Option<f64>,
    pub margin_enabled: bool,
}

pub fn evaluate_strategies(
    strategies: &[IdentifiedStrategy],
    positions: &[EnrichedPosition],
    account: &AccountContext,
) -> Vec<IdentifiedStrategy> {
    let mut remaining_cash = account.cash_balance.unwrap_or(0.0);
    let mut spot_by_underlying = HashMap::new();
    for position in positions {
        if let Some(spot) = position.underlying_spot {
            spot_by_underlying.entry(position.underlying.clone()).or_insert(spot);
        }
    }

    strategies
        .iter()
        .cloned()
        .map(|mut strategy| {
            let underlying = strategy.underlying.clone();
            let margin = compute_margin(
                &mut strategy,
                spot_by_underlying.get(&underlying).copied(),
                &mut remaining_cash,
                account,
            );
            strategy.margin = margin;
            strategy
        })
        .collect()
}

fn compute_margin(
    strategy: &mut IdentifiedStrategy,
    spot: Option<f64>,
    remaining_cash: &mut f64,
    account: &AccountContext,
) -> StrategyMargin {
    match strategy.kind {
        StrategyKind::CoveredCall => StrategyMargin {
            margin_required: 0.0,
            method: "Covered by long stock".to_string(),
        },
        StrategyKind::BullPutSpread | StrategyKind::BearCallSpread => {
            let width = spread_width(strategy);
            let credit = net_credit(strategy);
            let max_loss = (width - credit).max(0.0);
            strategy.max_loss = Some(max_loss);
            StrategyMargin {
                margin_required: max_loss,
                method: format!(
                    "credit spread max loss: width({:.2}) - credit({:.2}) = {:.2}",
                    width, credit, max_loss
                ),
            }
        }
        StrategyKind::BullCallSpread | StrategyKind::BearPutSpread | StrategyKind::Butterfly => {
            let debit = net_debit(strategy);
            strategy.max_loss = Some(debit.max(0.0));
            StrategyMargin {
                margin_required: 0.0,
                method: "Defined-risk debit strategy, no additional margin".to_string(),
            }
        }
        StrategyKind::IronCondor => {
            let (call_width, put_width) = iron_condor_widths(strategy);
            let gross_risk = call_width.max(put_width);
            let credit = net_credit(strategy);
            let max_loss = (gross_risk - credit).max(0.0);
            strategy.max_loss = Some(max_loss);
            StrategyMargin {
                margin_required: max_loss,
                method: format!(
                    "iron condor max side risk: max({:.2}, {:.2}) - credit({:.2}) = {:.2}",
                    call_width, put_width, credit, max_loss
                ),
            }
        }
        StrategyKind::NakedPut => {
            let strike = strategy
                .legs
                .iter()
                .find(|leg| leg.side == "put" && leg.quantity < 0)
                .and_then(|leg| leg.strike)
                .unwrap_or(0.0);
            let quantity = short_contracts(strategy);
            let premium = premium_per_contract(strategy);
            let cash_required = strike * quantity as f64 * 100.0;

            if *remaining_cash >= cash_required && cash_required > 0.0 {
                *remaining_cash -= cash_required;
                strategy.kind = StrategyKind::CashSecuredPut;
                strategy.max_loss = Some(cash_required);
                StrategyMargin {
                    margin_required: cash_required,
                    method: format!(
                        "cash-secured by available cash reserve ({:.2} remaining after allocation)",
                        *remaining_cash
                    ),
                }
            } else if account.margin_enabled {
                let live_spot = spot.unwrap_or(strike);
                let margin_required = naked_put_margin(live_spot, strike, quantity, premium);
                strategy.max_loss = None;
                StrategyMargin {
                    margin_required,
                    method: format!(
                        "naked put margin using live spot {:.2}{}",
                        live_spot,
                        if spot.is_some() { "" } else { " (strike proxy)" }
                    ),
                }
            } else {
                strategy.max_loss = Some(cash_required);
                StrategyMargin {
                    margin_required: cash_required,
                    method: "cash account requires full strike collateral".to_string(),
                }
            }
        }
        StrategyKind::Straddle | StrategyKind::Strangle if is_short_premium_structure(strategy) => {
            let live_spot = spot.or_else(|| spot_proxy(strategy));
            if let Some(live_spot) = live_spot {
                let short_call = strategy
                    .legs
                    .iter()
                    .find(|leg| leg.side == "call" && leg.quantity < 0);
                let short_put = strategy
                    .legs
                    .iter()
                    .find(|leg| leg.side == "put" && leg.quantity < 0);
                let qty = short_contracts(strategy);
                let call_margin = short_call
                    .map(|leg| {
                        naked_call_margin(
                            live_spot,
                            leg.strike.unwrap_or(live_spot),
                            qty,
                            leg.price,
                        )
                    })
                    .unwrap_or(0.0);
                let put_margin = short_put
                    .map(|leg| {
                        naked_put_margin(
                            live_spot,
                            leg.strike.unwrap_or(live_spot),
                            qty,
                            leg.price,
                        )
                    })
                    .unwrap_or(0.0);
                let call_premium = short_call
                    .map(|leg| leg.price * leg.quantity.unsigned_abs() as f64 * 100.0)
                    .unwrap_or(0.0);
                let put_premium = short_put
                    .map(|leg| leg.price * leg.quantity.unsigned_abs() as f64 * 100.0)
                    .unwrap_or(0.0);
                let margin_required = (call_margin + put_premium).max(put_margin + call_premium);

                StrategyMargin {
                    margin_required,
                    method: format!(
                        "short premium combo margin using live spot {:.2}{}",
                        live_spot,
                        if spot.is_some() { "" } else { " (proxy)" }
                    ),
                }
            } else {
                StrategyMargin {
                    margin_required: strategy.margin.margin_required,
                    method: "strict margin unavailable without live spot".to_string(),
                }
            }
        }
        StrategyKind::LongCall | StrategyKind::LongPut | StrategyKind::Straddle | StrategyKind::Strangle => {
            StrategyMargin {
                margin_required: 0.0,
                method: "Long premium / debit structure, no additional margin".to_string(),
            }
        }
        _ => StrategyMargin {
            margin_required: strategy.margin.margin_required,
            method: strategy.margin.method.clone(),
        },
    }
}

fn is_short_premium_structure(strategy: &IdentifiedStrategy) -> bool {
    strategy.legs.iter().all(|leg| leg.side == "stock" || leg.quantity < 0)
}

fn spread_width(strategy: &IdentifiedStrategy) -> f64 {
    let qty = short_contracts(strategy) as f64;
    let mut strikes: Vec<f64> = strategy.legs.iter().filter_map(|leg| leg.strike).collect();
    strikes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if strikes.len() < 2 {
        return 0.0;
    }
    (strikes.last().copied().unwrap_or(0.0) - strikes.first().copied().unwrap_or(0.0)).abs()
        * qty
        * 100.0
}

fn iron_condor_widths(strategy: &IdentifiedStrategy) -> (f64, f64) {
    let qty = short_contracts(strategy) as f64;
    let mut call_strikes: Vec<f64> = strategy
        .legs
        .iter()
        .filter(|leg| leg.side == "call")
        .filter_map(|leg| leg.strike)
        .collect();
    let mut put_strikes: Vec<f64> = strategy
        .legs
        .iter()
        .filter(|leg| leg.side == "put")
        .filter_map(|leg| leg.strike)
        .collect();
    call_strikes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    put_strikes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let call_width = if call_strikes.len() >= 2 {
        (call_strikes[1] - call_strikes[0]).abs() * qty * 100.0
    } else {
        0.0
    };
    let put_width = if put_strikes.len() >= 2 {
        (put_strikes[1] - put_strikes[0]).abs() * qty * 100.0
    } else {
        0.0
    };
    (call_width, put_width)
}

fn short_contracts(strategy: &IdentifiedStrategy) -> u64 {
    strategy
        .legs
        .iter()
        .find(|leg| leg.side != "stock" && leg.quantity < 0)
        .map(|leg| leg.quantity.unsigned_abs())
        .unwrap_or(0)
}

fn net_credit(strategy: &IdentifiedStrategy) -> f64 {
    strategy
        .legs
        .iter()
        .filter(|leg| leg.side != "stock")
        .map(|leg| {
            let contracts = leg.quantity.unsigned_abs() as f64 * 100.0;
            if leg.quantity < 0 {
                leg.price * contracts
            } else {
                -leg.price * contracts
            }
        })
        .sum::<f64>()
        .max(0.0)
}

fn net_debit(strategy: &IdentifiedStrategy) -> f64 {
    strategy
        .legs
        .iter()
        .filter(|leg| leg.side != "stock")
        .map(|leg| {
            let contracts = leg.quantity.unsigned_abs() as f64 * 100.0;
            if leg.quantity > 0 {
                leg.price * contracts
            } else {
                -leg.price * contracts
            }
        })
        .sum::<f64>()
        .max(0.0)
}

fn premium_per_contract(strategy: &IdentifiedStrategy) -> f64 {
    strategy
        .legs
        .iter()
        .find(|leg| leg.side == "put" && leg.quantity < 0)
        .map(|leg| leg.price)
        .unwrap_or(0.0)
}

fn spot_proxy(strategy: &IdentifiedStrategy) -> Option<f64> {
    let mut strikes: Vec<f64> = strategy.legs.iter().filter_map(|leg| leg.strike).collect();
    if strikes.is_empty() {
        return None;
    }
    strikes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some((strikes[0] + strikes[strikes.len() - 1]) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::risk_domain::{EnrichedPosition, IdentifiedStrategy, StrategyKind, StrategyLeg, StrategyMargin};

    #[test]
    fn upgrades_naked_put_to_cash_secured_put_when_cash_is_available() {
        let strategies = vec![IdentifiedStrategy {
            kind: StrategyKind::NakedPut,
            underlying: "TSLA".to_string(),
            legs: vec![StrategyLeg {
                symbol: "TSLA_P350".to_string(),
                side: "put".to_string(),
                strike: Some(350.0),
                expiry: Some("2026-03-20".to_string()),
                quantity: -1,
                price: 5.0,
            }],
            margin: StrategyMargin { margin_required: 0.0, method: String::new() },
            max_profit: None,
            max_loss: None,
            breakeven: vec![],
        }];
        let positions = vec![EnrichedPosition {
            symbol: "TSLA_P350".to_string(),
            underlying: "TSLA".to_string(),
            underlying_spot: Some(340.0),
            side: "put".to_string(),
            strike: Some(350.0),
            expiry: Some("2026-03-20".to_string()),
            net_quantity: -1,
            avg_cost: 5.0,
            current_price: 4.0,
            unrealized_pnl_per_unit: 1.0,
            unrealized_pnl: 100.0,
            greeks: None,
        }];
        let evaluated = evaluate_strategies(
            &strategies,
            &positions,
            &AccountContext {
                cash_balance: Some(40_000.0),
                option_buying_power: None,
                margin_enabled: true,
            },
        );
        assert_eq!(evaluated[0].kind, StrategyKind::CashSecuredPut);
        assert!((evaluated[0].margin.margin_required - 35_000.0).abs() < 0.01);
    }
}
