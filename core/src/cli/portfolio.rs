use crate::accounting_service::AccountingService;
use crate::analysis_service::ThetaAnalysisService;
use crate::ledger::{
    AccountMonitorSnapshot, AccountSnapshot, AccountSnapshotInput, Ledger, TradeFilter,
};
use crate::margin_engine::{self, AccountContext};
use crate::portfolio_service;
use crate::risk_domain::EnrichedPosition;
use crate::risk_engine;
use anyhow::{Result, bail};
use clap::{Args, Subcommand};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use time::{Date, format_description::well_known::Rfc3339};

#[derive(Args, Debug)]
#[command(name = "portfolio")]
#[command(about = "Options portfolio trade journal & risk analytics")]
pub struct Cli {
    /// Path to the portfolio database (default: ~/.theta/portfolio.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

    /// Account ID to operate on (default: firstrade)
    #[arg(long, global = true, default_value = "firstrade")]
    account: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Record and inspect account-level buying power snapshots
    Account {
        #[command(subcommand)]
        action: AccountAction,
    },
    /// Record & manage trades
    Trade {
        #[command(subcommand)]
        action: TradeAction,
    },
    /// Show current positions derived from trade history
    Positions {
        #[arg(long, help = "Filter by underlying symbol, e.g. TSLA")]
        underlying: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Identify option strategies from current positions
    Strategies {
        #[arg(long, help = "Filter by underlying symbol")]
        underlying: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Full portfolio risk report: positions, strategies, margin, Greeks
    Report {
        #[arg(long, help = "Filter by underlying symbol")]
        underlying: Option<String>,
        #[arg(long, help = "Skip LongPort connection, use offline data")]
        offline: bool,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum AccountAction {
    /// Set or append a new account snapshot
    Set {
        #[arg(
            long,
            help = "Broker cash balance; also used as the default for trade/settled cash"
        )]
        cash_balance: Option<f64>,
        #[arg(
            long,
            help = "Trade-date cash balance (defaults to cash balance when omitted)"
        )]
        trade_date_cash: Option<f64>,
        #[arg(
            long,
            help = "Settled cash balance (defaults to cash balance when omitted)"
        )]
        settled_cash: Option<f64>,
        #[arg(
            long,
            alias = "cash-buying-power",
            help = "Option or cash buying power"
        )]
        option_buying_power: Option<f64>,
        #[arg(
            long,
            alias = "margin-buying-power",
            help = "Margin / stock buying power"
        )]
        stock_buying_power: Option<f64>,
        #[arg(long, help = "Total account value / net liquidation value")]
        total_account_value: Option<f64>,
        #[arg(long, help = "Long stock market value")]
        long_stock_value: Option<f64>,
        #[arg(long, help = "Long option market value")]
        long_option_value: Option<f64>,
        #[arg(long, help = "Short option market value (typically negative)")]
        short_option_value: Option<f64>,
        #[arg(long, alias = "margin-balance", help = "Margin loan balance")]
        margin_loan: Option<f64>,
        #[arg(long, alias = "short-stock-value", help = "Short stock market value")]
        short_market_value: Option<f64>,
        #[arg(long, default_value_t = true)]
        margin: bool,
    },
    /// Show the latest account snapshot
    Show {
        #[arg(long)]
        json: bool,
    },
    /// List recent account snapshots
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
    /// Rebuild cash balances from the full trade ledger and record a fresh snapshot
    Rebuild {
        #[arg(long, help = "Rebuild balances as of YYYY-MM-DD (default: today)")]
        as_of: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List recent account-monitor snapshots with data quality
    MonitorHistory {
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug)]
enum TradeAction {
    /// Record a new buy trade
    Buy {
        #[arg(
            long,
            help = "Option or stock symbol, e.g. TSLA260320C00400000 or TSLA"
        )]
        symbol: String,
        #[arg(long, help = "Underlying symbol, e.g. TSLA")]
        underlying: String,
        #[arg(long, help = "Number of contracts (options) or shares (stock)")]
        quantity: i64,
        #[arg(long, help = "Price per share or per contract")]
        price: f64,
        #[arg(long, value_enum, help = "Position type: call, put, or stock")]
        side: SideArg,
        #[arg(long, help = "Strike price (required for options)")]
        strike: Option<f64>,
        #[arg(long, help = "Expiry date YYYY-MM-DD (required for options)")]
        expiry: Option<String>,
        #[arg(long, default_value_t = 0.0, help = "Commission / fees")]
        commission: f64,
        #[arg(long, help = "Trade date YYYY-MM-DD (default: today)")]
        date: Option<String>,
        #[arg(long, default_value = "", help = "Notes")]
        notes: String,
    },
    /// Record a new sell trade
    Sell {
        #[arg(long, help = "Option or stock symbol")]
        symbol: String,
        #[arg(long, help = "Underlying symbol")]
        underlying: String,
        #[arg(long, help = "Number of contracts or shares")]
        quantity: i64,
        #[arg(long, help = "Price per share or per contract")]
        price: f64,
        #[arg(long, value_enum, help = "Position type: call, put, or stock")]
        side: SideArg,
        #[arg(long, help = "Strike price (required for options)")]
        strike: Option<f64>,
        #[arg(long, help = "Expiry date YYYY-MM-DD (required for options)")]
        expiry: Option<String>,
        #[arg(long, default_value_t = 0.0, help = "Commission / fees")]
        commission: f64,
        #[arg(long, help = "Trade date YYYY-MM-DD (default: today)")]
        date: Option<String>,
        #[arg(long, default_value = "", help = "Notes")]
        notes: String,
    },
    /// Record a long option exercise (closes the option and books the stock delivery)
    Exercise {
        #[arg(long, help = "Option symbol, e.g. TSLA260320C00400000")]
        symbol: String,
        #[arg(long, help = "Underlying symbol, e.g. TSLA")]
        underlying: String,
        #[arg(long, help = "Number of contracts exercised")]
        quantity: i64,
        #[arg(long, value_enum, help = "Option type")]
        side: OptionSideArg,
        #[arg(long, help = "Strike price")]
        strike: f64,
        #[arg(long, help = "Expiry date YYYY-MM-DD")]
        expiry: String,
        #[arg(long, default_value_t = 0.0, help = "Commission / fees")]
        commission: f64,
        #[arg(long, help = "Event date YYYY-MM-DD (default: today)")]
        date: Option<String>,
        #[arg(long, default_value = "", help = "Notes")]
        notes: String,
    },
    /// Record a short option assignment (closes the option and books the stock delivery)
    Assign {
        #[arg(long, help = "Option symbol, e.g. TSLA260320C00400000")]
        symbol: String,
        #[arg(long, help = "Underlying symbol, e.g. TSLA")]
        underlying: String,
        #[arg(long, help = "Number of contracts assigned")]
        quantity: i64,
        #[arg(long, value_enum, help = "Option type")]
        side: OptionSideArg,
        #[arg(long, help = "Strike price")]
        strike: f64,
        #[arg(long, help = "Expiry date YYYY-MM-DD")]
        expiry: String,
        #[arg(long, default_value_t = 0.0, help = "Commission / fees")]
        commission: f64,
        #[arg(long, help = "Event date YYYY-MM-DD (default: today)")]
        date: Option<String>,
        #[arg(long, default_value = "", help = "Notes")]
        notes: String,
    },
    /// Record an option expiry (closes the option at zero value)
    Expire {
        #[arg(long, help = "Option symbol, e.g. TSLA260320C00400000")]
        symbol: String,
        #[arg(long, help = "Underlying symbol, e.g. TSLA")]
        underlying: String,
        #[arg(long, help = "Number of contracts expired")]
        quantity: i64,
        #[arg(long, value_enum, help = "Option type")]
        side: OptionSideArg,
        #[arg(
            long,
            value_enum,
            help = "Whether the expired option was long or short"
        )]
        position: PositionDirectionArg,
        #[arg(long, help = "Strike price")]
        strike: f64,
        #[arg(long, help = "Expiry date YYYY-MM-DD")]
        expiry: String,
        #[arg(long, default_value_t = 0.0, help = "Commission / fees")]
        commission: f64,
        #[arg(long, help = "Event date YYYY-MM-DD (default: today)")]
        date: Option<String>,
        #[arg(long, default_value = "", help = "Notes")]
        notes: String,
    },
    /// Scan expired open options and settle them as expire/exercise/assignment
    SettleExpiries {
        #[arg(long, help = "Settlement date YYYY-MM-DD (default: today)")]
        date: Option<String>,
        #[arg(long, help = "Filter by underlying symbol")]
        underlying: Option<String>,
        #[arg(
            long = "settlement-price",
            help = "Settlement price mapping, e.g. TSLA=412.50",
            value_name = "SYMBOL=PRICE"
        )]
        settlement_prices: Vec<String>,
        #[arg(long, help = "Write the generated settlement events into the ledger")]
        apply: bool,
        #[arg(long)]
        json: bool,
    },
    /// List trade history
    List {
        #[arg(long, help = "Filter by underlying symbol")]
        underlying: Option<String>,
        #[arg(long, help = "Filter by specific symbol")]
        symbol: Option<String>,
        #[arg(long, help = "Start date YYYY-MM-DD")]
        from: Option<String>,
        #[arg(long, help = "End date YYYY-MM-DD")]
        to: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Delete a trade by ID
    Delete {
        #[arg(help = "Trade ID to delete")]
        id: i64,
    },
    /// Record a cash deposit
    Deposit {
        #[arg(long, help = "Amount deposited")]
        amount: f64,
        #[arg(long, help = "Date YYYY-MM-DD (default: today)")]
        date: Option<String>,
        #[arg(long, default_value = "", help = "Notes")]
        notes: String,
    },
    /// Record a cash withdrawal
    Withdraw {
        #[arg(long, help = "Amount withdrawn")]
        amount: f64,
        #[arg(long, help = "Date YYYY-MM-DD (default: today)")]
        date: Option<String>,
        #[arg(long, default_value = "", help = "Notes")]
        notes: String,
    },
    /// Record a dividend payment
    Dividend {
        #[arg(long, help = "Underlying symbol")]
        underlying: String,
        #[arg(long, help = "Amount received")]
        amount: f64,
        #[arg(long, help = "Date YYYY-MM-DD (default: today)")]
        date: Option<String>,
        #[arg(long, default_value = "", help = "Notes")]
        notes: String,
    },
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum SideArg {
    Call,
    Put,
    Stock,
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum OptionSideArg {
    Call,
    Put,
}

impl OptionSideArg {
    fn as_str(&self) -> &str {
        match self {
            OptionSideArg::Call => "call",
            OptionSideArg::Put => "put",
        }
    }
}

#[derive(Clone, Debug, clap::ValueEnum)]
enum PositionDirectionArg {
    Long,
    Short,
}

#[derive(Debug, Clone, Serialize)]
struct SettlementDecision {
    underlying: String,
    option_symbol: String,
    option_side: String,
    position: String,
    quantity: i64,
    strike: f64,
    expiry: String,
    settlement_date: String,
    settlement_price: f64,
    action: String,
    in_the_money: bool,
    validation_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct UnsettledExpiry {
    underlying: String,
    option_symbol: String,
    expiry: String,
    reason: String,
}

impl SideArg {
    fn as_str(&self) -> &str {
        match self {
            SideArg::Call => "call",
            SideArg::Put => "put",
            SideArg::Stock => "stock",
        }
    }
}

pub async fn run(cli: Cli) -> Result<()> {
    let ledger = match &cli.db {
        Some(path) => Ledger::open(path)?,
        None => Ledger::open_default()?,
    };

    match cli.command {
        Command::Account { action } => handle_account(&ledger, &cli.account, action),
        Command::Trade { action } => match action {
            TradeAction::SettleExpiries {
                date,
                underlying,
                settlement_prices,
                apply,
                json,
            } => {
                handle_settle_expiries(
                    &ledger,
                    &cli.account,
                    date.unwrap_or_else(today),
                    underlying,
                    settlement_prices,
                    apply,
                    json,
                )
                .await
            }
            _ => handle_trade(&ledger, &cli.account, action).await,
        },
        Command::Positions { underlying, json } => {
            handle_positions(&ledger, &cli.account, underlying, json)
        }
        Command::Strategies { underlying, json } => {
            handle_strategies(&ledger, &cli.account, underlying, json).await
        }
        Command::Report {
            underlying,
            offline,
            json,
        } => handle_report(&ledger, &cli.account, underlying, offline, json).await,
    }
}

async fn handle_trade(ledger: &Ledger, account_id: &str, action: TradeAction) -> Result<()> {
    match action {
        TradeAction::Buy {
            symbol,
            underlying,
            quantity,
            price,
            side,
            strike,
            expiry,
            commission,
            date,
            notes,
        } => {
            validate_option_fields(&side, strike, expiry.as_deref())?;
            let trade_date = date.unwrap_or_else(today);
            let ctx =
                fetch_portfolio_enrichment(ledger, account_id, std::slice::from_ref(&underlying))
                    .await;

            ledger.with_transaction(|tx| {
                let id = tx.record_trade(
                    &trade_date,
                    &symbol,
                    &underlying,
                    side.as_str(),
                    strike,
                    expiry.as_deref(),
                    "buy",
                    quantity,
                    price,
                    commission,
                    &notes,
                    account_id,
                )?;
                println!("Recorded BUY trade #{id}: {quantity} \u{00d7} {symbol} @ {price}");
                record_auto_snapshot(tx, account_id, &trade_date, &ctx.enriched, ctx.calendar)?;
                Ok(())
            })?;
            Ok(())
        }
        TradeAction::Sell {
            symbol,
            underlying,
            quantity,
            price,
            side,
            strike,
            expiry,
            commission,
            date,
            notes,
        } => {
            validate_option_fields(&side, strike, expiry.as_deref())?;
            let trade_date = date.unwrap_or_else(today);
            let ctx =
                fetch_portfolio_enrichment(ledger, account_id, std::slice::from_ref(&underlying))
                    .await;

            ledger.with_transaction(|tx| {
                let id = tx.record_trade(
                    &trade_date,
                    &symbol,
                    &underlying,
                    side.as_str(),
                    strike,
                    expiry.as_deref(),
                    "sell",
                    quantity,
                    price,
                    commission,
                    &notes,
                    account_id,
                )?;
                println!("Recorded SELL trade #{id}: {quantity} \u{00d7} {symbol} @ {price}");
                record_auto_snapshot(tx, account_id, &trade_date, &ctx.enriched, ctx.calendar)?;
                Ok(())
            })?;
            Ok(())
        }
        TradeAction::Exercise {
            symbol,
            underlying,
            quantity,
            side,
            strike,
            expiry,
            commission,
            date,
            notes,
        } => {
            let trade_date = date.unwrap_or_else(today);
            ensure_option_position_available(
                ledger,
                account_id,
                &symbol,
                &underlying,
                side.as_str(),
                strike,
                &expiry,
                quantity,
                PositionDirectionArg::Long,
            )?;
            let stock_quantity = quantity * 100;
            let stock_action = match side {
                OptionSideArg::Call => "buy",
                OptionSideArg::Put => "sell",
            };
            let ctx =
                fetch_portfolio_enrichment(ledger, account_id, std::slice::from_ref(&underlying))
                    .await;
            let stock_id = ledger.with_transaction(|tx| {
                record_option_close_event(
                    tx,
                    account_id,
                    &trade_date,
                    &symbol,
                    &underlying,
                    side.as_str(),
                    strike,
                    &expiry,
                    "sell",
                    quantity,
                    commission,
                    event_note("exercise", &notes).as_str(),
                )?;

                let sid = tx.record_adjustment_trade(
                    &trade_date,
                    &underlying,
                    &underlying,
                    "stock",
                    None,
                    None,
                    stock_action,
                    stock_quantity,
                    strike,
                    0.0,
                    event_note("exercise stock delivery", &notes).as_str(),
                    account_id,
                )?;

                record_auto_snapshot(tx, account_id, &trade_date, &ctx.enriched, ctx.calendar)?;
                Ok(sid)
            })?;
            println!(
                "Recorded EXERCISE: closed {quantity} \u{00d7} {symbol} and booked stock leg #{stock_id} ({stock_action} {stock_quantity} {underlying} @ {strike})"
            );
            Ok(())
        }
        TradeAction::Assign {
            symbol,
            underlying,
            quantity,
            side,
            strike,
            expiry,
            commission,
            date,
            notes,
        } => {
            let trade_date = date.unwrap_or_else(today);
            ensure_option_position_available(
                ledger,
                account_id,
                &symbol,
                &underlying,
                side.as_str(),
                strike,
                &expiry,
                quantity,
                PositionDirectionArg::Short,
            )?;
            let stock_quantity = quantity * 100;
            let stock_action = match side {
                OptionSideArg::Call => "sell",
                OptionSideArg::Put => "buy",
            };
            let ctx =
                fetch_portfolio_enrichment(ledger, account_id, std::slice::from_ref(&underlying))
                    .await;
            let stock_id = ledger.with_transaction(|tx| {
                record_option_close_event(
                    tx,
                    account_id,
                    &trade_date,
                    &symbol,
                    &underlying,
                    side.as_str(),
                    strike,
                    &expiry,
                    "buy",
                    quantity,
                    commission,
                    event_note("assignment", &notes).as_str(),
                )?;

                let sid = tx.record_adjustment_trade(
                    &trade_date,
                    &underlying,
                    &underlying,
                    "stock",
                    None,
                    None,
                    stock_action,
                    stock_quantity,
                    strike,
                    0.0,
                    event_note("assignment stock delivery", &notes).as_str(),
                    account_id,
                )?;

                record_auto_snapshot(tx, account_id, &trade_date, &ctx.enriched, ctx.calendar)?;
                Ok(sid)
            })?;
            println!(
                "Recorded ASSIGNMENT: closed {quantity} \u{00d7} {symbol} and booked stock leg #{stock_id} ({stock_action} {stock_quantity} {underlying} @ {strike})"
            );
            Ok(())
        }
        TradeAction::Expire {
            symbol,
            underlying,
            quantity,
            side,
            position,
            strike,
            expiry,
            commission,
            date,
            notes,
        } => {
            let trade_date = date.unwrap_or_else(today);
            ensure_option_position_available(
                ledger,
                account_id,
                &symbol,
                &underlying,
                side.as_str(),
                strike,
                &expiry,
                quantity,
                position.clone(),
            )?;
            let close_action = match position {
                PositionDirectionArg::Long => "sell",
                PositionDirectionArg::Short => "buy",
            };
            let ctx =
                fetch_portfolio_enrichment(ledger, account_id, std::slice::from_ref(&underlying))
                    .await;
            ledger.with_transaction(|tx| {
                let id = record_option_close_event(
                    tx,
                    account_id,
                    &trade_date,
                    &symbol,
                    &underlying,
                    side.as_str(),
                    strike,
                    &expiry,
                    close_action,
                    quantity,
                    commission,
                    event_note("expiry", &notes).as_str(),
                )?;
                println!(
                    "Recorded EXPIRY adjustment #{id}: closed {quantity} \u{00d7} {symbol} at zero value"
                );
                record_auto_snapshot(tx, account_id, &trade_date, &ctx.enriched, ctx.calendar)?;
                Ok(())
            })?;
            Ok(())
        }
        TradeAction::SettleExpiries {
            date,
            underlying,
            settlement_prices,
            apply,
            json,
        } => {
            handle_settle_expiries(
                ledger,
                account_id,
                date.unwrap_or_else(today),
                underlying,
                settlement_prices,
                apply,
                json,
            )
            .await?;
            Ok(())
        }
        TradeAction::List {
            underlying,
            symbol,
            from,
            to,
            json,
        } => {
            let filter = TradeFilter {
                underlying,
                symbol,
                start_date: from,
                end_date: to,
                account_id: Some(account_id.to_string()),
            };
            let trades = ledger.list_trades(&filter)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&trades)?);
            } else if trades.is_empty() {
                println!("No trades found.");
            } else {
                println!(
                    "{:>5}  {:>10}  {:>6}  {:<30}  {:<6}  {:>8}  {:>10}  {:>8}  {:>10}",
                    "ID", "DATE", "ACTION", "SYMBOL", "SIDE", "QTY", "PRICE", "STRIKE", "EXPIRY"
                );
                println!("{}", "-".repeat(105));
                for t in &trades {
                    println!(
                        "{:>5}  {:>10}  {:>6}  {:<30}  {:<6}  {:>8}  {:>10.2}  {:>8}  {:>10}",
                        t.id,
                        t.trade_date,
                        t.action.to_uppercase(),
                        t.symbol,
                        t.side,
                        t.quantity,
                        t.price,
                        t.strike.map(|s| format!("{s:.2}")).unwrap_or_default(),
                        t.expiry.as_deref().unwrap_or(""),
                    );
                }
                println!("\nTotal: {} trades", trades.len());
            }
            Ok(())
        }
        TradeAction::Delete { id } => {
            if ledger.delete_trade(id)? {
                println!("Deleted trade #{id}");
            } else {
                println!("Trade #{id} not found");
            }
            Ok(())
        }
        TradeAction::Deposit {
            amount,
            date,
            notes,
        } => {
            let trade_date = date.unwrap_or_else(today);
            let ctx = fetch_portfolio_enrichment(ledger, account_id, &[]).await;
            ledger.with_transaction(|tx| {
                let id = tx.record_adjustment_trade(
                    &trade_date,
                    "CASH",
                    "CASH",
                    "stock",
                    None,
                    None,
                    "deposit",
                    1,
                    amount,
                    0.0,
                    &notes,
                    account_id,
                )?;
                println!("Recorded DEPOSIT #{id}: ${amount} on {trade_date}");
                record_auto_snapshot(tx, account_id, &trade_date, &ctx.enriched, ctx.calendar)?;
                Ok(())
            })?;
            Ok(())
        }
        TradeAction::Withdraw {
            amount,
            date,
            notes,
        } => {
            let trade_date = date.unwrap_or_else(today);
            let ctx = fetch_portfolio_enrichment(ledger, account_id, &[]).await;
            ledger.with_transaction(|tx| {
                let id = tx.record_adjustment_trade(
                    &trade_date,
                    "CASH",
                    "CASH",
                    "stock",
                    None,
                    None,
                    "withdraw",
                    1,
                    amount,
                    0.0,
                    &notes,
                    account_id,
                )?;
                println!("Recorded WITHDRAWAL #{id}: ${amount} on {trade_date}");
                record_auto_snapshot(tx, account_id, &trade_date, &ctx.enriched, ctx.calendar)?;
                Ok(())
            })?;
            Ok(())
        }
        TradeAction::Dividend {
            underlying,
            amount,
            date,
            notes,
        } => {
            let trade_date = date.unwrap_or_else(today);
            let ctx =
                fetch_portfolio_enrichment(ledger, account_id, std::slice::from_ref(&underlying))
                    .await;
            ledger.with_transaction(|tx| {
                let id = tx.record_adjustment_trade(
                    &trade_date,
                    &underlying,
                    &underlying,
                    "stock",
                    None,
                    None,
                    "dividend",
                    1,
                    amount,
                    0.0,
                    &notes,
                    account_id,
                )?;
                println!("Recorded DIVIDEND #{id}: ${amount} from {underlying} on {trade_date}");
                record_auto_snapshot(tx, account_id, &trade_date, &ctx.enriched, ctx.calendar)?;
                Ok(())
            })?;
            Ok(())
        }
    }
}

