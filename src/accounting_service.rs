use anyhow::{Context, Result};
use crate::ledger::{Ledger, Trade, TradeFilter};
use time::{Date, Duration, Weekday};
use std::collections::HashSet;

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
                break;
            }
        }
        
        Ok(settled.format(&format)?)
    }

    /// Calculate the cash flow of a single trade.
    pub fn calculate_cash_flow(trade: &Trade) -> f64 {
        match trade.action.as_str() {
            "deposit" => trade.price * trade.quantity as f64,
            "withdraw" => -trade.price * trade.quantity as f64,
            "dividend" => trade.price * trade.quantity as f64, // dividend is a positive flow
            "buy" => {
                let multiplier = if trade.side == "stock" { 1.0 } else { 100.0 };
                let principal = trade.price * trade.quantity as f64 * multiplier;
                -principal - trade.commission
            }
            "sell" => {
                let multiplier = if trade.side == "stock" { 1.0 } else { 100.0 };
                let principal = trade.price * trade.quantity as f64 * multiplier;
                principal - trade.commission
            }
            _ => 0.0,
        }
    }

    /// Derive the current account balances (trade-date and settled) based on all historical trades
    /// and a starting base snapshot if provided.
    pub fn derive_balances(&self, account_id: &str, as_of_date: &str) -> Result<(f64, f64)> {
        // Use latest manual snapshot as a checkpoint if available
        let checkpoint = self.ledger.latest_manual_snapshot(account_id)?;
        
        let (mut trade_date_total, mut settled_total, start_date) = if let Some(cp) = checkpoint {
            // We assume the checkpoint represents the state AT the end of cp.snapshot_at/trade_date
            // But wait, snapshots have their own timestamp. Let's use the date from snapshot_at if it's "YYYY-MM-DD..."
            let cp_date = &cp.snapshot_at[..10]; 
            (cp.trade_date_cash, cp.settled_cash, Some(cp_date.to_string()))
        } else {
            (0.0, 0.0, None)
        };

        let filter = TradeFilter {
            account_id: Some(account_id.to_string()),
            ..Default::default()
        };
        let trades = self.ledger.list_trades(&filter)?;
        
        for trade in trades {
            // Only process trades strictly AFTER the checkpoint date (if any)
            if let Some(ref start) = start_date {
                if trade.trade_date <= *start {
                    continue;
                }
            }

            let flow = Self::calculate_cash_flow(&trade);
            let settlement_date = self.get_settlement_date(&trade.trade_date)?;
            
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
}

fn is_weekend(date: Date) -> bool {
    matches!(date.weekday(), Weekday::Saturday | Weekday::Sunday)
}
