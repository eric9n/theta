use anyhow::{Context, Result, ensure};
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
    pub notes: String,
    pub account_id: String,
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

#[derive(Debug, Clone, Serialize)]
pub struct AccountSnapshot {
    pub id: i64,
    /// ISO8601 timestamp
    pub snapshot_at: String,
    /// Highest trade id already reflected in this snapshot, if known
    pub baseline_trade_id: Option<i64>,
    pub cash_balance: Option<f64>,
    pub trade_date_cash: f64,
    pub settled_cash: f64,
    pub option_buying_power: Option<f64>,
    pub stock_buying_power: Option<f64>,
    pub total_account_value: Option<f64>,
    pub long_stock_value: Option<f64>,
    pub long_option_value: Option<f64>,
    pub short_option_value: Option<f64>,
    pub margin_loan: Option<f64>,
    pub short_market_value: Option<f64>,
    pub margin_enabled: bool,
    pub notes: String,
    pub account_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct AccountSnapshotInput {
    pub snapshot_at: String,
    pub trade_date_cash: f64,
    pub settled_cash: f64,
    pub cash_balance: Option<f64>,
    pub option_buying_power: Option<f64>,
    pub stock_buying_power: Option<f64>,
    pub total_account_value: Option<f64>,
    pub long_stock_value: Option<f64>,
    pub long_option_value: Option<f64>,
    pub short_option_value: Option<f64>,
    pub margin_loan: Option<f64>,
    pub short_market_value: Option<f64>,
    pub margin_enabled: bool,
    pub notes: String,
    pub account_id: String,
}

#[derive(Debug, Clone, Default)]
pub struct AccountMonitorSnapshotInput {
    pub captured_at: String,
    pub account_id: String,
    pub status: String,
    pub data_quality: String,
    pub error_message: Option<String>,
    pub trade_date_cash: Option<f64>,
    pub settled_cash: Option<f64>,
    pub margin_loan: Option<f64>,
    pub option_buying_power: Option<f64>,
    pub positions_count: Option<i64>,
    pub position_market_value: Option<f64>,
    pub unrealized_pnl: Option<f64>,
    pub total_margin_required: Option<f64>,
    pub net_delta_shares: Option<f64>,
    pub total_gamma: Option<f64>,
    pub total_theta_per_day: Option<f64>,
    pub total_vega: Option<f64>,
    pub equity_estimate: Option<f64>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountMonitorSnapshot {
    pub id: i64,
    pub captured_at: String,
    pub account_id: String,
    pub status: String,
    pub data_quality: String,
    pub error_message: Option<String>,
    pub trade_date_cash: Option<f64>,
    pub settled_cash: Option<f64>,
    pub margin_loan: Option<f64>,
    pub option_buying_power: Option<f64>,
    pub positions_count: Option<i64>,
    pub position_market_value: Option<f64>,
    pub unrealized_pnl: Option<f64>,
    pub total_margin_required: Option<f64>,
    pub net_delta_shares: Option<f64>,
    pub total_gamma: Option<f64>,
    pub total_theta_per_day: Option<f64>,
    pub total_vega: Option<f64>,
    pub equity_estimate: Option<f64>,
    pub notes: String,
}

// ---------------------------------------------------------------------------
// Query filters
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct TradeFilter {
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub underlying: Option<String>,
    pub account_id: Option<String>,
    pub symbol: Option<String>,
}

struct PosAccum {
    net_quantity: i64,
    open_cost_total: f64,
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
                notes       TEXT    NOT NULL DEFAULT '',
                account_id  TEXT    NOT NULL DEFAULT 'firstrade'
            );

            CREATE INDEX IF NOT EXISTS idx_trades_underlying ON trades(underlying);
            CREATE INDEX IF NOT EXISTS idx_trades_symbol     ON trades(symbol);
            CREATE INDEX IF NOT EXISTS idx_trades_date       ON trades(trade_date);
            CREATE INDEX IF NOT EXISTS idx_trades_account    ON trades(account_id);

            CREATE TABLE IF NOT EXISTS account_snapshots (
                id                  INTEGER PRIMARY KEY,
                snapshot_at         TEXT    NOT NULL,
                baseline_trade_id   INTEGER,
                cash_balance        REAL,
                trade_date_cash     REAL    NOT NULL,
                settled_cash        REAL    NOT NULL,
                option_buying_power REAL,
                stock_buying_power  REAL,
                total_account_value REAL,
                long_stock_value    REAL,
                long_option_value   REAL,
                short_option_value  REAL,
                margin_loan         REAL,
                short_market_value  REAL,
                margin_enabled      BOOLEAN NOT NULL,
                notes               TEXT    NOT NULL,
                account_id          TEXT    NOT NULL DEFAULT 'firstrade'
            );

