use crate::ledger::{Ledger, Trade, TradeFilter};
use anyhow::{Context, Result, bail};
use std::collections::HashSet;
use time::{Date, Duration, Weekday};

pub struct AccountingService<'a> {
    ledger: &'a Ledger,
    trading_calendar: Option<HashSet<Date>>,
}

impl<'a> AccountingService<'a> {
    pub fn new(ledger: &'a Ledger) -> Self {
        Self {
            ledger,
            trading_calendar: None,
        }
    }

    pub fn with_calendar(mut self, calendar: Vec<Date>) -> Self {
        self.trading_calendar = Some(calendar.into_iter().collect());
        self
    }

    /// Calculate the settlement date for a given trade date based on T+1 rule.
    /// Skips non-trading days (weekends and optional holidays).
    pub fn get_settlement_date(&self, trade_date_str: &str) -> Result<String> {
        let format = time::format_description::parse("[year]-[month]-[day]")
            .context("failed to parse date format")?;
        let date = Date::parse(trade_date_str, &format)
            .with_context(|| format!("failed to parse trade date: {}", trade_date_str))?;

        // T+1 logic: Find the next trading day after the trade date
        let mut settled = date + Duration::days(1);

        loop {
            if let Some(ref calendar) = self.trading_calendar {
                if calendar.contains(&settled) {
                    break;
                }
            } else if !is_weekend(settled) {
                // Fallback to weekend-only logic if calendar is not provided
                break;
            }
            settled += Duration::days(1);

            // Safety break to prevent infinite loop if calendar is empty or overly restricted
            if settled > date + Duration::days(14) {
                bail!(
                    "failed to find settlement date within 14 days of {} using current trading calendar",
                    trade_date_str
                );
            }
        }

        Ok(settled.format(&format)?)
    }

    /// Calculate the cash flow of a single trade.
    pub fn calculate_cash_flow(trade: &Trade) -> Result<f64> {
        match trade.action.as_str() {
            "deposit" => Ok(trade.price * trade.quantity as f64),
            "withdraw" => Ok(-trade.price * trade.quantity as f64),
            "dividend" => Ok(trade.price * trade.quantity as f64), // dividend is a positive flow
            "buy" => {
                let multiplier = if trade.side == "stock" { 1.0 } else { 100.0 };
                let principal = trade.price * trade.quantity as f64 * multiplier;
                Ok(-principal - trade.commission)
            }
            "sell" => {
                let multiplier = if trade.side == "stock" { 1.0 } else { 100.0 };
                let principal = trade.price * trade.quantity as f64 * multiplier;
                Ok(principal - trade.commission)
            }
            other => bail!("unsupported trade action in cash-flow replay: {}", other),
        }
    }

    /// Derive the current account balances (trade-date and settled) based on all historical trades
    /// and a starting base snapshot if provided.
    pub fn derive_balances(&self, account_id: &str, as_of_date: &str) -> Result<(f64, f64)> {
        self.derive_balances_inner(account_id, as_of_date, true)
    }

    /// Derive balances by replaying the full ledger from origin, ignoring any snapshots.
    pub fn derive_balances_from_origin(
        &self,
        account_id: &str,
        as_of_date: &str,
    ) -> Result<(f64, f64)> {
        self.derive_balances_inner(account_id, as_of_date, false)
    }

