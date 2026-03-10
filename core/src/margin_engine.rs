use crate::risk_domain::{EnrichedPosition, IdentifiedStrategy, StrategyKind, StrategyMargin};
use crate::risk_engine::{naked_call_margin, naked_put_margin};
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct AccountContext {
    pub trade_date_cash: Option<f64>,
    pub settled_cash: Option<f64>,
    pub option_buying_power: Option<f64>,
    pub stock_buying_power: Option<f64>,
    pub margin_loan: Option<f64>,
    pub short_market_value: Option<f64>,
    pub margin_enabled: bool,
}

pub fn evaluate_strategies(
    strategies: &[IdentifiedStrategy],
    positions: &[EnrichedPosition],
    account: &AccountContext,
) -> Vec<IdentifiedStrategy> {
    let mut remaining_cash = cash_secured_budget(account);
    let mut spot_by_underlying = HashMap::new();
    let mut option_price_by_leg = HashMap::new();
    for position in positions {
        if let Some(spot) = position.underlying_spot.filter(|spot| *spot > 0.0) {
            spot_by_underlying
                .entry(position.underlying.clone())
                .or_insert(spot);
        }
        if position.side != "stock" && position.current_price > 0.0 {
            option_price_by_leg.insert(
                leg_price_key(
                    &position.symbol,
                    &position.side,
                    position.strike,
                    position.expiry.as_deref(),
                ),
                position.current_price,
            );
        }
    }

    let mut ordered: Vec<(usize, IdentifiedStrategy)> =
        strategies.iter().cloned().enumerate().collect();
    ordered.sort_by(|(_, left), (_, right)| strategy_sort_key(left).cmp(&strategy_sort_key(right)));

    let mut evaluated = vec![None; strategies.len()];
    for (original_idx, mut strategy) in ordered {
        let underlying = strategy.underlying.clone();
        let margin = compute_margin(
            &mut strategy,
            spot_by_underlying.get(&underlying).copied(),
            &option_price_by_leg,
            &mut remaining_cash,
            account,
        );
        strategy.margin = margin;
        evaluated[original_idx] = Some(strategy);
    }

    evaluated
        .into_iter()
        .map(|strategy| strategy.expect("strategy evaluation should preserve input length"))
        .collect()
}

fn cash_secured_budget(account: &AccountContext) -> f64 {
    let settled_or_cash = account
        .settled_cash
        .or(account.trade_date_cash)
        .unwrap_or(0.0)
        .max(0.0);

    if let Some(option_buying_power) = account.option_buying_power {
        settled_or_cash.min(option_buying_power.max(0.0))
    } else {
        settled_or_cash
    }
}

fn strategy_sort_key(strategy: &IdentifiedStrategy) -> String {
    let mut legs: Vec<String> = strategy
        .legs
        .iter()
        .map(|leg| {
            format!(
                "{}|{}|{}|{:.8}|{}|{:.8}",
                leg.side,
                leg.symbol,
                leg.expiry.as_deref().unwrap_or(""),
                leg.strike.unwrap_or(0.0),
                leg.quantity,
                leg.price
            )
        })
        .collect();
    legs.sort();
    format!(
        "{}|{}|{}",
        strategy.underlying,
        strategy.kind,
        legs.join(";")
    )
}

