use anyhow::{bail, Result};
use clap::{Parser, Subcommand};
use theta::analysis_service::ThetaAnalysisService;
use theta::ledger::{Ledger, TradeFilter};
use theta::portfolio_service;
use theta::risk_engine;
use theta::risk_domain::EnrichedPosition;
use std::path::PathBuf;

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
        #[arg(long, help = "Skip LongPort connection, use offline data")]
        offline: bool,
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
        Command::Trade { action } => handle_trade(&ledger, action),
        Command::Positions { underlying, json } => handle_positions(&ledger, underlying, json),
        Command::Strategies { underlying, offline, json } => {
            handle_strategies(&ledger, underlying, offline, json).await
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

async fn handle_strategies(ledger: &Ledger, underlying: Option<String>, _offline: bool, json: bool) -> Result<()> {
    let positions = ledger.calculate_positions(underlying.as_deref())?;
    if positions.is_empty() {
        println!("No open positions.");
        return Ok(());
    }

    let strategies = risk_engine::identify_strategies(&positions);
    if json {
        println!("{}", serde_json::to_string_pretty(&strategies)?);
        return Ok(());
    }

    if strategies.is_empty() {
        println!("No strategies identified.");
        return Ok(());
    }

    for (i, s) in strategies.iter().enumerate() {
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

    let total_margin: f64 = strategies.iter().map(|s| s.margin.margin_required).sum();
    println!("Total margin required: ${:.2}", total_margin);
    Ok(())
}

async fn handle_report(ledger: &Ledger, underlying: Option<String>, offline: bool, json: bool) -> Result<()> {
    let positions = ledger.calculate_positions(underlying.as_deref())?;
    if positions.is_empty() {
        println!("No open positions.");
        return Ok(());
    }

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
    let total_margin: f64 = strategies.iter().map(|s| s.margin.margin_required).sum();

    // Compute portfolio Greeks from enriched data
    let portfolio_greeks = enriched.as_ref().map(|ep| risk_engine::aggregate_greeks(ep));

    if json {
        let report = serde_json::json!({
            "positions": enriched.as_ref().map(|e| serde_json::to_value(e).ok()).unwrap_or_else(|| serde_json::to_value(&positions).ok()),
            "strategies": strategies,
            "portfolio_greeks": portfolio_greeks,
            "total_margin_required": total_margin,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
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
    if strategies.is_empty() {
        println!("No strategies identified.");
    } else {
        for (i, s) in strategies.iter().enumerate() {
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
    println!("Strategies      : {}", strategies.len());
    println!("Total margin    : ${:.2}", total_margin);
    if let Some(ref ep) = enriched {
        let total_pnl: f64 = ep.iter().map(|p| p.unrealized_pnl).sum();
        println!("Unrealized P&L  : ${:.2}", total_pnl);
    }

    Ok(())
}
