use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use std::collections::HashMap;
use theta::analysis_service::ThetaAnalysisService;
use theta::ledger::{AccountSnapshot, Ledger, TradeFilter};
use theta::margin_engine::{self, AccountContext};
use theta::portfolio_service;
use theta::risk_engine;
use theta::risk_domain::EnrichedPosition;
use std::path::PathBuf;
use time::format_description::well_known::Rfc3339;

#[derive(Parser, Debug)]
#[command(name = "portfolio")]
#[command(about = "Options portfolio trade journal & risk analytics")]
struct Cli {
    /// Path to the portfolio database (default: ~/.theta/portfolio.db)
    #[arg(long, global = true)]
    db: Option<PathBuf>,

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
        #[arg(long, help = "Cash balance available in the account")]
        cash_balance: f64,
        #[arg(long, help = "Option buying power")]
        option_buying_power: Option<f64>,
        #[arg(long, help = "Treat snapshot as a cash account (no margin enabled)")]
        cash_account: bool,
        #[arg(long, help = "Snapshot timestamp in RFC3339; default: now")]
        at: Option<String>,
        #[arg(long, default_value = "", help = "Notes")]
        notes: String,
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
}

#[derive(Subcommand, Debug)]
enum TradeAction {
    /// Record a new buy trade
    Buy {
        #[arg(long, help = "Option or stock symbol, e.g. TSLA260320C00400000 or TSLA")]
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
        #[arg(long, value_enum, help = "Whether the expired option was long or short")]
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let ledger = match &cli.db {
        Some(path) => Ledger::open(path)?,
        None => Ledger::open_default()?,
    };

    match cli.command {
        Command::Account { action } => handle_account(&ledger, action),
        Command::Trade { action } => handle_trade(&ledger, action),
        Command::Positions { underlying, json } => handle_positions(&ledger, underlying, json),
        Command::Strategies { underlying, json } => {
            handle_strategies(&ledger, underlying, json).await
        }
        Command::Report { underlying, offline, json } => {
            handle_report(&ledger, underlying, offline, json).await
        }
    }
}

fn handle_trade(ledger: &Ledger, action: TradeAction) -> Result<()> {
    match action {
        TradeAction::Buy {
            symbol, underlying, quantity, price, side,
            strike, expiry, commission, date, notes,
        } => {
            validate_option_fields(&side, strike, expiry.as_deref())?;
            let trade_date = date.unwrap_or_else(today);
            let id = ledger.record_trade(
                &trade_date, &symbol, &underlying, side.as_str(),
                strike, expiry.as_deref(), "buy",
                quantity, price, commission, &notes,
            )?;
            println!("Recorded BUY trade #{id}: {quantity} × {symbol} @ {price}");
            Ok(())
        }
        TradeAction::Sell {
            symbol, underlying, quantity, price, side,
            strike, expiry, commission, date, notes,
        } => {
            validate_option_fields(&side, strike, expiry.as_deref())?;
            let trade_date = date.unwrap_or_else(today);
            let id = ledger.record_trade(
                &trade_date, &symbol, &underlying, side.as_str(),
                strike, expiry.as_deref(), "sell",
                quantity, price, commission, &notes,
            )?;
            println!("Recorded SELL trade #{id}: {quantity} × {symbol} @ {price}");
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
            let stock_id = ledger.with_transaction(|tx| {
                record_option_close_event(
                    tx,
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

                tx.record_adjustment_trade(
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
                )
            })?;
            println!(
                "Recorded EXERCISE: closed {quantity} × {symbol} and booked stock leg #{stock_id} ({stock_action} {stock_quantity} {underlying} @ {strike})"
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
            let stock_id = ledger.with_transaction(|tx| {
                record_option_close_event(
                    tx,
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

                tx.record_adjustment_trade(
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
                )
            })?;
            println!(
                "Recorded ASSIGNMENT: closed {quantity} × {symbol} and booked stock leg #{stock_id} ({stock_action} {stock_quantity} {underlying} @ {strike})"
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
            let id = record_option_close_event(
                ledger,
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
                "Recorded EXPIRY adjustment #{id}: closed {quantity} × {symbol} at zero value"
            );
            Ok(())
        }
        TradeAction::SettleExpiries {
            date,
            underlying,
            settlement_prices,
            apply,
            json,
        } => handle_settle_expiries(
            ledger,
            date.unwrap_or_else(today),
            underlying,
            settlement_prices,
            apply,
            json,
        ),
        TradeAction::List { underlying, symbol, from, to, json } => {
            let filter = TradeFilter {
                underlying,
                symbol,
                from_date: from,
                to_date: to,
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
    }
}

fn handle_positions(ledger: &Ledger, underlying: Option<String>, json: bool) -> Result<()> {
    let positions = ledger.calculate_positions(underlying.as_deref())?;
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

fn handle_settle_expiries(
    ledger: &Ledger,
    settlement_date: String,
    underlying: Option<String>,
    settlement_prices: Vec<String>,
    apply: bool,
    json: bool,
) -> Result<()> {
    let price_map = parse_settlement_price_map(&settlement_prices)?;
    let positions = ledger.calculate_positions(underlying.as_deref())?;
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
            position: if is_long { "long".to_string() } else { "short".to_string() },
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
        ledger.with_transaction(|tx| {
            for decision in &decisions {
                apply_settlement_decision(tx, decision)?;
            }
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
            decision
                .validation_error
                .as_deref()
                .unwrap_or("ok"),
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
        let (symbol, price) = entry
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid --settlement-price `{entry}`; expected SYMBOL=PRICE"))?;
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

fn apply_settlement_decision(ledger: &Ledger, decision: &SettlementDecision) -> Result<()> {
    match decision.action.as_str() {
        "exercise" => {
            record_option_close_event(
                ledger,
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
            let stock_action = if decision.option_side == "call" { "buy" } else { "sell" };
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
            )?;
        }
        "assignment" => {
            record_option_close_event(
                ledger,
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
            let stock_action = if decision.option_side == "call" { "sell" } else { "buy" };
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
            )?;
        }
        "expire" => {
            let close_action = if decision.position == "long" { "sell" } else { "buy" };
            record_option_close_event(
                ledger,
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
    symbol: &str,
    underlying: &str,
    side: &str,
    strike: f64,
    expiry: &str,
    quantity: i64,
    direction: PositionDirectionArg,
) -> Result<()> {
    let positions = ledger.calculate_positions(Some(underlying))?;
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
    )
}

fn event_note(label: &str, notes: &str) -> String {
    if notes.trim().is_empty() {
        label.to_string()
    } else {
        format!("{label}: {}", notes.trim())
    }
}

fn handle_account(ledger: &Ledger, action: AccountAction) -> Result<()> {
    match action {
        AccountAction::Set {
            cash_balance,
            option_buying_power,
            cash_account,
            at,
            notes,
        } => {
            let snapshot_at = at.unwrap_or_else(now_rfc3339);
            let id = ledger.record_account_snapshot(
                &snapshot_at,
                cash_balance,
                option_buying_power,
                !cash_account,
                &notes,
            )?;
            println!(
                "Recorded account snapshot #{id} at {snapshot_at}: cash=${cash_balance:.2}{}",
                option_buying_power
                    .map(|obp| format!(", option buying power=${obp:.2}"))
                    .unwrap_or_default(),
            );
            Ok(())
        }
        AccountAction::Show { json } => {
            let snapshot = ledger.latest_account_snapshot()?;
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
            let snapshots = ledger.list_account_snapshots(limit)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshots)?);
            } else if snapshots.is_empty() {
                println!("No account snapshots recorded.");
            } else {
                println!(
                    "{:>5}  {:<25}  {:>14}  {:>18}  {:<8}  {}",
                    "ID", "TIMESTAMP", "CASH", "OPTION BP", "MARGIN", "NOTES"
                );
                println!("{}", "-".repeat(96));
                for snapshot in &snapshots {
                    println!(
                        "{:>5}  {:<25}  {:>14.2}  {:>18}  {:<8}  {}",
                        snapshot.id,
                        snapshot.snapshot_at,
                        snapshot.cash_balance,
                        snapshot
                            .option_buying_power
                            .map(|v| format!("{v:.2}"))
                            .unwrap_or_else(|| "-".to_string()),
                        if snapshot.margin_enabled { "yes" } else { "no" },
                        snapshot.notes,
                    );
                }
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

fn render_account_snapshot(snapshot: &AccountSnapshot) {
    println!("Account snapshot #{}", snapshot.id);
    println!("  at            : {}", snapshot.snapshot_at);
    println!("  cash balance  : ${:.2}", snapshot.cash_balance);
    println!(
        "  option bp     : {}",
        snapshot
            .option_buying_power
            .map(|v| format!("${v:.2}"))
            .unwrap_or_else(|| "-".to_string())
    );
    println!(
        "  margin        : {}",
        if snapshot.margin_enabled {
            "enabled"
        } else {
            "cash-only"
        }
    );
    if !snapshot.notes.is_empty() {
        println!("  notes         : {}", snapshot.notes);
    }
}

async fn handle_strategies(ledger: &Ledger, underlying: Option<String>, json: bool) -> Result<()> {
    let positions = ledger.calculate_positions(underlying.as_deref())?;
    if positions.is_empty() {
        println!("No open positions.");
        return Ok(());
    }

    let strategies = risk_engine::identify_strategies(&positions);
    let account_snapshot = ledger
        .latest_account_snapshot()?
        .ok_or_else(|| anyhow::anyhow!("no account snapshot found; run `portfolio account set ...` first"))?;
    let account = AccountContext {
        cash_balance: Some(account_snapshot.cash_balance),
        option_buying_power: account_snapshot.option_buying_power,
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
        "Account snapshot: {} | cash=${:.2} | option bp={}",
        account_snapshot.snapshot_at,
        account_snapshot.cash_balance,
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
                leg.strike.map(|s| format!(" K={:.2}", s)).unwrap_or_default(),
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
    underlying: Option<String>,
    offline: bool,
    json: bool,
) -> Result<()> {
    let positions = ledger.calculate_positions(underlying.as_deref())?;
    if positions.is_empty() {
        println!("No open positions.");
        return Ok(());
    }
    let account_snapshot = ledger
        .latest_account_snapshot()?
        .ok_or_else(|| anyhow::anyhow!("no account snapshot found; run `portfolio account set ...` first"))?;
    let account = AccountContext {
        cash_balance: Some(account_snapshot.cash_balance),
        option_buying_power: account_snapshot.option_buying_power,
        margin_enabled: account_snapshot.margin_enabled,
    };

    // Enrich with live data if not offline
    let enriched: Option<Vec<EnrichedPosition>> = if !offline {
        match ThetaAnalysisService::from_env().await {
            Ok(service) => {
                match portfolio_service::enrich_positions(&service, &positions).await {
                    Ok(ep) => Some(ep),
                    Err(e) => {
                        eprintln!("Warning: live data enrichment failed: {}. Using offline data.", e);
                        None
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: LongPort connection failed: {}. Using offline data.", e);
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
    let evaluated_strategies = margin_engine::evaluate_strategies(&strategies, &margin_positions, &account);
    let total_margin: f64 = evaluated_strategies.iter().map(|s| s.margin.margin_required).sum();

    // Compute portfolio Greeks from enriched data
    let portfolio_greeks = enriched.as_ref().map(|ep| risk_engine::aggregate_greeks(ep));

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
    println!("Cash balance    : ${:.2}", account_snapshot.cash_balance);
    println!(
        "Option BP       : {}",
        account_snapshot
            .option_buying_power
            .map(|v| format!("${v:.2}"))
            .unwrap_or_else(|| "-".to_string())
    );
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
                    leg.strike.map(|s| format!(" K={:.2}", s)).unwrap_or_default(),
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