fn compute_margin(
    strategy: &mut IdentifiedStrategy,
    spot: Option<f64>,
    option_price_by_leg: &HashMap<String, f64>,
    remaining_cash: &mut f64,
    account: &AccountContext,
) -> StrategyMargin {
    let live_spot = spot.filter(|spot| *spot > 0.0);

    match strategy.kind {
        StrategyKind::CoveredCall => {
            let stock_cost = strategy
                .legs
                .iter()
                .find(|leg| leg.side == "stock" && leg.quantity > 0)
                .map(|leg| leg.price * leg.quantity as f64)
                .unwrap_or(0.0);
            let call_credit = strategy
                .legs
                .iter()
                .find(|leg| leg.side == "call" && leg.quantity < 0)
                .map(|leg| leg.price * leg.quantity.unsigned_abs() as f64 * 100.0)
                .unwrap_or(0.0);
            strategy.max_loss = Some((stock_cost - call_credit).max(0.0));
            StrategyMargin {
                margin_required: 0.0,
                method: "Covered by long stock".to_string(),
            }
        }
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
        StrategyKind::BullCallSpread
        | StrategyKind::BearPutSpread
        | StrategyKind::CalendarCallSpread
        | StrategyKind::CalendarPutSpread
        | StrategyKind::DiagonalCallSpread
        | StrategyKind::DiagonalPutSpread
        | StrategyKind::Butterfly => {
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
            let premium = premium_per_contract(strategy, option_price_by_leg);
            let cash_required = strike * quantity as f64 * 100.0;
            let max_loss = (cash_required - premium * quantity as f64 * 100.0).max(0.0);

            if *remaining_cash >= cash_required && cash_required > 0.0 {
                *remaining_cash -= cash_required;
                strategy.kind = StrategyKind::CashSecuredPut;
                strategy.max_loss = Some(max_loss);
                StrategyMargin {
                    margin_required: cash_required,
                    method: format!(
                        "cash-secured by available cash reserve ({:.2} remaining after allocation)",
                        *remaining_cash
                    ),
                }
            } else if account.margin_enabled {
                let live_spot = live_spot.unwrap_or(strike);
                let margin_required = naked_put_margin(live_spot, strike, quantity, premium);
                strategy.max_loss = Some(max_loss);
                StrategyMargin {
                    margin_required,
                    method: format!(
                        "naked put margin using live spot {:.2}{}",
                        live_spot,
                        if live_spot == strike && spot != Some(strike) {
                            " (strike proxy)"
                        } else {
                            ""
                        }
                    ),
                }
            } else {
                strategy.max_loss = Some(max_loss);
                StrategyMargin {
                    margin_required: cash_required,
                    method: "cash account requires full strike collateral".to_string(),
                }
            }
        }
        StrategyKind::Straddle | StrategyKind::Strangle if is_short_premium_structure(strategy) => {
            let live_spot = live_spot.or_else(|| spot_proxy(strategy));
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
                            option_market_price(leg, option_price_by_leg),
                        )
                    })
                    .unwrap_or(0.0);
                let put_margin = short_put
                    .map(|leg| {
                        naked_put_margin(
                            live_spot,
                            leg.strike.unwrap_or(live_spot),
                            qty,
                            option_market_price(leg, option_price_by_leg),
                        )
                    })
                    .unwrap_or(0.0);
                let call_premium = short_call
                    .map(|leg| {
                        option_market_price(leg, option_price_by_leg)
                            * leg.quantity.unsigned_abs() as f64
                            * 100.0
                    })
                    .unwrap_or(0.0);
                let put_premium = short_put
                    .map(|leg| {
                        option_market_price(leg, option_price_by_leg)
                            * leg.quantity.unsigned_abs() as f64
                            * 100.0
                    })
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
        StrategyKind::LongCall
        | StrategyKind::LongPut
        | StrategyKind::Straddle
        | StrategyKind::Strangle => StrategyMargin {
            margin_required: 0.0,
            method: "Long premium / debit structure, no additional margin".to_string(),
        },
        _ => StrategyMargin {
            margin_required: strategy.margin.margin_required,
            method: strategy.margin.method.clone(),
        },
    }
}