fn handle_positions(
    ledger: &Ledger,
    account_id: &str,
    underlying: Option<String>,
    json: bool,
) -> Result<()> {
    let positions = ledger.calculate_positions(account_id, underlying.as_deref())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&positions)?);
        return Ok(());
    }
    if positions.is_empty() {
        println!("No open positions.");
        return Ok(());
    }

    println!(
        "{:<30}  {:<6}  {:<6}  {:>8}  {:>10}  {:>8}  {:>10}",
        "SYMBOL", "SIDE", "DIR", "QTY", "AVG COST", "STRIKE", "EXPIRY"
    );
    println!("{}", "-".repeat(90));
    for p in &positions {
        let dir = if p.net_quantity > 0 { "LONG" } else { "SHORT" };
        println!(
            "{:<30}  {:<6}  {:<6}  {:>8}  {:>10.2}  {:>8}  {:>10}",
            p.symbol,
            p.side,
            dir,
            p.net_quantity,
            p.avg_cost,
            p.strike.map(|s| format!("{s:.2}")).unwrap_or_default(),
            p.expiry.as_deref().unwrap_or(""),
        );
    }
    println!("\nTotal: {} positions", positions.len());
    Ok(())
}

async fn handle_settle_expiries(
    ledger: &Ledger,
    account_id: &str,
    settlement_date: String,
    underlying: Option<String>,
    settlement_prices: Vec<String>,
    apply: bool,
    json: bool,
) -> Result<()> {
    let price_map = parse_settlement_price_map(&settlement_prices)?;
    let positions = ledger.calculate_positions(account_id, underlying.as_deref())?;
    let mut decisions = Vec::new();
    let mut skipped = Vec::new();

    for position in positions
        .iter()
        .filter(|p| matches!(p.side.as_str(), "call" | "put"))
    {
        let expiry = match &position.expiry {
            Some(expiry) if expiry <= &settlement_date => expiry.clone(),
            _ => continue,
        };
        let strike = match position.strike {
            Some(strike) => strike,
            None => continue,
        };
        let settlement_price = match price_map.get(&position.underlying) {
            Some(price) => *price,
            None => {
                skipped.push(UnsettledExpiry {
                    underlying: position.underlying.clone(),
                    option_symbol: position.symbol.clone(),
                    expiry,
                    reason: "missing settlement price".to_string(),
                });
                continue;
            }
        };

        let is_long = position.net_quantity > 0;
        let in_the_money = match position.side.as_str() {
            "call" => settlement_price > strike,
            "put" => settlement_price < strike,
            _ => false,
        };
        let action = if in_the_money {
            if is_long { "exercise" } else { "assignment" }
        } else {
            "expire"
        };

        decisions.push(SettlementDecision {
            underlying: position.underlying.clone(),
            option_symbol: position.symbol.clone(),
            option_side: position.side.clone(),
            position: if is_long {
                "long".to_string()
            } else {
                "short".to_string()
            },
            quantity: position.net_quantity.abs(),
            strike,
            expiry,
            settlement_date: settlement_date.clone(),
            settlement_price,
            action: action.to_string(),
            in_the_money,
            validation_error: None,
        });
    }

    for decision in &mut decisions {
        let validation = ensure_option_position_available(
            ledger,
            account_id,
            &decision.option_symbol,
            &decision.underlying,
            &decision.option_side,
            decision.strike,
            &decision.expiry,
            decision.quantity,
            if decision.position == "long" {
                PositionDirectionArg::Long
            } else {
                PositionDirectionArg::Short
            },
        );
        decision.validation_error = validation.err().map(|e| e.to_string());
    }

    if apply && !skipped.is_empty() {
        let summary = skipped
            .iter()
            .map(|item| format!("{} ({})", item.option_symbol, item.reason))
            .collect::<Vec<_>>()
            .join(", ");
        bail!(
            "refusing to apply partial expiry settlement; missing settlement prices for: {}",
            summary
        );
    }

    if apply {
        for decision in &decisions {
            if let Some(error) = &decision.validation_error {
                bail!(
                    "refusing to apply expiry settlement; {} failed validation: {}",
                    decision.option_symbol,
                    error
                );
            }
        }
        let preview_underlyings: Vec<String> = decisions
            .iter()
            .map(|decision| decision.underlying.clone())
            .collect();
        let ctx = fetch_portfolio_enrichment(ledger, account_id, &preview_underlyings).await;
        ledger.with_transaction(|tx| {
            for decision in &decisions {
                apply_settlement_decision(tx, account_id, decision)?;
            }
            record_auto_snapshot(
                tx,
                account_id,
                &settlement_date,
                &ctx.enriched,
                ctx.calendar,
            )?;
            Ok(())
        })?;
    }

    if json {
        let payload = serde_json::json!({
            "settlement_date": settlement_date,
            "applied": apply,
            "decisions": decisions,
            "skipped": skipped,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    if decisions.is_empty() && skipped.is_empty() {
        println!("No expired open option positions matched the provided settlement prices.");
        return Ok(());
    }

    println!(
        "{} {} settlement decisions for {}",
        if apply { "Applied" } else { "Planned" },
        decisions.len(),
        settlement_date
    );
    println!(
        "{:<8}  {:<28}  {:<5}  {:<5}  {:>4}  {:>8}  {:>10}  {:<10}  {}",
        "UNDERLY", "OPTION", "SIDE", "POS", "QTY", "STRIKE", "SETTLE", "ACTION", "STATUS"
    );
    println!("{}", "-".repeat(126));
    for decision in &decisions {
        println!(
            "{:<8}  {:<28}  {:<5}  {:<5}  {:>4}  {:>8.2}  {:>10.2}  {:<10}  {}",
            decision.underlying,
            decision.option_symbol,
            decision.option_side,
            decision.position,
            decision.quantity,
            decision.strike,
            decision.settlement_price,
            decision.action,
            decision.validation_error.as_deref().unwrap_or("ok"),
        );
    }
    if !skipped.is_empty() {
        println!("\nSkipped:");
        for item in &skipped {
            println!(
                "  {} {} (expiry {}) - {}",
                item.underlying, item.option_symbol, item.expiry, item.reason
            );
        }
    }
    if !apply {
        println!("\nUse --apply to write these settlement events into the ledger.");
    }
    Ok(())
}

fn parse_settlement_price_map(entries: &[String]) -> Result<HashMap<String, f64>> {
    let mut prices = HashMap::new();
    for entry in entries {
        let (symbol, price) = entry.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("invalid --settlement-price `{entry}`; expected SYMBOL=PRICE")
        })?;
        let value: f64 = price
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid settlement price in `{entry}`"))?;
        if value <= 0.0 {
            bail!("settlement price must be positive in `{entry}`");
        }
        prices.insert(symbol.trim().to_string(), value);
    }
    Ok(prices)
}

fn apply_settlement_decision(
    ledger: &Ledger,
    account_id: &str,
    decision: &SettlementDecision,
) -> Result<()> {
    match decision.action.as_str() {
        "exercise" => {
            record_option_close_event(
                ledger,
                account_id,
                &decision.settlement_date,
                &decision.option_symbol,
                &decision.underlying,
                &decision.option_side,
                decision.strike,
                &decision.expiry,
                "sell",
                decision.quantity,
                0.0,
                "auto settlement: exercise",
            )?;
            let stock_action = if decision.option_side == "call" {
                "buy"
            } else {
                "sell"
            };
            ledger.record_adjustment_trade(
                &decision.settlement_date,
                &decision.underlying,
                &decision.underlying,
                "stock",
                None,
                None,
                stock_action,
                decision.quantity * 100,
                decision.strike,
                0.0,
                "auto settlement: exercise stock delivery",
                account_id,
            )?;
        }
        "assignment" => {
            record_option_close_event(
                ledger,
                account_id,
                &decision.settlement_date,
                &decision.option_symbol,
                &decision.underlying,
                &decision.option_side,
                decision.strike,
                &decision.expiry,
                "buy",
                decision.quantity,
                0.0,
                "auto settlement: assignment",
            )?;
            let stock_action = if decision.option_side == "call" {
                "sell"
            } else {
                "buy"
            };
            ledger.record_adjustment_trade(
                &decision.settlement_date,
                &decision.underlying,
                &decision.underlying,
                "stock",
                None,
                None,
                stock_action,
                decision.quantity * 100,
                decision.strike,
                0.0,
                "auto settlement: assignment stock delivery",
                account_id,
            )?;
        }
        "expire" => {
            let close_action = if decision.position == "long" {
                "sell"
            } else {
                "buy"
            };
            record_option_close_event(
                ledger,
                account_id,
                &decision.settlement_date,
                &decision.option_symbol,
                &decision.underlying,
                &decision.option_side,
                decision.strike,
                &decision.expiry,
                close_action,
                decision.quantity,
                0.0,
                "auto settlement: expiry",
            )?;
        }
        other => bail!("unsupported settlement action: {other}"),
    }
    Ok(())
}

fn ensure_option_position_available(
    ledger: &Ledger,
    account_id: &str,
    symbol: &str,
    underlying: &str,
    side: &str,
    strike: f64,
    expiry: &str,
    quantity: i64,
    direction: PositionDirectionArg,
) -> Result<()> {
    let positions = ledger.calculate_positions(account_id, Some(underlying))?;
    let required_sign = match direction {
        PositionDirectionArg::Long => 1,
        PositionDirectionArg::Short => -1,
    };

    let matched = positions.iter().find(|p| {
        p.symbol == symbol
            && p.underlying == underlying
            && p.side == side
            && p.expiry.as_deref() == Some(expiry)
            && p.strike == Some(strike)
            && p.net_quantity.signum() == required_sign
    });

    let Some(position) = matched else {
        bail!(
            "no matching {} option position found for {} ({} {} {:.2} {})",
            if required_sign > 0 { "long" } else { "short" },
            symbol,
            side,
            underlying,
            strike,
            expiry
        );
    };

    if position.net_quantity.abs() < quantity {
        bail!(
            "insufficient open contracts for {}: have {}, need {}",
            symbol,
            position.net_quantity.abs(),
            quantity
        );
    }

    Ok(())
}

fn record_option_close_event(
    ledger: &Ledger,
    account_id: &str,
    trade_date: &str,
    symbol: &str,
    underlying: &str,
    side: &str,
    strike: f64,
    expiry: &str,
    close_action: &str,
    quantity: i64,
    commission: f64,
    notes: &str,
) -> Result<i64> {
    ledger.record_adjustment_trade(
        trade_date,
        symbol,
        underlying,
        side,
        Some(strike),
        Some(expiry),
        close_action,
        quantity,
        0.0,
        commission,
        notes,
        account_id,
    )
}

fn event_note(label: &str, notes: &str) -> String {
    if notes.trim().is_empty() {
        label.to_string()
    } else {
        format!("{label}: {}", notes.trim())
    }
}

fn handle_account(ledger: &Ledger, account_id: &str, action: AccountAction) -> Result<()> {
    match action {
        AccountAction::Set {
            cash_balance,
            trade_date_cash,
            settled_cash,
            option_buying_power,
            stock_buying_power,
            total_account_value,
            long_stock_value,
            long_option_value,
            short_option_value,
            margin_loan,
            short_market_value,
            margin,
        } => {
            let trade_date_cash = trade_date_cash
                .or(cash_balance)
                .ok_or_else(|| anyhow::anyhow!("provide --trade-date-cash or --cash-balance"))?;
            let settled_cash = settled_cash
                .or(cash_balance)
                .ok_or_else(|| anyhow::anyhow!("provide --settled-cash or --cash-balance"))?;
            let snapshot_at = now_rfc3339();
            ledger.record_account_snapshot(&AccountSnapshotInput {
                snapshot_at,
                trade_date_cash,
                settled_cash,
                cash_balance,
                option_buying_power,
                stock_buying_power,
                total_account_value,
                long_stock_value,
                long_option_value,
                short_option_value,
                margin_loan,
                short_market_value,
                margin_enabled: margin,
                notes: "manual set".to_string(),
                account_id: account_id.to_string(),
            })?;
            println!("Account snapshot recorded");
            Ok(())
        }
        AccountAction::Show { json } => {
            let snapshot = ledger.latest_account_snapshot(account_id)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            } else if let Some(snapshot) = snapshot {
                render_account_snapshot(&snapshot);
            } else {
                println!("No account snapshot recorded.");
            }
            Ok(())
        }
        AccountAction::History { limit, json } => {
            let snapshots: Vec<_> = ledger
                .list_account_snapshots(account_id)?
                .into_iter()
                .take(limit)
                .collect();
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshots)?);
            } else if snapshots.is_empty() {
                println!("No account snapshots recorded.");
            } else {
                println!(
                    "{:>5}  {:<25}  {:<8}  {}",
                    "ID", "TIMESTAMP", "MARGIN", "NOTES"
                );
                println!("{}", "-".repeat(90));
                for snapshot in &snapshots {
                    render_account_snapshot(snapshot);
                }
            }
            Ok(())
        }
        AccountAction::MonitorHistory { limit, json } => {
            let snapshots = ledger.list_account_monitor_snapshots(account_id, limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshots)?);
            } else if snapshots.is_empty() {
                println!("No account monitor snapshots recorded.");
            } else {
                println!(
                    "{:>5}  {:<25}  {:<8}  {:<18}  {:>10}  {:>10}  {:>10}  {}",
                    "ID", "CAPTURED", "STATUS", "QUALITY", "EQUITY", "P&L", "MARGIN", "NOTES"
                );
                println!("{}", "-".repeat(120));
                for snapshot in &snapshots {
                    render_account_monitor_snapshot(snapshot);
                }
            }
            Ok(())
        }
        AccountAction::Rebuild { as_of, json } => {
            let as_of_date = as_of.unwrap_or_else(today);
            let service = AccountingService::new(ledger);
            let (trade_date_cash, settled_cash) =
                service.derive_balances_from_origin(account_id, &as_of_date)?;

            let prior_snapshot = ledger.latest_account_snapshot(account_id)?;
            let option_buying_power = prior_snapshot.as_ref().and_then(|s| s.option_buying_power);
            let cash_balance = Some(trade_date_cash);
            let margin_enabled = prior_snapshot
                .as_ref()
                .map(|s| s.margin_enabled)
                .unwrap_or(true);
            let estimated_margin_loan = estimate_margin_loan(settled_cash);
            let stock_buying_power = Some(estimate_stock_buying_power(
                option_buying_power.unwrap_or(0.0),
                margin_enabled,
            ));
            let positions = ledger.calculate_positions(account_id, None)?;
            let market_values = summarize_account_market_values(
                &positions,
                &[],
                trade_date_cash,
                Some(estimated_margin_loan),
            );

            let snapshot_at = now_rfc3339();
            ledger.record_account_snapshot(&AccountSnapshotInput {
                snapshot_at,
                trade_date_cash,
                settled_cash,
                cash_balance,
                option_buying_power,
                stock_buying_power,
                total_account_value: Some(market_values.total_account_value),
                long_stock_value: Some(market_values.long_stock_value),
                long_option_value: Some(market_values.long_option_value),
                short_option_value: Some(market_values.short_option_value),
                margin_loan: Some(estimated_margin_loan),
                short_market_value: Some(market_values.short_market_value),
                margin_enabled,
                notes: format!("rebuilt cash snapshot from ledger as of {as_of_date}"),
                account_id: account_id.to_string(),
            })?;

            let snapshot = ledger
                .latest_account_snapshot(account_id)?
                .ok_or_else(|| anyhow::anyhow!("failed to load rebuilt account snapshot"))?;
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            } else {
                println!("Rebuilt account snapshot at {}", snapshot.snapshot_at);
                render_account_snapshot(&snapshot);
            }
            Ok(())
        }
    }
}

