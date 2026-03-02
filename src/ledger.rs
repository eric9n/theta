use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::path::PathBuf;

// ---------------------------------------------------------------------------
// Trade record stored in SQLite
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Trade {
    pub id: i64,
    pub trade_date: String,
    /// Option symbol (e.g. TSLA260320C00400000) or stock symbol (e.g. TSLA)
    pub symbol: String,
    /// Underlying stock symbol (e.g. TSLA)
    pub underlying: String,
    /// "call", "put", or "stock"
    pub side: String,
    /// Strike price (None for stock)
    pub strike: Option<f64>,
    /// Expiry date YYYY-MM-DD (None for stock)
    pub expiry: Option<String>,
    /// "buy" or "sell"
    pub action: String,
    /// Number of contracts (options) or shares (stock). Always positive.
    pub quantity: i64,
    /// Price per share / per contract
    pub price: f64,
    /// Commission / fees
    pub commission: f64,
    /// Free-form notes
    pub notes: String,
}

// ---------------------------------------------------------------------------
// Aggregated position derived from trades
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Position {
    pub symbol: String,
    pub underlying: String,
    /// "call", "put", or "stock"
    pub side: String,
    pub strike: Option<f64>,
    pub expiry: Option<String>,
    /// Positive = long, negative = short
    pub net_quantity: i64,
    /// Weighted average cost per share/contract
    pub avg_cost: f64,
    /// Total cost basis (absolute value)
    pub total_cost: f64,
}

// ---------------------------------------------------------------------------
// Query filters
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct TradeFilter {
    pub underlying: Option<String>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub symbol: Option<String>,
}

// ---------------------------------------------------------------------------
// Ledger — SQLite-backed trade ledger
// ---------------------------------------------------------------------------

pub struct Ledger {
    conn: Connection,
}

impl Ledger {
    /// Open (or create) the ledger database.
    pub fn open(path: &std::path::Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database {}", path.display()))?;
        let ledger = Self { conn };
        ledger.create_tables()?;
        Ok(ledger)
    }

    /// Open the default ledger at ~/.theta/portfolio.db
    pub fn open_default() -> Result<Self> {
        Self::open(&default_db_path()?)
    }

    fn create_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS trades (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                trade_date  TEXT    NOT NULL,
                symbol      TEXT    NOT NULL,
                underlying  TEXT    NOT NULL,
                side        TEXT    NOT NULL,  -- 'call', 'put', 'stock'
                strike      REAL,
                expiry      TEXT,
                action      TEXT    NOT NULL,  -- 'buy', 'sell'
                quantity    INTEGER NOT NULL,
                price       REAL    NOT NULL,
                commission  REAL    NOT NULL DEFAULT 0,
                notes       TEXT    NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_trades_underlying ON trades(underlying);
            CREATE INDEX IF NOT EXISTS idx_trades_symbol     ON trades(symbol);
            CREATE INDEX IF NOT EXISTS idx_trades_date       ON trades(trade_date);",
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Write operations
    // -----------------------------------------------------------------------

    /// Record a new trade. Returns the auto-generated trade id.
    pub fn record_trade(
        &self,
        trade_date: &str,
        symbol: &str,
        underlying: &str,
        side: &str,
        strike: Option<f64>,
        expiry: Option<&str>,
        action: &str,
        quantity: i64,
        price: f64,
        commission: f64,
        notes: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO trades (trade_date, symbol, underlying, side, strike, expiry, action, quantity, price, commission, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![trade_date, symbol, underlying, side, strike, expiry, action, quantity, price, commission, notes],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Delete a trade by id. Returns true if a row was deleted.
    pub fn delete_trade(&self, id: i64) -> Result<bool> {
        let affected = self.conn.execute(
            "DELETE FROM trades WHERE id = ?1",
            params![id],
        )?;
        Ok(affected > 0)
    }

    // -----------------------------------------------------------------------
    // Read operations
    // -----------------------------------------------------------------------

    /// List trades with optional filters.
    pub fn list_trades(&self, filter: &TradeFilter) -> Result<Vec<Trade>> {
        let mut sql = String::from("SELECT id, trade_date, symbol, underlying, side, strike, expiry, action, quantity, price, commission, notes FROM trades WHERE 1=1");
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref underlying) = filter.underlying {
            sql.push_str(" AND underlying = ?");
            param_values.push(Box::new(underlying.clone()));
        }
        if let Some(ref sym) = filter.symbol {
            sql.push_str(" AND symbol = ?");
            param_values.push(Box::new(sym.clone()));
        }
        if let Some(ref from) = filter.from_date {
            sql.push_str(" AND trade_date >= ?");
            param_values.push(Box::new(from.clone()));
        }
        if let Some(ref to) = filter.to_date {
            sql.push_str(" AND trade_date <= ?");
            param_values.push(Box::new(to.clone()));
        }

        sql.push_str(" ORDER BY trade_date ASC, id ASC");

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(Trade {
                id: row.get(0)?,
                trade_date: row.get(1)?,
                symbol: row.get(2)?,
                underlying: row.get(3)?,
                side: row.get(4)?,
                strike: row.get(5)?,
                expiry: row.get(6)?,
                action: row.get(7)?,
                quantity: row.get(8)?,
                price: row.get(9)?,
                commission: row.get(10)?,
                notes: row.get(11)?,
            })
        })?;

        let mut trades = Vec::new();
        for row in rows {
            trades.push(row?);
        }
        Ok(trades)
    }