    fn derive_balances_inner(
        &self,
        account_id: &str,
        as_of_date: &str,
        use_checkpoint: bool,
    ) -> Result<(f64, f64)> {
        let checkpoint = if use_checkpoint {
            self.ledger.latest_manual_snapshot(account_id)?
        } else {
            None
        };

        let (mut trade_date_total, mut settled_total, checkpoint_trade_id, legacy_start_date) =
            if let Some(cp) = checkpoint {
                let cp_date = &cp.snapshot_at[..10];
                (
                    cp.trade_date_cash,
                    cp.settled_cash,
                    cp.baseline_trade_id,
                    Some(cp_date.to_string()),
                )
            } else {
                (0.0, 0.0, None, None)
            };

        let filter = TradeFilter {
            account_id: Some(account_id.to_string()),
            ..Default::default()
        };
        let trades = self.ledger.list_trades(&filter)?;

        for trade in trades {
            if let Some(start_id) = checkpoint_trade_id {
                if trade.id <= start_id {
                    continue;
                }
            } else if let Some(ref start) = legacy_start_date {
                // Legacy snapshots do not have a trade cursor, so date-based replay remains
                // the best available fallback for pre-migration rows.
                if trade.trade_date <= *start {
                    continue;
                }
            }

            let flow = Self::calculate_cash_flow(&trade)?;
            let settlement_date = if Self::settles_same_day(&trade) {
                trade.trade_date.clone()
            } else {
                self.get_settlement_date(&trade.trade_date)?
            };

            // Trade date cash is immediate
            if trade.trade_date.as_str() <= as_of_date {
                trade_date_total += flow;
            }

            // Settled cash is only after settlement date
            if settlement_date.as_str() <= as_of_date {
                settled_total += flow;
            }
        }

        Ok((trade_date_total, settled_total))
    }

    fn settles_same_day(trade: &Trade) -> bool {
        matches!(trade.action.as_str(), "deposit" | "withdraw" | "dividend")
    }
}