fn validate_option_fields(side: &SideArg, strike: Option<f64>, expiry: Option<&str>) -> Result<()> {
    match side {
        SideArg::Call | SideArg::Put => {
            if strike.is_none() {
                bail!("--strike is required for option trades");
            }
            if expiry.is_none() {
                bail!("--expiry is required for option trades");
            }
        }
        SideArg::Stock => {}
    }
    Ok(())
}

fn today() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}",
        now.year(),
        now.month() as u8,
        now.day()
    )
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| today())
}

fn render_account_snapshot(s: &AccountSnapshot) {
    let derived_estimated = snapshot_uses_estimated_overview(s);
    println!(
        "{:>5}  {:<25}  {:<8}  {}",
        s.id,
        s.snapshot_at,
        if s.margin_enabled { "yes" } else { "no" },
        s.notes
    );
    println!(
        "       {:<25}  Cash: {:>12}  T-Cash: {:>12}  S-Cash: {:>12}",
        "",
        format_optional_money(s.cash_balance),
        format_money(s.trade_date_cash),
        format_money(s.settled_cash),
    );
    if s.option_buying_power.is_some()
        || s.stock_buying_power.is_some()
        || s.total_account_value.is_some()
    {
        println!(
            "       {:<25}  {:<15} {:>9}  {:<15} {:>9}  {:<12} {:>12}",
            "",
            metric_label("Option BP", derived_estimated),
            format_optional_money(s.option_buying_power),
            metric_label("Margin BP", derived_estimated),
            format_optional_money(s.stock_buying_power),
            metric_label("Equity", derived_estimated),
            format_optional_money(s.total_account_value),
        );
    }
    if s.long_stock_value.is_some()
        || s.long_option_value.is_some()
        || s.short_option_value.is_some()
    {
        println!(
            "       {:<25}  {:<18} {:>8}  {:<16} {:>9}  {:<16} {:>9}",
            "",
            metric_label("Long Stock", derived_estimated),
            format_optional_money(s.long_stock_value),
            metric_label("Long Opt", derived_estimated),
            format_optional_money(s.long_option_value),
            metric_label("Short Opt", derived_estimated),
            format_optional_money(s.short_option_value),
        );
    }
    if s.margin_loan.is_some() || s.short_market_value.is_some() {
        println!(
            "       {:<25}  {:<18} {:>7}  {:<18} {:>8}",
            "",
            metric_label("Margin Loan", derived_estimated),
            format_optional_money(s.margin_loan),
            metric_label("Short Stock", derived_estimated),
            format_optional_money(s.short_market_value),
        );
    }
    if let Some(cursor) = s.baseline_trade_id {
        println!("       {:<25}  Baseline trade id: {}", "", cursor);
    }
}