    /// Compute current positions by aggregating all trades.
    /// Buy adds to position, sell subtracts. Returns only non-zero positions.
    pub fn calculate_positions(&self, underlying_filter: Option<&str>) -> Result<Vec<Position>> {
        let trades = self.list_trades(&TradeFilter {
            underlying: underlying_filter.map(String::from),
            ..Default::default()
        })?;

        // Group by (symbol, underlying, side, strike, expiry)
        use std::collections::HashMap;

        #[derive(Hash, Eq, PartialEq, Clone)]
        struct PosKey {
            symbol: String,
            underlying: String,
            side: String,
            strike_cents: i64, // strike × 100 for hashing
            expiry: Option<String>,
        }

        struct PosAccum {
            net_quantity: i64,
            total_buy_cost: f64,
            total_buy_qty: i64,
        }

        let mut accum: HashMap<PosKey, PosAccum> = HashMap::new();

        for t in &trades {
            let key = PosKey {
                symbol: t.symbol.clone(),
                underlying: t.underlying.clone(),
                side: t.side.clone(),
                strike_cents: t.strike.map(|s| (s * 100.0) as i64).unwrap_or(0),
                expiry: t.expiry.clone(),
            };

            let entry = accum.entry(key).or_insert(PosAccum {
                net_quantity: 0,
                total_buy_cost: 0.0,
                total_buy_qty: 0,
            });

            match t.action.as_str() {
                "buy" => {
                    entry.net_quantity += t.quantity;
                    entry.total_buy_cost += t.price * t.quantity as f64;
                    entry.total_buy_qty += t.quantity;
                }
                "sell" => {
                    entry.net_quantity -= t.quantity;
                }
                _ => {}
            }
        }

        let mut positions: Vec<Position> = accum
            .into_iter()
            .filter(|(_, v)| v.net_quantity != 0)
            .map(|(k, v)| {
                let avg_cost = if v.total_buy_qty > 0 {
                    v.total_buy_cost / v.total_buy_qty as f64
                } else {
                    0.0
                };
                Position {
                    symbol: k.symbol,
                    underlying: k.underlying,
                    side: k.side,
                    strike: if k.strike_cents != 0 {
                        Some(k.strike_cents as f64 / 100.0)
                    } else {
                        None
                    },
                    expiry: k.expiry,
                    net_quantity: v.net_quantity,
                    avg_cost,
                    total_cost: (avg_cost * v.net_quantity.unsigned_abs() as f64),
                }
            })
            .collect();

        positions.sort_by(|a, b| {
            a.underlying
                .cmp(&b.underlying)
                .then(a.expiry.cmp(&b.expiry))
                .then(a.side.cmp(&b.side))
                .then(
                    a.strike
                        .partial_cmp(&b.strike)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });

        Ok(positions)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_db_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME environment variable not set")?;
    Ok(PathBuf::from(home).join(".theta").join("portfolio.db"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_ledger() -> Ledger {
        let conn = Connection::open_in_memory().unwrap();
        let ledger = Ledger { conn };
        ledger.create_tables().unwrap();
        ledger
    }

    #[test]
    fn record_and_list_trades() {
        let ledger = in_memory_ledger();

        let id = ledger
            .record_trade(
                "2026-03-01",
                "TSLA260320C00400000",
                "TSLA",
                "call",
                Some(400.0),
                Some("2026-03-20"),
                "sell",
                2,
                5.30,
                0.0,
                "opening short call",
            )
            .unwrap();
        assert_eq!(id, 1);

        let trades = ledger.list_trades(&TradeFilter::default()).unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].symbol, "TSLA260320C00400000");
        assert_eq!(trades[0].quantity, 2);
    }

    #[test]
    fn positions_aggregate_correctly() {
        let ledger = in_memory_ledger();

        // Buy 200 shares of TSLA
        ledger
            .record_trade("2026-01-15", "TSLA", "TSLA", "stock", None, None, "buy", 200, 350.0, 0.0, "")
            .unwrap();

        // Sell 2 call contracts (short)
        ledger
            .record_trade(
                "2026-02-01", "TSLA260320C00400000", "TSLA", "call",
                Some(400.0), Some("2026-03-20"), "sell", 2, 5.30, 0.0, "",
            )
            .unwrap();

        // Buy 1 call contract back (partial close)
        ledger
            .record_trade(
                "2026-02-15", "TSLA260320C00400000", "TSLA", "call",
                Some(400.0), Some("2026-03-20"), "buy", 1, 3.20, 0.0, "",
            )
            .unwrap();

        let positions = ledger.calculate_positions(None).unwrap();
        assert_eq!(positions.len(), 2);

        let stock = positions.iter().find(|p| p.side == "stock").unwrap();
        assert_eq!(stock.net_quantity, 200);
        assert!((stock.avg_cost - 350.0).abs() < 0.001);

        let call = positions.iter().find(|p| p.side == "call").unwrap();
        // sold 2, bought 1 back → net = -1 (short 1 contract)
        assert_eq!(call.net_quantity, -1);
    }

    #[test]
    fn delete_trade() {
        let ledger = in_memory_ledger();
        let id = ledger
            .record_trade("2026-03-01", "TSLA", "TSLA", "stock", None, None, "buy", 100, 380.0, 0.0, "")
            .unwrap();

        assert!(ledger.delete_trade(id).unwrap());
        assert!(!ledger.delete_trade(id).unwrap()); // already deleted

        let trades = ledger.list_trades(&TradeFilter::default()).unwrap();
        assert!(trades.is_empty());
    }

    #[test]
    fn filter_by_underlying() {
        let ledger = in_memory_ledger();
        ledger.record_trade("2026-03-01", "TSLA", "TSLA", "stock", None, None, "buy", 100, 380.0, 0.0, "").unwrap();
        ledger.record_trade("2026-03-01", "AAPL", "AAPL", "stock", None, None, "buy", 50, 175.0, 0.0, "").unwrap();

        let filter = TradeFilter { underlying: Some("TSLA".into()), ..Default::default() };
        let trades = ledger.list_trades(&filter).unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].underlying, "TSLA");
    }

    #[test]
    fn closed_position_not_in_results() {
        let ledger = in_memory_ledger();
        ledger.record_trade("2026-01-01", "TSLA", "TSLA", "stock", None, None, "buy", 100, 350.0, 0.0, "").unwrap();
        ledger.record_trade("2026-02-01", "TSLA", "TSLA", "stock", None, None, "sell", 100, 400.0, 0.0, "").unwrap();

        let positions = ledger.calculate_positions(None).unwrap();
        assert!(positions.is_empty());
    }
}