fn is_weekend(date: Date) -> bool {
    matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::Ledger;
    use tempfile::tempdir;

    fn temp_ledger() -> Ledger {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("portfolio.db");
        let ledger = Ledger::open(&db_path).unwrap();
        std::mem::forget(dir);
        ledger
    }

    #[test]
    fn derive_balances_uses_snapshot_trade_cursor_for_same_day_trades() {
        let ledger = temp_ledger();
        ledger
            .record_trade(
                "2026-03-06",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "buy",
                10,
                100.0,
                0.0,
                "initial trade before snapshot",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_account_snapshot(
                "2026-03-06T09:30:00Z",
                9_000.0,
                9_000.0,
                None,
                None,
                None,
                None,
                true,
                "manual set",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_trade(
                "2026-03-06",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "sell",
                5,
                110.0,
                0.0,
                "later same-day trade after snapshot",
                "firstrade",
            )
            .unwrap();

        let service = AccountingService::new(&ledger);
        let (trade_date_cash, settled_cash) =
            service.derive_balances("firstrade", "2026-03-09").unwrap();

        assert_eq!(trade_date_cash, 9_550.0);
        assert_eq!(settled_cash, 9_550.0);
    }

    #[test]
    fn get_settlement_date_skips_weekend_without_calendar() {
        let ledger = temp_ledger();
        let service = AccountingService::new(&ledger);

        let settlement = service.get_settlement_date("2026-03-06").unwrap();
        assert_eq!(settlement, "2026-03-09");
    }

    #[test]
    fn get_settlement_date_uses_trading_calendar_when_provided() {
        let ledger = temp_ledger();
        let service = AccountingService::new(&ledger).with_calendar(vec![
            Date::from_calendar_date(2026, time::Month::March, 10).unwrap(),
        ]);

        let settlement = service.get_settlement_date("2026-03-06").unwrap();
        assert_eq!(settlement, "2026-03-10");
    }

    #[test]
    fn get_settlement_date_errors_when_calendar_has_no_future_trading_day() {
        let ledger = temp_ledger();
        let service = AccountingService::new(&ledger).with_calendar(vec![]);

        let err = service.get_settlement_date("2026-03-06").unwrap_err();
        assert!(
            err.to_string()
                .contains("failed to find settlement date within 14 days")
        );
    }

    #[test]
    fn derive_balances_from_origin_ignores_manual_snapshot_checkpoint() {
        let ledger = temp_ledger();
        ledger
            .record_trade(
                "2026-03-05",
                "CASH",
                "CASH",
                "stock",
                None,
                None,
                "deposit",
                1,
                10_000.0,
                0.0,
                "seed cash",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_trade(
                "2026-03-06",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "buy",
                10,
                100.0,
                0.0,
                "initial trade before snapshot",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_account_snapshot(
                "2026-03-06T09:30:00Z",
                9_000.0,
                9_000.0,
                None,
                None,
                None,
                None,
                true,
                "manual set",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_trade(
                "2026-03-06",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "sell",
                5,
                110.0,
                0.0,
                "later same-day trade after snapshot",
                "firstrade",
            )
            .unwrap();

        let service = AccountingService::new(&ledger);
        let (trade_date_cash, settled_cash) = service
            .derive_balances_from_origin("firstrade", "2026-03-09")
            .unwrap();

        assert_eq!(trade_date_cash, 9_550.0);
        assert_eq!(settled_cash, 9_550.0);
    }

    #[test]
    fn cash_adjustments_settle_same_day() {
        let ledger = temp_ledger();
        ledger
            .record_trade(
                "2026-03-06",
                "CASH",
                "CASH",
                "stock",
                None,
                None,
                "deposit",
                1,
                10_000.0,
                0.0,
                "seed cash",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_trade(
                "2026-03-06",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "dividend",
                1,
                125.0,
                0.0,
                "cash dividend",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_trade(
                "2026-03-06",
                "CASH",
                "CASH",
                "stock",
                None,
                None,
                "withdraw",
                1,
                40.0,
                0.0,
                "cash withdrawal",
                "firstrade",
            )
            .unwrap();

        let service = AccountingService::new(&ledger);
        let (trade_date_cash, settled_cash) = service
            .derive_balances_from_origin("firstrade", "2026-03-06")
            .unwrap();

        assert_eq!(trade_date_cash, 10_085.0);
        assert_eq!(settled_cash, 10_085.0);
    }

    #[test]
    fn calculate_cash_flow_rejects_unknown_action() {
        let trade = Trade {
            id: 1,
            trade_date: "2026-03-06".to_string(),
            symbol: "TSLA".to_string(),
            underlying: "TSLA".to_string(),
            side: "stock".to_string(),
            strike: None,
            expiry: None,
            action: "mystery".to_string(),
            quantity: 10,
            price: 100.0,
            commission: 0.0,
            notes: String::new(),
            account_id: "firstrade".to_string(),
        };

        let err = AccountingService::calculate_cash_flow(&trade).unwrap_err();
        assert!(
            err.to_string()
                .contains("unsupported trade action in cash-flow replay")
        );
    }

    #[test]
    fn calculate_cash_flow_handles_deposit_and_option_sell() {
        let deposit = Trade {
            id: 1,
            trade_date: "2026-03-05".to_string(),
            symbol: "CASH".to_string(),
            underlying: "CASH".to_string(),
            side: "stock".to_string(),
            strike: None,
            expiry: None,
            action: "deposit".to_string(),
            quantity: 1,
            price: 10_000.0,
            commission: 0.0,
            notes: String::new(),
            account_id: "firstrade".to_string(),
        };
        let short_put_sale = Trade {
            id: 2,
            trade_date: "2026-03-06".to_string(),
            symbol: "TSLA260320P00350000".to_string(),
            underlying: "TSLA".to_string(),
            side: "put".to_string(),
            strike: Some(350.0),
            expiry: Some("2026-03-20".to_string()),
            action: "sell".to_string(),
            quantity: 2,
            price: 5.0,
            commission: 1.25,
            notes: String::new(),
            account_id: "firstrade".to_string(),
        };

        assert_eq!(
            AccountingService::calculate_cash_flow(&deposit).unwrap(),
            10_000.0
        );
        assert_eq!(
            AccountingService::calculate_cash_flow(&short_put_sale).unwrap(),
            998.75
        );
    }
}