fn render_account_monitor_snapshot(s: &AccountMonitorSnapshot) {
    println!(
        "{:>5}  {:<25}  {:<8}  {:<18}  {:>10}  {:>10}  {:>10}  {}",
        s.id,
        s.captured_at,
        s.status,
        s.data_quality,
        format_optional_money(s.equity_estimate),
        format_optional_money(s.unrealized_pnl),
        format_optional_money(s.total_margin_required),
        s.notes
    );
    if let Some(error_message) = &s.error_message {
        println!("       error: {}", error_message);
    }
}

fn snapshot_uses_estimated_overview(snapshot: &AccountSnapshot) -> bool {
    snapshot.notes.starts_with("auto-update after trade on ")
        || snapshot
            .notes
            .starts_with("rebuilt cash snapshot from ledger as of ")
}

fn metric_label(label: &'static str, estimated: bool) -> &'static str {
    match (label, estimated) {
        ("Cash Balance", true) => "Cash Balance (est)",
        ("Option BP", true) => "Option BP (est)",
        ("Margin BP", true) => "Margin BP (est)",
        ("Equity", true) => "Equity (est)",
        ("Total Equity", true) => "Total Equity (est)",
        ("Long Stock", true) => "Long Stock (est)",
        ("Long Stock MV", true) => "Long Stock MV (est)",
        ("Long Opt", true) => "Long Opt (est)",
        ("Long Option MV", true) => "Long Option MV (est)",
        ("Short Opt", true) => "Short Opt (est)",
        ("Short Option MV", true) => "Short Option MV (est)",
        ("Margin Loan", true) => "Margin Loan (est)",
        ("Short Stock", true) => "Short Stock (est)",
        ("Short Stock MV", true) => "Short Stock MV (est)",
        _ => label,
    }
}

fn format_optional_money(value: Option<f64>) -> String {
    value.map(format_money).unwrap_or_else(|| "-".to_string())
}

fn format_money(value: f64) -> String {
    format!("{value:.2}")
}

async fn handle_strategies(
    ledger: &Ledger,
    account_id: &str,
    underlying: Option<String>,
    json: bool,
) -> Result<()> {
    let positions = ledger.calculate_positions(account_id, underlying.as_deref())?;
    if positions.is_empty() {
        println!("No open positions.");
        return Ok(());
    }

    let strategies = risk_engine::identify_strategies(&positions);
    let account_snapshot = ledger.latest_account_snapshot(account_id)?.ok_or_else(|| {
        anyhow::anyhow!("no account snapshot found; run `portfolio account set ...` first")
    })?;
    let account = AccountContext {
        trade_date_cash: Some(account_snapshot.trade_date_cash),
        settled_cash: Some(account_snapshot.settled_cash),
        option_buying_power: account_snapshot.option_buying_power,
        stock_buying_power: account_snapshot.stock_buying_power,
        margin_loan: account_snapshot.margin_loan,
        short_market_value: account_snapshot.short_market_value,
        margin_enabled: account_snapshot.margin_enabled,
    };
    let margin_positions: Vec<EnrichedPosition> = positions
        .iter()
        .map(|p| EnrichedPosition {
            symbol: p.symbol.clone(),
            underlying: p.underlying.clone(),
            underlying_spot: None,
            side: p.side.clone(),
            strike: p.strike,
            expiry: p.expiry.clone(),
            net_quantity: p.net_quantity,
            avg_cost: p.avg_cost,
            current_price: p.avg_cost,
            unrealized_pnl_per_unit: 0.0,
            unrealized_pnl: 0.0,
            greeks: None,
        })
        .collect();
    let evaluated_strategies =
        margin_engine::evaluate_strategies(&strategies, &margin_positions, &account);

    if json {
        let payload = serde_json::json!({
            "account_snapshot": account_snapshot,
            "strategies": evaluated_strategies,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    println!(
        "Account snapshot: {} | t-cash=${:.2} | s-cash=${:.2} | option bp={}",
        account_snapshot.snapshot_at,
        account_snapshot.trade_date_cash,
        account_snapshot.settled_cash,
        account_snapshot
            .option_buying_power
            .map(|v| format!("${v:.2}"))
            .unwrap_or_else(|| "-".to_string())
    );

    if evaluated_strategies.is_empty() {
        println!("No strategies identified.");
        return Ok(());
    }

    for (i, s) in evaluated_strategies.iter().enumerate() {
        println!("Strategy #{}: {} ({})", i + 1, s.kind, s.underlying);
        for leg in &s.legs {
            let dir = if leg.quantity > 0 { "LONG" } else { "SHORT" };
            println!(
                "  {} {} {} × {} @ {:.2}{}",
                dir,
                leg.side,
                leg.quantity.abs(),
                leg.symbol,
                leg.price,
                leg.strike
                    .map(|s| format!(" K={:.2}", s))
                    .unwrap_or_default(),
            );
        }
        println!(
            "  Margin: ${:.2} ({})",
            s.margin.margin_required, s.margin.method
        );
        if let Some(max_loss) = s.max_loss {
            println!("  Max loss: ${:.2}", max_loss);
        }
        println!();
    }

    let total_margin: f64 = evaluated_strategies
        .iter()
        .map(|s| s.margin.margin_required)
        .sum();
    println!("Total margin required: ${:.2}", total_margin);
    Ok(())
}

async fn handle_report(
    ledger: &Ledger,
    account_id: &str,
    underlying: Option<String>,
    offline: bool,
    json: bool,
) -> Result<()> {
    let positions = ledger.calculate_positions(account_id, underlying.as_deref())?;
    if positions.is_empty() {
        println!("No open positions.");
        return Ok(());
    }
    let account_snapshot = ledger.latest_account_snapshot(account_id)?.ok_or_else(|| {
        anyhow::anyhow!("no account snapshot found; run `portfolio account set ...` first")
    })?;
    let account = AccountContext {
        trade_date_cash: Some(account_snapshot.trade_date_cash),
        settled_cash: Some(account_snapshot.settled_cash),
        option_buying_power: account_snapshot.option_buying_power,
        stock_buying_power: account_snapshot.stock_buying_power,
        margin_loan: account_snapshot.margin_loan,
        short_market_value: account_snapshot.short_market_value,
        margin_enabled: account_snapshot.margin_enabled,
    };

    // Enrich with live data if not offline
    let enriched: Option<Vec<EnrichedPosition>> = if !offline {
        match ThetaAnalysisService::from_env().await {
            Ok(service) => match portfolio_service::enrich_positions(&service, &positions).await {
                Ok(ep) => Some(ep),
                Err(e) => {
                    eprintln!(
                        "Warning: live data enrichment failed: {}. Using offline data.",
                        e
                    );
                    None
                }
            },
            Err(e) => {
                eprintln!(
                    "Warning: LongPort connection failed: {}. Using offline data.",
                    e
                );
                None
            }
        }
    } else {
        None
    };

    let strategies = risk_engine::identify_strategies(&positions);
    let margin_positions: Vec<EnrichedPosition> = enriched.clone().unwrap_or_else(|| {
        positions
            .iter()
            .map(|p| EnrichedPosition {
                symbol: p.symbol.clone(),
                underlying: p.underlying.clone(),
                underlying_spot: None,
                side: p.side.clone(),
                strike: p.strike,
                expiry: p.expiry.clone(),
                net_quantity: p.net_quantity,
                avg_cost: p.avg_cost,
                current_price: p.avg_cost,
                unrealized_pnl_per_unit: 0.0,
                unrealized_pnl: 0.0,
                greeks: None,
            })
            .collect()
    });
    let evaluated_strategies =
        margin_engine::evaluate_strategies(&strategies, &margin_positions, &account);
    let total_margin: f64 = evaluated_strategies
        .iter()
        .map(|s| s.margin.margin_required)
        .sum();

    // Compute portfolio Greeks from enriched data
    let portfolio_greeks = enriched
        .as_ref()
        .map(|ep| risk_engine::aggregate_greeks(ep));

    if json {
        let report = serde_json::json!({
            "positions": enriched.as_ref().map(|e| serde_json::to_value(e).ok()).unwrap_or_else(|| serde_json::to_value(&positions).ok()),
            "strategies": evaluated_strategies,
            "account_snapshot": account_snapshot,
            "portfolio_greeks": portfolio_greeks,
            "total_margin_required": total_margin,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    // -- Account --
    println!("\u{2550}\u{2550}\u{2550} ACCOUNT \u{2550}\u{2550}\u{2550}");
    println!("Snapshot        : {}", account_snapshot.snapshot_at);
    let derived_estimated = snapshot_uses_estimated_overview(&account_snapshot);
    if let Some(cash_balance) = account_snapshot.cash_balance {
        println!(
            "{:<15}: ${cash_balance:.2}",
            metric_label("Cash Balance", derived_estimated)
        );
    }
    println!("Trade-date Cash : ${:.2}", account_snapshot.trade_date_cash);
    println!("Settled Cash    : ${:.2}", account_snapshot.settled_cash);
    println!(
        "{:<15}: {}",
        metric_label("Option BP", derived_estimated),
        account_snapshot
            .option_buying_power
            .map(|v| format!("${v:.2}"))
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "{:<15}: {}",
        metric_label("Margin BP", derived_estimated),
        account_snapshot
            .stock_buying_power
            .map(|v| format!("${v:.2}"))
            .unwrap_or_else(|| "-".to_string())
    );
    if let Some(total_account_value) = account_snapshot.total_account_value {
        println!(
            "{:<15}: ${total_account_value:.2}",
            metric_label("Total Equity", derived_estimated)
        );
    }
    if let Some(long_stock_value) = account_snapshot.long_stock_value {
        println!(
            "{:<15}: ${long_stock_value:.2}",
            metric_label("Long Stock MV", derived_estimated)
        );
    }
    if let Some(long_option_value) = account_snapshot.long_option_value {
        println!(
            "{:<15}: ${long_option_value:.2}",
            metric_label("Long Option MV", derived_estimated)
        );
    }
    if let Some(short_option_value) = account_snapshot.short_option_value {
        println!(
            "{:<15}: ${short_option_value:.2}",
            metric_label("Short Option MV", derived_estimated)
        );
    }
    if let Some(margin_loan) = account_snapshot.margin_loan {
        println!(
            "{:<15}: ${margin_loan:.2}",
            metric_label("Margin Loan", derived_estimated)
        );
    }
    if let Some(short_market_value) = account_snapshot.short_market_value {
        println!(
            "{:<15}: ${short_market_value:.2}",
            metric_label("Short Stock MV", derived_estimated)
        );
    }
    println!(
        "Margin enabled  : {}",
        if account_snapshot.margin_enabled {
            "yes"
        } else {
            "no"
        }
    );
    if !account_snapshot.notes.is_empty() {
        println!("Notes           : {}", account_snapshot.notes);
    }

    // -- Positions --
    if let Some(ref ep) = enriched {
        println!("\u{2550}\u{2550}\u{2550} POSITIONS (LIVE) \u{2550}\u{2550}\u{2550}");
        println!(
            "{:<26}  {:<6}  {:<6}  {:>5}  {:>9}  {:>9}  {:>10}  {:>8}  {:>10}",
            "SYMBOL", "SIDE", "DIR", "QTY", "AVG COST", "PRICE", "P&L", "STRIKE", "EXPIRY"
        );
        println!("{}", "\u{2500}".repeat(105));
        for p in ep {
            let dir = if p.net_quantity > 0 { "LONG" } else { "SHORT" };
            println!(
                "{:<26}  {:<6}  {:<6}  {:>5}  {:>9.2}  {:>9.2}  {:>10.2}  {:>8}  {:>10}",
                p.symbol,
                p.side,
                dir,
                p.net_quantity,
                p.avg_cost,
                p.current_price,
                p.unrealized_pnl,
                p.strike.map(|s| format!("{s:.2}")).unwrap_or_default(),
                p.expiry.as_deref().unwrap_or(""),
            );
        }
        let total_pnl: f64 = ep.iter().map(|p| p.unrealized_pnl).sum();
        println!("\nTotal unrealized P&L: ${:.2}", total_pnl);
    } else {
        println!("\u{2550}\u{2550}\u{2550} POSITIONS (OFFLINE) \u{2550}\u{2550}\u{2550}");
        println!(
            "{:<30}  {:<6}  {:<6}  {:>8}  {:>10}  {:>8}  {:>10}",
            "SYMBOL", "SIDE", "DIR", "QTY", "AVG COST", "STRIKE", "EXPIRY"
        );
        println!("{}", "\u{2500}".repeat(90));
        for p in &positions {
            let dir = if p.net_quantity > 0 { "LONG" } else { "SHORT" };
            println!(
                "{:<30}  {:<6}  {:<6}  {:>8}  {:>10.2}  {:>8}  {:>10}",
                p.symbol,
                p.side,
                dir,
                p.net_quantity,
                p.avg_cost,
                p.strike.map(|s| format!("{s:.2}")).unwrap_or_default(),
                p.expiry.as_deref().unwrap_or(""),
            );
        }
    }

    // -- Strategies --
    println!("\n\u{2550}\u{2550}\u{2550} STRATEGIES \u{2550}\u{2550}\u{2550}");
    if evaluated_strategies.is_empty() {
        println!("No strategies identified.");
    } else {
        for (i, s) in evaluated_strategies.iter().enumerate() {
            println!("{}. {} ({})", i + 1, s.kind, s.underlying);
            for leg in &s.legs {
                let dir = if leg.quantity > 0 { "LONG" } else { "SHORT" };
                println!(
                    "   {} {} {} \u{00d7} {}{}",
                    dir,
                    leg.side,
                    leg.quantity.abs(),
                    leg.symbol,
                    leg.strike
                        .map(|s| format!(" K={:.2}", s))
                        .unwrap_or_default(),
                );
            }
            println!("   Margin: ${:.2}", s.margin.margin_required);
            if let Some(ml) = s.max_loss {
                println!("   Max loss: ${:.2}", ml);
            }
        }
    }

    // -- Greeks --
    if let Some(ref g) = portfolio_greeks {
        println!("\n\u{2550}\u{2550}\u{2550} PORTFOLIO GREEKS \u{2550}\u{2550}\u{2550}");
        println!("Net Delta (shares) : {:>+.2}", g.net_delta_shares);
        println!("Total Gamma        : {:>+.4}", g.total_gamma);
        println!("Total Theta/day ($): {:>+.2}", g.total_theta_per_day);
        println!("Total Vega         : {:>+.2}", g.total_vega);
    }

    // -- Summary --
    println!("\n\u{2550}\u{2550}\u{2550} SUMMARY \u{2550}\u{2550}\u{2550}");
    println!("Positions       : {}", positions.len());
    println!("Strategies      : {}", evaluated_strategies.len());
    println!("Total margin    : ${:.2}", total_margin);
    if let Some(ref ep) = enriched {
        let total_pnl: f64 = ep.iter().map(|p| p.unrealized_pnl).sum();
        println!("Unrealized P&L  : ${:.2}", total_pnl);
    }

    Ok(())
}

struct MarketContext {
    enriched: Vec<EnrichedPosition>,
    calendar: Option<Vec<Date>>,
}

async fn fetch_portfolio_enrichment(
    ledger: &Ledger,
    account_id: &str,
    preview_underlyings: &[String],
) -> MarketContext {
    let positions = load_positions_for_enrichment(ledger, account_id, preview_underlyings);

    let Ok(svc) = ThetaAnalysisService::from_env().await else {
        return MarketContext {
            enriched: Vec::new(),
            calendar: None,
        };
    };

    let enriched = match portfolio_service::enrich_positions(&svc, &positions).await {
        Ok(enriched) => enriched,
        Err(err) => {
            eprintln!("Warning: live portfolio enrichment unavailable: {err}");
            Vec::new()
        }
    };

    // Fetch trading days for settlement logic (e.g., current month and surrounding)
    let today = time::OffsetDateTime::now_utc().date();
    let start = today - time::Duration::days(60); // Cover historical trades in recent history
    let end = today + time::Duration::days(30);

    let calendar = svc
        .market()
        .fetch_trading_days(crate::market_data::Market::US, start, end)
        .await
        .ok();

    MarketContext { enriched, calendar }
}

fn load_positions_for_enrichment(
    ledger: &Ledger,
    account_id: &str,
    preview_underlyings: &[String],
) -> Vec<crate::ledger::Position> {
    let mut positions = ledger
        .calculate_positions(account_id, None)
        .unwrap_or_default();
    let mut existing_underlyings: std::collections::HashSet<String> = positions
        .iter()
        .map(|position| position.underlying.clone())
        .collect();

    for underlying in preview_underlyings {
        let trimmed = underlying.trim();
        if trimmed.is_empty() || existing_underlyings.contains(trimmed) {
            continue;
        }

        positions.push(crate::ledger::Position {
            symbol: trimmed.to_string(),
            underlying: trimmed.to_string(),
            side: "stock".to_string(),
            strike: None,
            expiry: None,
            net_quantity: 0,
            avg_cost: 0.0,
            total_cost: 0.0,
        });
        existing_underlyings.insert(trimmed.to_string());
    }

    positions
}

fn record_auto_snapshot(
    ledger: &Ledger,
    account_id: &str,
    trade_date: &str,
    enriched: &[EnrichedPosition],
    trading_calendar: Option<Vec<Date>>,
) -> Result<()> {
    let mut service = AccountingService::new(ledger);
    if let Some(calendar) = trading_calendar {
        service = service.with_calendar(calendar);
    }
    let (trade_cash, settled_cash) = service.derive_balances(account_id, trade_date)?;

    let last_snapshot = ledger.latest_account_snapshot(account_id)?;
    let margin_enabled = last_snapshot
        .as_ref()
        .map(|s| s.margin_enabled)
        .unwrap_or(true);
    let estimated_margin_loan = estimate_margin_loan(settled_cash);

    let positions = ledger.calculate_positions(account_id, None)?;
    let strategies = risk_engine::identify_strategies(&positions);
    let market_values = summarize_account_market_values(
        &positions,
        enriched,
        trade_cash,
        Some(estimated_margin_loan),
    );

    let account_ctx = AccountContext {
        trade_date_cash: Some(trade_cash),
        settled_cash: Some(settled_cash),
        option_buying_power: None,
        stock_buying_power: None,
        margin_loan: Some(estimated_margin_loan),
        short_market_value: Some(market_values.short_market_value),
        margin_enabled,
    };

    let evaluated = margin_engine::evaluate_strategies(&strategies, enriched, &account_ctx);
    let total_margin: f64 = evaluated.iter().map(|s| s.margin.margin_required).sum();
    let computed_buying_power = estimate_option_buying_power(settled_cash, total_margin);
    let estimated_stock_buying_power =
        estimate_stock_buying_power(computed_buying_power, margin_enabled);

    let snapshot_at = now_rfc3339();
    ledger.record_account_snapshot(&AccountSnapshotInput {
        snapshot_at,
        trade_date_cash: trade_cash,
        settled_cash,
        cash_balance: Some(trade_cash),
        option_buying_power: Some(computed_buying_power),
        stock_buying_power: Some(estimated_stock_buying_power),
        total_account_value: Some(market_values.total_account_value),
        long_stock_value: Some(market_values.long_stock_value),
        long_option_value: Some(market_values.long_option_value),
        short_option_value: Some(market_values.short_option_value),
        margin_loan: Some(estimated_margin_loan),
        short_market_value: Some(market_values.short_market_value),
        margin_enabled,
        notes: format!("auto-update after trade on {}", trade_date),
        account_id: account_id.to_string(),
    })?;
    println!(
        "Auto-updated account snapshot: T-Cash=${:.2}, S-Cash=${:.2}, Buying Power=${:.2}",
        trade_cash, settled_cash, computed_buying_power
    );
    Ok(())
}

fn estimate_option_buying_power(settled_cash: f64, total_margin: f64) -> f64 {
    (settled_cash - total_margin).max(0.0)
}

fn estimate_stock_buying_power(option_buying_power: f64, margin_enabled: bool) -> f64 {
    if margin_enabled {
        (option_buying_power.max(0.0)) * 2.0
    } else {
        option_buying_power.max(0.0)
    }
}

fn estimate_margin_loan(settled_cash: f64) -> f64 {
    (-settled_cash).max(0.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct AccountMarketValueSummary {
    total_account_value: f64,
    long_stock_value: f64,
    short_market_value: f64,
    long_option_value: f64,
    short_option_value: f64,
}

fn summarize_account_market_values(
    positions: &[crate::ledger::Position],
    enriched: &[EnrichedPosition],
    cash_balance: f64,
    margin_loan: Option<f64>,
) -> AccountMarketValueSummary {
    let live_prices: HashMap<&str, f64> = enriched
        .iter()
        .map(|position| (position.symbol.as_str(), position.current_price))
        .collect();

    let mut long_stock_value = 0.0;
    let mut short_market_value = 0.0;
    let mut long_option_value = 0.0;
    let mut short_option_value = 0.0;

    for position in positions {
        let multiplier = if position.side == "stock" { 1.0 } else { 100.0 };
        let current_price = live_prices
            .get(position.symbol.as_str())
            .copied()
            .unwrap_or(position.avg_cost);
        let market_value = current_price * position.net_quantity.unsigned_abs() as f64 * multiplier;

        match (position.side.as_str(), position.net_quantity.signum()) {
            ("stock", 1) => long_stock_value += market_value,
            ("stock", -1) => short_market_value += market_value,
            (_, 1) => long_option_value += market_value,
            (_, -1) => short_option_value -= market_value,
            _ => {}
        }
    }

    let total_account_value = cash_balance + long_stock_value + long_option_value
        - short_market_value
        + short_option_value
        - margin_loan.unwrap_or(0.0);

    AccountMarketValueSummary {
        total_account_value,
        long_stock_value,
        short_market_value,
        long_option_value,
        short_option_value,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        estimate_margin_loan, estimate_option_buying_power, estimate_stock_buying_power,
        load_positions_for_enrichment, metric_label, snapshot_uses_estimated_overview,
        summarize_account_market_values,
    };
    use crate::ledger::{AccountSnapshot, Ledger, Position};
    use crate::risk_domain::EnrichedPosition;
    use tempfile::tempdir;

    fn temp_ledger() -> Ledger {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("portfolio.db");
        let ledger = Ledger::open(&db_path).unwrap();
        std::mem::forget(dir);
        ledger
    }

    #[test]
    fn estimated_option_buying_power_uses_settled_cash() {
        let option_bp = estimate_option_buying_power(5_000.0, 1_200.0);
        assert!((option_bp - 3_800.0).abs() < 0.0001);
    }

    #[test]
    fn estimated_option_buying_power_clamps_at_zero() {
        let option_bp = estimate_option_buying_power(500.0, 1_200.0);
        assert_eq!(option_bp, 0.0);
    }

    #[test]
    fn estimated_stock_buying_power_doubles_option_buying_power_for_margin_accounts() {
        let stock_bp = estimate_stock_buying_power(13_632.92, true);
        assert!((stock_bp - 27_265.84).abs() < 0.0001);
    }

    #[test]
    fn estimated_stock_buying_power_matches_cash_buying_power_without_margin() {
        let stock_bp = estimate_stock_buying_power(5_000.0, false);
        assert_eq!(stock_bp, 5_000.0);
    }

    #[test]
    fn estimated_margin_loan_clamps_negative_settled_cash() {
        assert_eq!(estimate_margin_loan(-2_500.0), 2_500.0);
        assert_eq!(estimate_margin_loan(100.0), 0.0);
    }

    #[test]
    fn load_positions_for_enrichment_uses_requested_account() {
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
                "",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_trade(
                "2026-03-06",
                "AAPL",
                "AAPL",
                "stock",
                None,
                None,
                "buy",
                5,
                200.0,
                0.0,
                "",
                "other",
            )
            .unwrap();

        let firstrade_positions = load_positions_for_enrichment(&ledger, "firstrade", &[]);
        let other_positions = load_positions_for_enrichment(&ledger, "other", &[]);

        assert_eq!(firstrade_positions.len(), 1);
        assert_eq!(firstrade_positions[0].symbol, "TSLA");
        assert_eq!(other_positions.len(), 1);
        assert_eq!(other_positions[0].symbol, "AAPL");
    }

    #[test]
    fn load_positions_for_enrichment_includes_preview_underlyings() {
        let ledger = temp_ledger();
        let preview_underlyings = vec!["TSLA".to_string(), "QQQ".to_string()];

        let positions = load_positions_for_enrichment(&ledger, "firstrade", &preview_underlyings);

        assert_eq!(positions.len(), 2);
        assert!(
            positions
                .iter()
                .any(|position| position.underlying == "TSLA")
        );
        assert!(
            positions
                .iter()
                .any(|position| position.underlying == "QQQ")
        );
        assert!(positions.iter().all(|position| position.net_quantity == 0));
    }

    #[test]
    fn summarize_account_market_values_uses_live_prices_and_current_quantities() {
        let positions = vec![
            Position {
                symbol: "TSLA".to_string(),
                underlying: "TSLA".to_string(),
                side: "stock".to_string(),
                strike: None,
                expiry: None,
                net_quantity: 10,
                avg_cost: 100.0,
                total_cost: 1_000.0,
            },
            Position {
                symbol: "TSLA260320C00400000".to_string(),
                underlying: "TSLA".to_string(),
                side: "call".to_string(),
                strike: Some(400.0),
                expiry: Some("2026-03-20".to_string()),
                net_quantity: 1,
                avg_cost: 5.0,
                total_cost: 500.0,
            },
            Position {
                symbol: "TSLA260320P00380000".to_string(),
                underlying: "TSLA".to_string(),
                side: "put".to_string(),
                strike: Some(380.0),
                expiry: Some("2026-03-20".to_string()),
                net_quantity: -2,
                avg_cost: 4.0,
                total_cost: 800.0,
            },
        ];
        let enriched = vec![
            EnrichedPosition {
                symbol: "TSLA".to_string(),
                underlying: "TSLA".to_string(),
                underlying_spot: Some(220.0),
                side: "stock".to_string(),
                strike: None,
                expiry: None,
                net_quantity: 10,
                avg_cost: 100.0,
                current_price: 220.0,
                unrealized_pnl_per_unit: 120.0,
                unrealized_pnl: 1_200.0,
                greeks: None,
            },
            EnrichedPosition {
                symbol: "TSLA260320C00400000".to_string(),
                underlying: "TSLA".to_string(),
                underlying_spot: Some(220.0),
                side: "call".to_string(),
                strike: Some(400.0),
                expiry: Some("2026-03-20".to_string()),
                net_quantity: 1,
                avg_cost: 5.0,
                current_price: 2.5,
                unrealized_pnl_per_unit: -2.5,
                unrealized_pnl: -250.0,
                greeks: None,
            },
            EnrichedPosition {
                symbol: "TSLA260320P00380000".to_string(),
                underlying: "TSLA".to_string(),
                underlying_spot: Some(220.0),
                side: "put".to_string(),
                strike: Some(380.0),
                expiry: Some("2026-03-20".to_string()),
                net_quantity: -2,
                avg_cost: 4.0,
                current_price: 1.25,
                unrealized_pnl_per_unit: 2.75,
                unrealized_pnl: 550.0,
                greeks: None,
            },
        ];

        let summary = summarize_account_market_values(&positions, &enriched, 1_000.0, Some(50.0));

        assert_eq!(summary.long_stock_value, 2_200.0);
        assert_eq!(summary.long_option_value, 250.0);
        assert_eq!(summary.short_option_value, -250.0);
        assert_eq!(summary.short_market_value, 0.0);
        assert_eq!(summary.total_account_value, 3_150.0);
    }

    #[test]
    fn summarize_account_market_values_falls_back_to_average_cost_without_live_prices() {
        let positions = vec![Position {
            symbol: "TSLA260320P00380000".to_string(),
            underlying: "TSLA".to_string(),
            side: "put".to_string(),
            strike: Some(380.0),
            expiry: Some("2026-03-20".to_string()),
            net_quantity: -1,
            avg_cost: 3.2,
            total_cost: 320.0,
        }];

        let summary = summarize_account_market_values(&positions, &[], 500.0, None);

        assert_eq!(summary.long_stock_value, 0.0);
        assert_eq!(summary.long_option_value, 0.0);
        assert_eq!(summary.short_option_value, -320.0);
        assert_eq!(summary.short_market_value, 0.0);
        assert_eq!(summary.total_account_value, 180.0);
    }

    #[test]
    fn auto_snapshots_mark_overview_fields_as_estimated() {
        let snapshot = AccountSnapshot {
            id: 1,
            account_id: "firstrade".to_string(),
            snapshot_at: "2026-03-10T00:00:00Z".to_string(),
            trade_date_cash: 100.0,
            settled_cash: 100.0,
            option_buying_power: Some(100.0),
            stock_buying_power: Some(200.0),
            margin_enabled: true,
            margin_loan: Some(0.0),
            short_market_value: Some(0.0),
            notes: "auto-update after trade on 2026-03-10".to_string(),
            baseline_trade_id: None,
            cash_balance: Some(100.0),
            total_account_value: Some(100.0),
            long_stock_value: Some(0.0),
            long_option_value: Some(0.0),
            short_option_value: Some(0.0),
        };

        assert!(snapshot_uses_estimated_overview(&snapshot));
        assert_eq!(metric_label("Margin BP", true), "Margin BP (est)");
        assert_eq!(metric_label("Margin Loan", true), "Margin Loan (est)");
    }

    #[test]
    fn manual_snapshots_keep_broker_labels_plain() {
        let snapshot = AccountSnapshot {
            id: 1,
            account_id: "firstrade".to_string(),
            snapshot_at: "2026-03-10T00:00:00Z".to_string(),
            trade_date_cash: 100.0,
            settled_cash: 100.0,
            option_buying_power: Some(100.0),
            stock_buying_power: Some(200.0),
            margin_enabled: true,
            margin_loan: Some(0.0),
            short_market_value: Some(0.0),
            notes: "manual set".to_string(),
            baseline_trade_id: None,
            cash_balance: Some(100.0),
            total_account_value: Some(100.0),
            long_stock_value: Some(0.0),
            long_option_value: Some(0.0),
            short_option_value: Some(0.0),
        };

        assert!(!snapshot_uses_estimated_overview(&snapshot));
        assert_eq!(metric_label("Margin BP", false), "Margin BP");
        assert_eq!(metric_label("Margin Loan", false), "Margin Loan");
    }
}