            CREATE INDEX IF NOT EXISTS idx_account_snapshots_time
                ON account_snapshots(snapshot_at);
            CREATE INDEX IF NOT EXISTS idx_account_snapshots_account
                ON account_snapshots(account_id);

            CREATE TABLE IF NOT EXISTS account_monitor_snapshots (
                id                    INTEGER PRIMARY KEY AUTOINCREMENT,
                captured_at           TEXT    NOT NULL,
                account_id            TEXT    NOT NULL,
                status                TEXT    NOT NULL,
                data_quality          TEXT    NOT NULL DEFAULT 'unknown',
                error_message         TEXT,
                trade_date_cash       REAL,
                settled_cash          REAL,
                margin_loan           REAL,
                option_buying_power   REAL,
                positions_count       INTEGER,
                position_market_value REAL,
                unrealized_pnl        REAL,
                total_margin_required REAL,
                net_delta_shares      REAL,
                total_gamma           REAL,
                total_theta_per_day   REAL,
                total_vega            REAL,
                equity_estimate       REAL,
                notes                 TEXT    NOT NULL DEFAULT ''
            );

            CREATE INDEX IF NOT EXISTS idx_account_monitor_snapshots_time
                ON account_monitor_snapshots(captured_at);
            CREATE INDEX IF NOT EXISTS idx_account_monitor_snapshots_account
                ON account_monitor_snapshots(account_id, captured_at);",
        )?;
        self.ensure_account_snapshots_columns()?;
        self.ensure_account_monitor_snapshot_columns()?;
        Ok(())
    }

    pub fn with_transaction<T, F>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Self) -> Result<T>,
    {
        self.conn.execute_batch("BEGIN IMMEDIATE")?;
        let result = f(self);
        match result {
            Ok(value) => {
                self.conn.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(err) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(err)
            }
        }
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
        account_id: &str,
    ) -> Result<i64> {
        self.record_trade_internal(
            trade_date, symbol, underlying, side, strike, expiry, action, quantity, price,
            commission, notes, account_id, false,
        )
    }

    pub fn record_adjustment_trade(
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
        account_id: &str,
    ) -> Result<i64> {
        self.record_trade_internal(
            trade_date, symbol, underlying, side, strike, expiry, action, quantity, price,
            commission, notes, account_id, true,
        )
    }

    fn record_trade_internal(
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
        account_id: &str,
        allow_zero_price: bool,
    ) -> Result<i64> {
        ensure!(
            matches!(side, "call" | "put" | "stock"),
            "invalid side: {side}"
        );
        ensure!(
            matches!(action, "buy" | "sell" | "deposit" | "withdraw" | "dividend"),
            "invalid action: {action}"
        );
        ensure!(quantity > 0, "quantity must be positive");
        if allow_zero_price {
            ensure!(price >= 0.0, "price must be non-negative");
        } else {
            ensure!(price > 0.0, "price must be positive");
        }
        ensure!(commission >= 0.0, "commission must be non-negative");

        self.conn.execute(
            "INSERT INTO trades (trade_date, symbol, underlying, side, strike, expiry, action, quantity, price, commission, notes, account_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![trade_date, symbol, underlying, side, strike, expiry, action, quantity, price, commission, notes, account_id],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Delete a trade by id. Returns true if a row was deleted.
    pub fn delete_trade(&self, id: i64) -> Result<bool> {
        let affected = self
            .conn
            .execute("DELETE FROM trades WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    pub fn record_account_snapshot(&self, input: &AccountSnapshotInput) -> Result<i64> {
        let baseline_trade_id = self.latest_trade_id(&input.account_id)?;
        self.conn.execute(
            "INSERT INTO account_snapshots (
                snapshot_at, baseline_trade_id, cash_balance, trade_date_cash, settled_cash,
                option_buying_power, stock_buying_power, total_account_value,
                long_stock_value, long_option_value, short_option_value,
                margin_loan, short_market_value, margin_enabled, notes, account_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                &input.snapshot_at,
                baseline_trade_id,
                input.cash_balance,
                input.trade_date_cash,
                input.settled_cash,
                input.option_buying_power,
                input.stock_buying_power,
                input.total_account_value,
                input.long_stock_value,
                input.long_option_value,
                input.short_option_value,
                input.margin_loan,
                input.short_market_value,
                input.margin_enabled,
                &input.notes,
                &input.account_id,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn record_account_monitor_snapshot(
        &self,
        input: &AccountMonitorSnapshotInput,
    ) -> Result<i64> {
        ensure!(
            !input.captured_at.trim().is_empty(),
            "captured_at must not be empty"
        );
        ensure!(
            !input.account_id.trim().is_empty(),
            "account_id must not be empty"
        );
        ensure!(
            matches!(input.status.as_str(), "ok" | "error"),
            "invalid status: {}",
            input.status
        );
        ensure!(
            matches!(
                input.data_quality.as_str(),
                "complete" | "incomplete_quotes" | "quote_unavailable" | "system_error" | "unknown"
            ),
            "invalid data_quality: {}",
            input.data_quality
        );

        self.conn.execute(
            "INSERT INTO account_monitor_snapshots (
                captured_at, account_id, status, data_quality, error_message,
                trade_date_cash, settled_cash, margin_loan, option_buying_power,
                positions_count, position_market_value, unrealized_pnl,
                total_margin_required, net_delta_shares, total_gamma,
                total_theta_per_day, total_vega, equity_estimate, notes
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                &input.captured_at,
                &input.account_id,
                &input.status,
                &input.data_quality,
                &input.error_message,
                input.trade_date_cash,
                input.settled_cash,
                input.margin_loan,
                input.option_buying_power,
                input.positions_count,
                input.position_market_value,
                input.unrealized_pnl,
                input.total_margin_required,
                input.net_delta_shares,
                input.total_gamma,
                input.total_theta_per_day,
                input.total_vega,
                input.equity_estimate,
                &input.notes,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    // -----------------------------------------------------------------------
    // Read operations
    // -----------------------------------------------------------------------

    /// List trades with optional filters.
    pub fn list_trades(&self, filter: &TradeFilter) -> Result<Vec<Trade>> {
        let mut sql = String::from(
            "SELECT id, trade_date, symbol, underlying, side, strike, expiry, action, quantity, price, commission, notes, account_id FROM trades WHERE 1=1",
        );
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(ref underlying) = filter.underlying {
            sql.push_str(" AND underlying = ?");
            param_values.push(Box::new(underlying.clone()));
        }
        if let Some(ref sym) = filter.symbol {
            sql.push_str(" AND symbol = ?");
            param_values.push(Box::new(sym.clone()));
        }
        if let Some(ref start_date) = filter.start_date {
            sql.push_str(" AND trade_date >= ?");
            param_values.push(Box::new(start_date.clone()));
        }
        if let Some(ref end_date) = filter.end_date {
            sql.push_str(" AND trade_date <= ?");
            param_values.push(Box::new(end_date.clone()));
        }
        if let Some(ref account_id) = filter.account_id {
            sql.push_str(" AND account_id = ?");
            param_values.push(Box::new(account_id.clone()));
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
                account_id: row.get(12)?,
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
    pub fn calculate_positions(
        &self,
        account_id: &str,
        underlying_filter: Option<&str>,
    ) -> Result<Vec<Position>> {
        let trades = self.list_trades(&TradeFilter {
            underlying: underlying_filter.map(String::from),
            account_id: Some(account_id.to_string()),
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
                open_cost_total: 0.0,
            });

            let signed_trade_quantity = match t.action.as_str() {
                "buy" => t.quantity,
                "sell" => -t.quantity,
                _ => continue,
            };
            let trade_quantity_abs = t.quantity;
            let trade_direction = signed_trade_quantity.signum();

            if entry.net_quantity == 0 || entry.net_quantity.signum() == trade_direction {
                apply_open_trade(
                    entry,
                    trade_direction,
                    trade_quantity_abs,
                    t.price,
                    t.commission,
                );
                continue;
            }

            let closing_quantity = trade_quantity_abs.min(entry.net_quantity.unsigned_abs() as i64);
            if closing_quantity > 0 {
                let avg_open_cost =
                    entry.open_cost_total / entry.net_quantity.unsigned_abs() as f64;
                entry.net_quantity += trade_direction * closing_quantity;
                entry.open_cost_total -= avg_open_cost * closing_quantity as f64;
                if entry.net_quantity == 0 {
                    entry.open_cost_total = 0.0;
                }
            }

            let opening_remainder = trade_quantity_abs - closing_quantity;
            if opening_remainder > 0 {
                let opening_commission =
                    t.commission * (opening_remainder as f64 / trade_quantity_abs as f64);
                apply_open_trade(
                    entry,
                    trade_direction,
                    opening_remainder,
                    t.price,
                    opening_commission,
                );
            }
        }

        let today = time::OffsetDateTime::now_utc().date().to_string();

        let mut positions: Vec<Position> = accum
            .into_iter()
            .filter(|(k, v)| {
                // filter quantity
                if v.net_quantity == 0 {
                    return false;
                }
                // filter expiry (if present and in the past)
                if let Some(ref exp) = k.expiry {
                    if exp < &today {
                        return false;
                    }
                }
                true
            })
            .map(|(k, v)| {
                let avg_cost = if v.net_quantity != 0 {
                    v.open_cost_total / v.net_quantity.unsigned_abs() as f64
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
                    total_cost: v.open_cost_total.abs(),
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

    pub fn latest_account_snapshot(&self, account_id: &str) -> Result<Option<AccountSnapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, snapshot_at, baseline_trade_id, cash_balance, trade_date_cash, settled_cash,
                    option_buying_power, stock_buying_power, total_account_value,
                    long_stock_value, long_option_value, short_option_value,
                    margin_loan, short_market_value, margin_enabled, notes, account_id
             FROM account_snapshots 
             WHERE account_id = ?
             ORDER BY snapshot_at DESC, id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![account_id], |row| {
            Ok(AccountSnapshot {
                id: row.get(0)?,
                snapshot_at: row.get(1)?,
                baseline_trade_id: row.get(2)?,
                cash_balance: row.get(3)?,
                trade_date_cash: row.get(4)?,
                settled_cash: row.get(5)?,
                option_buying_power: row.get(6)?,
                stock_buying_power: row.get(7)?,
                total_account_value: row.get(8)?,
                long_stock_value: row.get(9)?,
                long_option_value: row.get(10)?,
                short_option_value: row.get(11)?,
                margin_loan: row.get(12)?,
                short_market_value: row.get(13)?,
                margin_enabled: row.get(14)?,
                notes: row.get(15)?,
                account_id: row.get(16)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    /// Returns the latest snapshot that was NOT an automatic update.
    /// This is used as a checkpoint/baseline for balance derivation.
    pub fn latest_manual_snapshot(&self, account_id: &str) -> Result<Option<AccountSnapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, snapshot_at, baseline_trade_id, cash_balance, trade_date_cash, settled_cash,
                    option_buying_power, stock_buying_power, total_account_value,
                    long_stock_value, long_option_value, short_option_value,
                    margin_loan, short_market_value, margin_enabled, notes, account_id
             FROM account_snapshots 
             WHERE notes NOT LIKE 'auto-update%' AND account_id = ?
             ORDER BY snapshot_at DESC, id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query_map(params![account_id], |row| {
            Ok(AccountSnapshot {
                id: row.get(0)?,
                snapshot_at: row.get(1)?,
                baseline_trade_id: row.get(2)?,
                cash_balance: row.get(3)?,
                trade_date_cash: row.get(4)?,
                settled_cash: row.get(5)?,
                option_buying_power: row.get(6)?,
                stock_buying_power: row.get(7)?,
                total_account_value: row.get(8)?,
                long_stock_value: row.get(9)?,
                long_option_value: row.get(10)?,
                short_option_value: row.get(11)?,
                margin_loan: row.get(12)?,
                short_market_value: row.get(13)?,
                margin_enabled: row.get(14)?,
                notes: row.get(15)?,
                account_id: row.get(16)?,
            })
        })?;
        Ok(rows.next().transpose()?)
    }

    pub fn list_account_snapshots(&self, account_id: &str) -> Result<Vec<AccountSnapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, snapshot_at, baseline_trade_id, cash_balance, trade_date_cash, settled_cash,
                    option_buying_power, stock_buying_power, total_account_value,
                    long_stock_value, long_option_value, short_option_value,
                    margin_loan, short_market_value, margin_enabled, notes, account_id
             FROM account_snapshots
             WHERE account_id = ?
             ORDER BY snapshot_at DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![account_id], |row| {
            Ok(AccountSnapshot {
                id: row.get(0)?,
                snapshot_at: row.get(1)?,
                baseline_trade_id: row.get(2)?,
                cash_balance: row.get(3)?,
                trade_date_cash: row.get(4)?,
                settled_cash: row.get(5)?,
                option_buying_power: row.get(6)?,
                stock_buying_power: row.get(7)?,
                total_account_value: row.get(8)?,
                long_stock_value: row.get(9)?,
                long_option_value: row.get(10)?,
                short_option_value: row.get(11)?,
                margin_loan: row.get(12)?,
                short_market_value: row.get(13)?,
                margin_enabled: row.get(14)?,
                notes: row.get(15)?,
                account_id: row.get(16)?,
            })
        })?;

        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row?);
        }
        Ok(snapshots)
    }

    pub fn list_account_monitor_snapshots(
        &self,
        account_id: &str,
        limit: usize,
    ) -> Result<Vec<AccountMonitorSnapshot>> {
        let limit = i64::try_from(limit).context("monitor snapshot limit is too large")?;
        let mut stmt = self.conn.prepare(
            "SELECT id, captured_at, account_id, status, data_quality, error_message,
                    trade_date_cash, settled_cash, margin_loan, option_buying_power,
                    positions_count, position_market_value, unrealized_pnl,
                    total_margin_required, net_delta_shares, total_gamma,
                    total_theta_per_day, total_vega, equity_estimate, notes
             FROM account_monitor_snapshots
             WHERE account_id = ?1
             ORDER BY captured_at DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![account_id, limit], |row| {
            Ok(AccountMonitorSnapshot {
                id: row.get(0)?,
                captured_at: row.get(1)?,
                account_id: row.get(2)?,
                status: row.get(3)?,
                data_quality: row.get(4)?,
                error_message: row.get(5)?,
                trade_date_cash: row.get(6)?,
                settled_cash: row.get(7)?,
                margin_loan: row.get(8)?,
                option_buying_power: row.get(9)?,
                positions_count: row.get(10)?,
                position_market_value: row.get(11)?,
                unrealized_pnl: row.get(12)?,
                total_margin_required: row.get(13)?,
                net_delta_shares: row.get(14)?,
                total_gamma: row.get(15)?,
                total_theta_per_day: row.get(16)?,
                total_vega: row.get(17)?,
                equity_estimate: row.get(18)?,
                notes: row.get(19)?,
            })
        })?;

        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row?);
        }
        Ok(snapshots)
    }

    fn ensure_account_snapshots_columns(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(account_snapshots)")?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        if !columns.iter().any(|column| column == "baseline_trade_id") {
            self.conn.execute(
                "ALTER TABLE account_snapshots ADD COLUMN baseline_trade_id INTEGER",
                [],
            )?;
        }
        if !columns.iter().any(|column| column == "cash_balance") {
            self.conn.execute(
                "ALTER TABLE account_snapshots ADD COLUMN cash_balance REAL",
                [],
            )?;
        }
        if !columns.iter().any(|column| column == "total_account_value") {
            self.conn.execute(
                "ALTER TABLE account_snapshots ADD COLUMN total_account_value REAL",
                [],
            )?;
        }
        if !columns.iter().any(|column| column == "long_stock_value") {
            self.conn.execute(
                "ALTER TABLE account_snapshots ADD COLUMN long_stock_value REAL",
                [],
            )?;
        }
        if !columns.iter().any(|column| column == "long_option_value") {
            self.conn.execute(
                "ALTER TABLE account_snapshots ADD COLUMN long_option_value REAL",
                [],
            )?;
        }
        if !columns.iter().any(|column| column == "short_option_value") {
            self.conn.execute(
                "ALTER TABLE account_snapshots ADD COLUMN short_option_value REAL",
                [],
            )?;
        }

        Ok(())
    }

    fn ensure_account_monitor_snapshot_columns(&self) -> Result<()> {
        let mut stmt = self
            .conn
            .prepare("PRAGMA table_info(account_monitor_snapshots)")?;
        let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut has_data_quality = false;
        for column in columns {
            if column? == "data_quality" {
                has_data_quality = true;
                break;
            }
        }

        if !has_data_quality {
            self.conn.execute(
                "ALTER TABLE account_monitor_snapshots ADD COLUMN data_quality TEXT NOT NULL DEFAULT 'unknown'",
                [],
            )?;
        }

        Ok(())
    }

    fn latest_trade_id(&self, account_id: &str) -> Result<Option<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT MAX(id) FROM trades WHERE account_id = ?1")?;
        let latest = stmt.query_row(params![account_id], |row| row.get(0))?;
        Ok(latest)
    }
}

fn apply_open_trade(
    accum: &mut PosAccum,
    direction: i64,
    quantity: i64,
    price: f64,
    commission: f64,
) {
    if quantity <= 0 {
        return;
    }

    accum.net_quantity += direction * quantity;

    if direction > 0 {
        accum.open_cost_total += price * quantity as f64 + commission;
    } else {
        accum.open_cost_total += price * quantity as f64 - commission;
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
                "firstrade",
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
            .record_trade(
                "2026-01-15",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "buy",
                200,
                350.0,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();

        // Sell 2 call contracts (short)
        ledger
            .record_trade(
                "2026-02-01",
                "TSLA260320C00400000",
                "TSLA",
                "call",
                Some(400.0),
                Some("2026-03-20"),
                "sell",
                2,
                5.30,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();

        // Buy 1 call contract back (partial close)
        ledger
            .record_trade(
                "2026-02-15",
                "TSLA260320C00400000",
                "TSLA",
                "call",
                Some(400.0),
                Some("2026-03-20"),
                "buy",
                1,
                3.20,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();

        let positions = ledger.calculate_positions("firstrade", None).unwrap();
        assert_eq!(positions.len(), 2);

        let stock = positions.iter().find(|p| p.side == "stock").unwrap();
        assert_eq!(stock.net_quantity, 200);
        assert!((stock.avg_cost - 350.0).abs() < 0.001);

        let call = positions.iter().find(|p| p.side == "call").unwrap();
        // sold 2, bought 1 back → net = -1 (short 1 contract)
        assert_eq!(call.net_quantity, -1);
        assert!((call.avg_cost - 5.30).abs() < 0.001);
    }

    #[test]
    fn delete_trade() {
        let ledger = in_memory_ledger();
        let id = ledger
            .record_trade(
                "2026-03-01",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "buy",
                100,
                380.0,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();

        assert!(ledger.delete_trade(id).unwrap());
        assert!(!ledger.delete_trade(id).unwrap()); // already deleted

        let trades = ledger.list_trades(&TradeFilter::default()).unwrap();
        assert!(trades.is_empty());
    }

    #[test]
    fn filter_by_underlying() {
        let ledger = in_memory_ledger();
        ledger
            .record_trade(
                "2026-03-01",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "buy",
                100,
                380.0,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_trade(
                "2026-03-01",
                "AAPL",
                "AAPL",
                "stock",
                None,
                None,
                "buy",
                50,
                175.0,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();

        let filter = TradeFilter {
            underlying: Some("TSLA".into()),
            ..Default::default()
        };
        let trades = ledger.list_trades(&filter).unwrap();
        assert_eq!(trades.len(), 1);
        assert_eq!(trades[0].underlying, "TSLA");
    }

    #[test]
    fn closed_position_not_in_results() {
        let ledger = in_memory_ledger();
        ledger
            .record_trade(
                "2026-01-01",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "buy",
                100,
                350.0,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_trade(
                "2026-02-01",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "sell",
                100,
                400.0,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();

        let positions = ledger.calculate_positions("firstrade", None).unwrap();
        assert!(positions.is_empty());
    }

    #[test]
    fn short_positions_keep_opening_credit_as_cost_basis() {
        let ledger = in_memory_ledger();
        ledger
            .record_trade(
                "2026-03-01",
                "TSLA260320P00350000",
                "TSLA",
                "put",
                Some(350.0),
                Some("2026-03-20"),
                "sell",
                1,
                5.00,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();

        let positions = ledger.calculate_positions("firstrade", None).unwrap();
        let put = positions.iter().find(|p| p.side == "put").unwrap();
        assert_eq!(put.net_quantity, -1);
        assert!((put.avg_cost - 5.00).abs() < 0.001);
        assert!((put.total_cost - 5.00).abs() < 0.001);
    }

    #[test]
    fn commissions_are_included_in_remaining_open_cost() {
        let ledger = in_memory_ledger();
        ledger
            .record_trade(
                "2026-03-01",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "buy",
                100,
                10.0,
                5.0,
                "",
                "firstrade",
            )
            .unwrap();

        let positions = ledger.calculate_positions("firstrade", None).unwrap();
        let stock = positions.iter().find(|p| p.side == "stock").unwrap();
        assert_eq!(stock.net_quantity, 100);
        assert!((stock.avg_cost - 10.05).abs() < 0.001);
        assert!((stock.total_cost - 1005.0).abs() < 0.001);
    }

    #[test]
    fn expired_contract_is_filtered() {
        let ledger = in_memory_ledger();
        // Record a trade with an old expiry (e.g., 2026-02-20)
        ledger
            .record_trade(
                "2026-02-20",
                "TSLA260220P390000",
                "TSLA",
                "put",
                Some(390.0),
                Some("2026-02-20"),
                "sell",
                1,
                0.01,
                0.0,
                "expired short put",
                "firstrade",
            )
            .unwrap();

        // Calculate positions - assuming today is after 2026-02-20
        let positions = ledger.calculate_positions("firstrade", None).unwrap();
        assert!(
            positions.is_empty(),
            "Expired contract should be filtered out"
        );
    }

    #[test]
    fn account_snapshots_roundtrip() {
        let ledger = in_memory_ledger();
        ledger
            .record_trade(
                "2026-03-02",
                "TSLA",
                "TSLA",
                "stock",
                None,
                None,
                "buy",
                1,
                100.0,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();
        let id = ledger
            .record_account_snapshot(&AccountSnapshotInput {
                snapshot_at: "2026-03-02T09:30:00Z".to_string(),
                trade_date_cash: 50_000.0,
                settled_cash: 50_000.0,
                cash_balance: Some(50_500.0),
                option_buying_power: Some(120_000.0),
                stock_buying_power: Some(200_000.0),
                total_account_value: Some(75_000.0),
                long_stock_value: Some(15_000.0),
                long_option_value: Some(3_500.0),
                short_option_value: Some(-2_250.0),
                margin_loan: None,
                short_market_value: None,
                margin_enabled: true,
                notes: "initial snapshot".to_string(),
                account_id: "firstrade".to_string(),
            })
            .unwrap();
        assert_eq!(id, 1);

        let latest = ledger
            .latest_account_snapshot("firstrade")
            .unwrap()
            .expect("snapshot");
        assert_eq!(latest.baseline_trade_id, Some(1));
        assert_eq!(latest.cash_balance, Some(50_500.0));
        assert_eq!(latest.trade_date_cash, 50_000.0);
        assert_eq!(latest.settled_cash, 50_000.0);
        assert_eq!(latest.option_buying_power, Some(120_000.0));
        assert_eq!(latest.stock_buying_power, Some(200_000.0));
        assert_eq!(latest.total_account_value, Some(75_000.0));
        assert_eq!(latest.long_stock_value, Some(15_000.0));
        assert_eq!(latest.long_option_value, Some(3_500.0));
        assert_eq!(latest.short_option_value, Some(-2_250.0));
        assert!(latest.margin_enabled);
        assert_eq!(latest.notes, "initial snapshot");
    }

    #[test]
    fn create_tables_migrates_legacy_account_snapshots_schema() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE account_snapshots (
                id                  INTEGER PRIMARY KEY,
                snapshot_at         TEXT    NOT NULL,
                trade_date_cash     REAL    NOT NULL,
                settled_cash        REAL    NOT NULL,
                option_buying_power REAL,
                stock_buying_power  REAL,
                margin_loan         REAL,
                short_market_value  REAL,
                margin_enabled      BOOLEAN NOT NULL,
                notes               TEXT    NOT NULL,
                account_id          TEXT    NOT NULL DEFAULT 'firstrade'
            );",
        )
        .unwrap();

        let ledger = Ledger { conn };
        ledger.create_tables().unwrap();

        let mut stmt = ledger
            .conn
            .prepare("PRAGMA table_info(account_snapshots)")
            .unwrap();
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        assert!(columns.iter().any(|name| name == "baseline_trade_id"));
        assert!(columns.iter().any(|name| name == "cash_balance"));
        assert!(columns.iter().any(|name| name == "total_account_value"));
        assert!(columns.iter().any(|name| name == "long_stock_value"));
        assert!(columns.iter().any(|name| name == "long_option_value"));
        assert!(columns.iter().any(|name| name == "short_option_value"));
    }

    #[test]
    fn zero_priced_adjustments_can_close_positions() {
        let ledger = in_memory_ledger();
        ledger
            .record_trade(
                "2026-03-01",
                "TSLA260320C00400000",
                "TSLA",
                "call",
                Some(400.0),
                Some("2026-03-20"),
                "buy",
                1,
                5.0,
                0.0,
                "",
                "firstrade",
            )
            .unwrap();
        ledger
            .record_adjustment_trade(
                "2026-03-20",
                "TSLA260320C00400000",
                "TSLA",
                "call",
                Some(400.0),
                Some("2026-03-20"),
                "sell",
                1,
                0.0,
                0.0,
                "expired",
                "firstrade",
            )
            .unwrap();

        let positions = ledger.calculate_positions("firstrade", None).unwrap();
        assert!(positions.is_empty());
    }

    #[test]
    fn account_monitor_snapshots_roundtrip_ok() {
        let ledger = in_memory_ledger();
        ledger
            .record_account_monitor_snapshot(&AccountMonitorSnapshotInput {
                captured_at: "2026-03-06T14:35:00Z".to_string(),
                account_id: "firstrade".to_string(),
                status: "ok".to_string(),
                data_quality: "complete".to_string(),
                error_message: None,
                trade_date_cash: Some(12_000.0),
                settled_cash: Some(11_500.0),
                margin_loan: Some(2_000.0),
                option_buying_power: Some(8_000.0),
                positions_count: Some(3),
                position_market_value: Some(25_000.0),
                unrealized_pnl: Some(320.5),
                total_margin_required: Some(1_200.0),
                net_delta_shares: Some(45.0),
                total_gamma: Some(-0.2),
                total_theta_per_day: Some(6.0),
                total_vega: Some(40.0),
                equity_estimate: Some(34_500.0),
                notes: "monitor tick".to_string(),
            })
            .unwrap();
        let snapshots = ledger
            .list_account_monitor_snapshots("firstrade", 10)
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        let row = &snapshots[0];

        assert_eq!(row.status, "ok");
        assert_eq!(row.data_quality, "complete");
        assert_eq!(row.account_id, "firstrade");
        assert_eq!(row.positions_count, Some(3));
        assert_eq!(row.equity_estimate, Some(34_500.0));
        assert_eq!(row.notes, "monitor tick");
    }

    #[test]
    fn account_monitor_snapshots_roundtrip_error() {
        let ledger = in_memory_ledger();
        ledger
            .record_account_monitor_snapshot(&AccountMonitorSnapshotInput {
                captured_at: "2026-03-06T14:40:00Z".to_string(),
                account_id: "firstrade".to_string(),
                status: "error".to_string(),
                data_quality: "system_error".to_string(),
                error_message: Some("failed to enrich positions".to_string()),
                notes: "monitor tick".to_string(),
                ..Default::default()
            })
            .unwrap();
        let snapshots = ledger
            .list_account_monitor_snapshots("firstrade", 10)
            .unwrap();
        assert_eq!(snapshots.len(), 1);
        let row = &snapshots[0];

        assert_eq!(row.status, "error");
        assert_eq!(
            row.error_message.as_deref(),
            Some("failed to enrich positions")
        );
        assert_eq!(row.trade_date_cash, None);
    }

    #[test]
    fn list_account_monitor_snapshots_respects_limit() {
        let ledger = in_memory_ledger();
        for i in 0..3 {
            ledger
                .record_account_monitor_snapshot(&AccountMonitorSnapshotInput {
                    captured_at: format!("2026-03-06T14:4{i}:00Z"),
                    account_id: "firstrade".to_string(),
                    status: "ok".to_string(),
                    data_quality: "complete".to_string(),
                    notes: format!("tick-{i}"),
                    ..Default::default()
                })
                .unwrap();
        }

        let snapshots = ledger
            .list_account_monitor_snapshots("firstrade", 2)
            .unwrap();
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].notes, "tick-2");
        assert_eq!(snapshots[1].notes, "tick-1");
    }

    #[test]
    fn create_tables_migrates_legacy_account_monitor_snapshots_schema() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE account_monitor_snapshots (
                id                    INTEGER PRIMARY KEY,
                captured_at           TEXT    NOT NULL,
                account_id            TEXT    NOT NULL,
                status                TEXT    NOT NULL,
                error_message         TEXT,
                trade_date_cash       REAL,
                settled_cash          REAL,
                margin_loan           REAL,
                option_buying_power   REAL,
                positions_count       INTEGER,
                position_market_value REAL,
                unrealized_pnl        REAL,
                total_margin_required REAL,
                net_delta_shares      REAL,
                total_gamma           REAL,
                total_theta_per_day   REAL,
                total_vega            REAL,
                equity_estimate       REAL,
                notes                 TEXT    NOT NULL
            );",
        )
        .unwrap();

        let ledger = Ledger { conn };
        ledger.create_tables().unwrap();

        let mut stmt = ledger
            .conn
            .prepare("PRAGMA table_info(account_monitor_snapshots)")
            .unwrap();
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        assert!(columns.iter().any(|name| name == "data_quality"));
    }
}