fn is_short_premium_structure(strategy: &IdentifiedStrategy) -> bool {
    strategy
        .legs
        .iter()
        .all(|leg| leg.side == "stock" || leg.quantity < 0)
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

fn premium_per_contract(
    strategy: &IdentifiedStrategy,
    option_price_by_leg: &HashMap<String, f64>,
) -> f64 {
    strategy
        .legs
        .iter()
        .find(|leg| leg.side == "put" && leg.quantity < 0)
        .map(|leg| option_market_price(leg, option_price_by_leg))
        .unwrap_or(0.0)
}

fn option_market_price(
    leg: &crate::risk_domain::StrategyLeg,
    option_price_by_leg: &HashMap<String, f64>,
) -> f64 {
    option_price_by_leg
        .get(&leg_price_key(
            &leg.symbol,
            &leg.side,
            leg.strike,
            leg.expiry.as_deref(),
        ))
        .copied()
        .unwrap_or(leg.price)
}

fn leg_price_key(symbol: &str, side: &str, strike: Option<f64>, expiry: Option<&str>) -> String {
    format!(
        "{}|{}|{:.8}|{}",
        symbol,
        side,
        strike.unwrap_or(0.0),
        expiry.unwrap_or("")
    )
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
    use crate::risk_domain::{
        EnrichedPosition, IdentifiedStrategy, StrategyKind, StrategyLeg, StrategyMargin,
    };

    fn naked_put_strategy(symbol: &str, underlying: &str, strike: f64) -> IdentifiedStrategy {
        IdentifiedStrategy {
            kind: StrategyKind::NakedPut,
            underlying: underlying.to_string(),
            legs: vec![StrategyLeg {
                symbol: symbol.to_string(),
                side: "put".to_string(),
                strike: Some(strike),
                expiry: Some("2026-03-20".to_string()),
                quantity: -1,
                price: 5.0,
            }],
            margin: StrategyMargin {
                margin_required: 0.0,
                method: String::new(),
            },
            max_profit: None,
            max_loss: None,
            breakeven: vec![],
        }
    }

    fn covered_call_strategy(
        stock_symbol: &str,
        option_symbol: &str,
        underlying: &str,
    ) -> IdentifiedStrategy {
        IdentifiedStrategy {
            kind: StrategyKind::CoveredCall,
            underlying: underlying.to_string(),
            legs: vec![
                StrategyLeg {
                    symbol: stock_symbol.to_string(),
                    side: "stock".to_string(),
                    strike: None,
                    expiry: None,
                    quantity: 100,
                    price: 400.0,
                },
                StrategyLeg {
                    symbol: option_symbol.to_string(),
                    side: "call".to_string(),
                    strike: Some(420.0),
                    expiry: Some("2026-03-20".to_string()),
                    quantity: -1,
                    price: 2.5,
                },
            ],
            margin: StrategyMargin {
                margin_required: 0.0,
                method: String::new(),
            },
            max_profit: None,
            max_loss: None,
            breakeven: vec![],
        }
    }

    fn naked_put_position(
        symbol: &str,
        underlying: &str,
        strike: f64,
        spot: Option<f64>,
    ) -> EnrichedPosition {
        EnrichedPosition {
            symbol: symbol.to_string(),
            underlying: underlying.to_string(),
            underlying_spot: spot,
            side: "put".to_string(),
            strike: Some(strike),
            expiry: Some("2026-03-20".to_string()),
            net_quantity: -1,
            avg_cost: 5.0,
            current_price: 4.0,
            unrealized_pnl_per_unit: 1.0,
            unrealized_pnl: 100.0,
            greeks: None,
        }
    }

    fn calendar_put_strategy() -> IdentifiedStrategy {
        IdentifiedStrategy {
            kind: StrategyKind::CalendarPutSpread,
            underlying: "TSLA".to_string(),
            legs: vec![
                StrategyLeg {
                    symbol: "TSLA_P385_NEAR".to_string(),
                    side: "put".to_string(),
                    strike: Some(385.0),
                    expiry: Some("2026-03-13".to_string()),
                    quantity: -1,
                    price: 5.94,
                },
                StrategyLeg {
                    symbol: "TSLA_P385_FAR".to_string(),
                    side: "put".to_string(),
                    strike: Some(385.0),
                    expiry: Some("2026-03-27".to_string()),
                    quantity: 1,
                    price: 10.54,
                },
            ],
            margin: StrategyMargin {
                margin_required: 123.0,
                method: "placeholder".to_string(),
            },
            max_profit: None,
            max_loss: None,
            breakeven: vec![],
        }
    }

    #[test]
    fn upgrades_naked_put_to_cash_secured_put_when_cash_is_available() {
        let strategies = vec![naked_put_strategy("TSLA_P350", "TSLA", 350.0)];
        let positions = vec![naked_put_position("TSLA_P350", "TSLA", 350.0, Some(340.0))];
        let evaluated = evaluate_strategies(
            &strategies,
            &positions,
            &AccountContext {
                trade_date_cash: Some(40_000.0),
                settled_cash: Some(40_000.0),
                option_buying_power: None,
                stock_buying_power: None,
                margin_loan: None,
                short_market_value: None,
                margin_enabled: true,
            },
        );
        assert_eq!(evaluated[0].kind, StrategyKind::CashSecuredPut);
        assert!((evaluated[0].margin.margin_required - 35_000.0).abs() < 0.01);
        assert!((evaluated[0].max_loss.unwrap_or_default() - 34_600.0).abs() < 0.01);
    }

    #[test]
    fn covered_call_max_loss_reflects_stock_cost_less_collected_premium() {
        let strategies = vec![covered_call_strategy("TSLA", "TSLA_C420", "TSLA")];
        let evaluated = evaluate_strategies(
            &strategies,
            &[],
            &AccountContext {
                trade_date_cash: Some(0.0),
                settled_cash: Some(0.0),
                option_buying_power: None,
                stock_buying_power: None,
                margin_loan: None,
                short_market_value: None,
                margin_enabled: true,
            },
        );

        assert_eq!(evaluated[0].kind, StrategyKind::CoveredCall);
        assert_eq!(evaluated[0].margin.margin_required, 0.0);
        assert!((evaluated[0].max_loss.unwrap_or_default() - 39_750.0).abs() < 0.01);
    }

    #[test]
    fn calendar_spread_is_treated_as_defined_risk_debit_structure() {
        let strategies = vec![calendar_put_strategy()];
        let evaluated = evaluate_strategies(
            &strategies,
            &[],
            &AccountContext {
                trade_date_cash: Some(0.0),
                settled_cash: Some(0.0),
                option_buying_power: None,
                stock_buying_power: None,
                margin_loan: None,
                short_market_value: None,
                margin_enabled: true,
            },
        );

        assert_eq!(evaluated[0].kind, StrategyKind::CalendarPutSpread);
        assert_eq!(evaluated[0].margin.margin_required, 0.0);
        assert!((evaluated[0].max_loss.unwrap_or_default() - 460.0).abs() < 0.01);
    }

    #[test]
    fn does_not_use_unsettled_cash_to_cash_secure_puts() {
        let strategies = vec![naked_put_strategy("TSLA_P350", "TSLA", 350.0)];
        let positions = vec![naked_put_position("TSLA_P350", "TSLA", 350.0, Some(340.0))];
        let evaluated = evaluate_strategies(
            &strategies,
            &positions,
            &AccountContext {
                trade_date_cash: Some(40_000.0),
                settled_cash: Some(10_000.0),
                option_buying_power: Some(80_000.0),
                stock_buying_power: None,
                margin_loan: None,
                short_market_value: None,
                margin_enabled: true,
            },
        );

        assert_eq!(evaluated[0].kind, StrategyKind::NakedPut);
        assert!(
            (evaluated[0].margin.margin_required - naked_put_margin(340.0, 350.0, 1, 4.0)).abs()
                < 0.01
        );
        assert!((evaluated[0].max_loss.unwrap_or_default() - 34_600.0).abs() < 0.01);
    }

    #[test]
    fn falls_back_to_strike_proxy_when_underlying_spot_is_missing() {
        let strategies = vec![naked_put_strategy("TSLA_P350", "TSLA", 350.0)];
        let positions = vec![naked_put_position("TSLA_P350", "TSLA", 350.0, Some(0.0))];
        let evaluated = evaluate_strategies(
            &strategies,
            &positions,
            &AccountContext {
                trade_date_cash: Some(0.0),
                settled_cash: Some(0.0),
                option_buying_power: None,
                stock_buying_power: None,
                margin_loan: None,
                short_market_value: None,
                margin_enabled: true,
            },
        );

        assert_eq!(evaluated[0].kind, StrategyKind::NakedPut);
        assert!(evaluated[0].margin.method.contains("strike proxy"));
        assert!(
            (evaluated[0].margin.margin_required - naked_put_margin(350.0, 350.0, 1, 4.0)).abs()
                < 0.01
        );
    }

    #[test]
    fn naked_put_margin_uses_current_option_price_not_avg_cost() {
        let strategies = vec![naked_put_strategy("TSLA_P350", "TSLA", 350.0)];
        let positions = vec![naked_put_position("TSLA_P350", "TSLA", 350.0, Some(340.0))];
        let evaluated = evaluate_strategies(
            &strategies,
            &positions,
            &AccountContext {
                trade_date_cash: Some(0.0),
                settled_cash: Some(0.0),
                option_buying_power: None,
                stock_buying_power: None,
                margin_loan: None,
                short_market_value: None,
                margin_enabled: true,
            },
        );

        assert_eq!(evaluated[0].kind, StrategyKind::NakedPut);
        assert!(
            (evaluated[0].margin.margin_required - naked_put_margin(340.0, 350.0, 1, 4.0)).abs()
                < 0.01
        );
        assert!(
            (evaluated[0].margin.margin_required - naked_put_margin(340.0, 350.0, 1, 5.0)).abs()
                > 0.01
        );
    }

    #[test]
    fn cash_secured_put_allocation_is_deterministic() {
        let strategies = vec![
            naked_put_strategy("TSLA_P250", "TSLA", 250.0),
            naked_put_strategy("AAPL_P200", "AAPL", 200.0),
        ];
        let positions = vec![
            naked_put_position("TSLA_P250", "TSLA", 250.0, Some(240.0)),
            naked_put_position("AAPL_P200", "AAPL", 200.0, Some(190.0)),
        ];
        let account = AccountContext {
            trade_date_cash: Some(30_000.0),
            settled_cash: Some(30_000.0),
            option_buying_power: None,
            stock_buying_power: None,
            margin_loan: None,
            short_market_value: None,
            margin_enabled: true,
        };

        let evaluated = evaluate_strategies(&strategies, &positions, &account);
        let mut reversed = strategies.clone();
        reversed.reverse();
        let evaluated_reversed = evaluate_strategies(&reversed, &positions, &account);

        assert_eq!(evaluated[0].underlying, "TSLA");
        assert_eq!(evaluated[1].underlying, "AAPL");
        assert_eq!(evaluated[0].kind, StrategyKind::NakedPut);
        assert_eq!(evaluated[1].kind, StrategyKind::CashSecuredPut);
        assert_eq!(evaluated_reversed[0].underlying, "AAPL");
        assert_eq!(evaluated_reversed[1].underlying, "TSLA");
        assert_eq!(evaluated_reversed[0].kind, StrategyKind::CashSecuredPut);
        assert_eq!(evaluated_reversed[1].kind, StrategyKind::NakedPut);
    }

    #[test]
    fn option_buying_power_caps_cash_secured_budget() {
        let strategies = vec![naked_put_strategy("TSLA_P350", "TSLA", 350.0)];
        let positions = vec![naked_put_position("TSLA_P350", "TSLA", 350.0, Some(340.0))];
        let evaluated = evaluate_strategies(
            &strategies,
            &positions,
            &AccountContext {
                trade_date_cash: Some(40_000.0),
                settled_cash: Some(40_000.0),
                option_buying_power: Some(10_000.0),
                stock_buying_power: None,
                margin_loan: None,
                short_market_value: None,
                margin_enabled: true,
            },
        );

        assert_eq!(evaluated[0].kind, StrategyKind::NakedPut);
        assert!((evaluated[0].max_loss.unwrap_or_default() - 34_600.0).abs() < 0.01);
    }

    #[test]
    fn cash_account_put_max_loss_subtracts_premium() {
        let strategies = vec![naked_put_strategy("TSLA_P350", "TSLA", 350.0)];
        let positions = vec![naked_put_position("TSLA_P350", "TSLA", 350.0, Some(340.0))];
        let evaluated = evaluate_strategies(
            &strategies,
            &positions,
            &AccountContext {
                trade_date_cash: Some(0.0),
                settled_cash: Some(0.0),
                option_buying_power: None,
                stock_buying_power: None,
                margin_loan: None,
                short_market_value: None,
                margin_enabled: false,
            },
        );

        assert_eq!(evaluated[0].kind, StrategyKind::NakedPut);
        assert!((evaluated[0].margin.margin_required - 35_000.0).abs() < 0.01);
        assert!((evaluated[0].max_loss.unwrap_or_default() - 34_600.0).abs() < 0.01);
    }

    #[test]
    fn cash_secured_budget_uses_option_buying_power_when_lower_than_settled_cash() {
        let budget = cash_secured_budget(&AccountContext {
            trade_date_cash: Some(50_000.0),
            settled_cash: Some(40_000.0),
            option_buying_power: Some(12_500.0),
            stock_buying_power: None,
            margin_loan: None,
            short_market_value: None,
            margin_enabled: true,
        });

        assert_eq!(budget, 12_500.0);
    }
}
