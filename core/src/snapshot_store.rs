use crate::domain::{MarketToneView, SmilePoint};
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct MarketToneSnapshotRow {
    pub id: i64,
    pub captured_at: String,
    pub symbol: String,
    pub front_expiry: String,
    pub delta_skew: Option<f64>,
    pub otm_skew: Option<f64>,
    pub front_atm_iv: f64,
    pub farthest_atm_iv: Option<f64>,
    pub term_structure_change_from_front: Option<f64>,
    pub open_interest_bias_ratio: Option<f64>,
    pub otm_open_interest_bias_ratio: Option<f64>,
    pub overall_tone: String,
    pub summary_sentence: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyMonitorSnapshotRow {
    pub id: i64,
    pub captured_at: String,
    pub symbol: String,
    pub near_expiry: String,
    pub near_days_to_expiry: i64,
    pub far_expiry: String,
    pub far_days_to_expiry: i64,
    pub bull_put_net_credit: Option<f64>,
    pub bear_call_net_credit: Option<f64>,
    pub iron_condor_net_credit: Option<f64>,
    pub bull_call_net_debit: Option<f64>,
    pub calendar_call_net_debit: Option<f64>,
    pub pmcc_net_debit: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct StrategyMonitorSnapshotInput {
    pub captured_at: String,
    pub symbol: String,
    pub near_expiry: String,
    pub near_days_to_expiry: i64,
    pub far_expiry: String,
    pub far_days_to_expiry: i64,
    pub bull_put_short_strike: Option<f64>,
    pub bull_put_long_strike: Option<f64>,
    pub bull_put_net_credit: Option<f64>,
    pub bull_put_max_loss: Option<f64>,
    pub bull_put_breakeven: Option<f64>,
    pub bull_put_annualized_return: Option<f64>,
    pub bear_call_short_strike: Option<f64>,
    pub bear_call_long_strike: Option<f64>,
    pub bear_call_net_credit: Option<f64>,
    pub bear_call_max_loss: Option<f64>,
    pub bear_call_breakeven: Option<f64>,
    pub bear_call_annualized_return: Option<f64>,
    pub iron_condor_short_put_strike: Option<f64>,
    pub iron_condor_long_put_strike: Option<f64>,
    pub iron_condor_short_call_strike: Option<f64>,
    pub iron_condor_long_call_strike: Option<f64>,
    pub iron_condor_net_credit: Option<f64>,
    pub iron_condor_max_loss: Option<f64>,
    pub iron_condor_breakeven_low: Option<f64>,
    pub iron_condor_breakeven_high: Option<f64>,
    pub iron_condor_annualized_return: Option<f64>,
    pub bull_call_long_strike: Option<f64>,
    pub bull_call_short_strike: Option<f64>,
    pub bull_call_net_debit: Option<f64>,
    pub bull_call_max_profit: Option<f64>,
    pub bull_call_max_loss: Option<f64>,
    pub bull_call_breakeven: Option<f64>,
    pub bull_call_annualized_return: Option<f64>,
    pub calendar_call_strike: Option<f64>,
    pub calendar_call_net_debit: Option<f64>,
    pub calendar_call_theta_carry_per_day: Option<f64>,
    pub calendar_call_annualized_carry: Option<f64>,
    pub calendar_call_max_loss: Option<f64>,
    pub pmcc_near_strike: Option<f64>,
    pub pmcc_far_strike: Option<f64>,
    pub pmcc_net_debit: Option<f64>,
    pub pmcc_theta_carry_per_day: Option<f64>,
    pub pmcc_annualized_carry: Option<f64>,
    pub pmcc_max_loss: Option<f64>,
    pub report_json: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrontAtmIvRankRow {
    pub symbol: String,
    pub sample_count: usize,
    pub current_captured_at: String,
    pub current_front_expiry: String,
    pub current_front_atm_iv: f64,
    pub min_front_atm_iv: f64,
    pub max_front_atm_iv: f64,
    pub iv_rank: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketExtremeMetricStat {
    pub current: f64,
    pub mean: f64,
    pub std_dev: f64,
    pub z_score: Option<f64>,
    pub sample_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketExtremeRow {
    pub symbol: String,
    pub sample_count: usize,
    pub current_captured_at: String,
    pub current_front_expiry: String,
    pub delta_skew: Option<MarketExtremeMetricStat>,
    pub otm_skew: Option<MarketExtremeMetricStat>,
    pub front_atm_iv: MarketExtremeMetricStat,
    pub term_structure_change_from_front: Option<MarketExtremeMetricStat>,
    pub open_interest_bias_ratio: Option<MarketExtremeMetricStat>,
    pub otm_open_interest_bias_ratio: Option<MarketExtremeMetricStat>,
    pub average_iv_bias: Option<MarketExtremeMetricStat>,
    pub otm_average_iv_bias: Option<MarketExtremeMetricStat>,
}

pub struct SignalSnapshotStore {
    conn: Connection,
}

impl SignalSnapshotStore {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database {}", path.display()))?;
        let store = Self { conn };
        store.create_tables()?;
        Ok(store)
    }

    pub fn open_default() -> Result<Self> {
        Self::open(&default_db_path()?)
    }

    pub fn record_market_tone(&self, captured_at: &str, view: &MarketToneView) -> Result<()> {
        self.conn.execute(
            "INSERT INTO market_tone_snapshots (
                captured_at,
                symbol,
                front_expiry,
                delta_skew,
                otm_skew,
                front_atm_iv,
                farthest_atm_iv,
                term_structure_change_from_front,
                put_wing_slope,
                call_wing_slope,
                open_interest_bias_ratio,
                otm_open_interest_bias_ratio,
                average_iv_bias,
                otm_average_iv_bias,
                downside_protection,
                term_structure_shape,
                wing_shape,
                positioning_bias,
                overall_tone,
                summary_sentence
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20
            )",
            params![
                captured_at,
                view.underlying_symbol,
                view.front_expiry,
                view.summary.delta_skew,
                view.summary.otm_skew,
                view.summary.front_atm_iv,
                view.summary.farthest_atm_iv,
                view.summary.term_structure_change_from_front,
                view.summary.put_wing_slope,
                view.summary.call_wing_slope,
                view.summary.open_interest_bias_ratio,
                view.summary.otm_open_interest_bias_ratio,
                view.summary.average_iv_bias,
                view.summary.otm_average_iv_bias,
                view.summary.downside_protection,
                view.summary.term_structure_shape,
                view.summary.wing_shape,
                view.summary.positioning_bias,
                view.summary.overall_tone,
                view.summary.summary_sentence,
            ],
        )?;

        for point in &view.term_structure.points {
            self.conn.execute(
                "INSERT INTO term_structure_snapshots (
                    captured_at,
                    symbol,
                    front_expiry,
                    expiry,
                    days_to_expiry,
                    atm_call_iv,
                    atm_put_iv,
                    atm_iv
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    captured_at,
                    view.underlying_symbol,
                    view.front_expiry,
                    point.expiry,
                    point.days_to_expiry,
                    point.atm_call_iv,
                    point.atm_put_iv,
                    point.atm_iv,
                ],
            )?;
        }

        let delta25_put = view.skew.delta_put.as_ref();
        let delta25_call = view.skew.delta_call.as_ref();
        let otm5_put = find_smile_point(&view.smile.put_points, 0.05);
        let otm5_call = find_smile_point(&view.smile.call_points, 0.05);
        let otm10_put = find_smile_point(&view.smile.put_points, 0.10);
        let otm10_call = find_smile_point(&view.smile.call_points, 0.10);

        self.conn.execute(
            "INSERT INTO front_structure_anchors (
                captured_at,
                symbol,
                front_expiry,
                delta25_put_symbol,
                delta25_put_iv,
                delta25_call_symbol,
                delta25_call_iv,
                otm5_put_symbol,
                otm5_put_iv,
                otm5_call_symbol,
                otm5_call_iv,
                otm10_put_symbol,
                otm10_put_iv,
                otm10_call_symbol,
                otm10_call_iv
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15
            )",
            params![
                captured_at,
                view.underlying_symbol,
                view.front_expiry,
                delta25_put.map(|point| point.option_symbol.as_str()),
                delta25_put.map(|point| point.implied_volatility),
                delta25_call.map(|point| point.option_symbol.as_str()),
                delta25_call.map(|point| point.implied_volatility),
                otm5_put.map(|point| point.option_symbol.as_str()),
                otm5_put.map(|point| point.implied_volatility),
                otm5_call.map(|point| point.option_symbol.as_str()),
                otm5_call.map(|point| point.implied_volatility),
                otm10_put.map(|point| point.option_symbol.as_str()),
                otm10_put.map(|point| point.implied_volatility),
                otm10_call.map(|point| point.option_symbol.as_str()),
                otm10_call.map(|point| point.implied_volatility),
            ],
        )?;
        Ok(())
    }

    pub fn record_strategy_monitor_snapshot(
        &self,
        input: &StrategyMonitorSnapshotInput,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO strategy_monitor_snapshots (
                captured_at,
                symbol,
                near_expiry,
                near_days_to_expiry,
                far_expiry,
                far_days_to_expiry,
                bull_put_short_strike,
                bull_put_long_strike,
                bull_put_net_credit,
                bull_put_max_loss,
                bull_put_breakeven,
                bull_put_annualized_return,
                bear_call_short_strike,
                bear_call_long_strike,
                bear_call_net_credit,
                bear_call_max_loss,
                bear_call_breakeven,
                bear_call_annualized_return,
                iron_condor_short_put_strike,
                iron_condor_long_put_strike,
                iron_condor_short_call_strike,
                iron_condor_long_call_strike,
                iron_condor_net_credit,
                iron_condor_max_loss,
                iron_condor_breakeven_low,
                iron_condor_breakeven_high,
                iron_condor_annualized_return,
                bull_call_long_strike,
                bull_call_short_strike,
                bull_call_net_debit,
                bull_call_max_profit,
                bull_call_max_loss,
                bull_call_breakeven,
                bull_call_annualized_return,
                calendar_call_strike,
                calendar_call_net_debit,
                calendar_call_theta_carry_per_day,
                calendar_call_annualized_carry,
                calendar_call_max_loss,
                pmcc_near_strike,
                pmcc_far_strike,
                pmcc_net_debit,
                pmcc_theta_carry_per_day,
                pmcc_annualized_carry,
                pmcc_max_loss,
                report_json
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34,
                ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42, ?43, ?44, ?45, ?46
            )",
            params![
                input.captured_at,
                input.symbol,
                input.near_expiry,
                input.near_days_to_expiry,
                input.far_expiry,
                input.far_days_to_expiry,
                input.bull_put_short_strike,
                input.bull_put_long_strike,
                input.bull_put_net_credit,
                input.bull_put_max_loss,
                input.bull_put_breakeven,
                input.bull_put_annualized_return,
                input.bear_call_short_strike,
                input.bear_call_long_strike,
                input.bear_call_net_credit,
                input.bear_call_max_loss,
                input.bear_call_breakeven,
                input.bear_call_annualized_return,
                input.iron_condor_short_put_strike,
                input.iron_condor_long_put_strike,
                input.iron_condor_short_call_strike,
                input.iron_condor_long_call_strike,
                input.iron_condor_net_credit,
                input.iron_condor_max_loss,
                input.iron_condor_breakeven_low,
                input.iron_condor_breakeven_high,
                input.iron_condor_annualized_return,
                input.bull_call_long_strike,
                input.bull_call_short_strike,
                input.bull_call_net_debit,
                input.bull_call_max_profit,
                input.bull_call_max_loss,
                input.bull_call_breakeven,
                input.bull_call_annualized_return,
                input.calendar_call_strike,
                input.calendar_call_net_debit,
                input.calendar_call_theta_carry_per_day,
                input.calendar_call_annualized_carry,
                input.calendar_call_max_loss,
                input.pmcc_near_strike,
                input.pmcc_far_strike,
                input.pmcc_net_debit,
                input.pmcc_theta_carry_per_day,
                input.pmcc_annualized_carry,
                input.pmcc_max_loss,
                input.report_json,
            ],
        )?;
        Ok(())
    }

    pub fn list_strategy_monitor_snapshots(
        &self,
        symbol: Option<&str>,
        limit: usize,
    ) -> Result<Vec<StrategyMonitorSnapshotRow>> {
        let mut stmt = if symbol.is_some() {
            self.conn.prepare(
                "SELECT
                    id,
                    captured_at,
                    symbol,
                    near_expiry,
                    near_days_to_expiry,
                    far_expiry,
                    far_days_to_expiry,
                    bull_put_net_credit,
                    bear_call_net_credit,
                    iron_condor_net_credit,
                    bull_call_net_debit,
                    calendar_call_net_debit,
                    pmcc_net_debit
                 FROM strategy_monitor_snapshots
                 WHERE symbol = ?1
                 ORDER BY captured_at DESC, id DESC
                 LIMIT ?2",
            )?
        } else {
            self.conn.prepare(
                "SELECT
                    id,
                    captured_at,
                    symbol,
                    near_expiry,
                    near_days_to_expiry,
                    far_expiry,
                    far_days_to_expiry,
                    bull_put_net_credit,
                    bear_call_net_credit,
                    iron_condor_net_credit,
                    bull_call_net_debit,
                    calendar_call_net_debit,
                    pmcc_net_debit
                 FROM strategy_monitor_snapshots
                 ORDER BY captured_at DESC, id DESC
                 LIMIT ?1",
            )?
        };

        let mut result = Vec::new();
        if let Some(symbol) = symbol {
            let rows = stmt.query_map(params![symbol, limit as i64], |row| {
                Ok(StrategyMonitorSnapshotRow {
                    id: row.get(0)?,
                    captured_at: row.get(1)?,
                    symbol: row.get(2)?,
                    near_expiry: row.get(3)?,
                    near_days_to_expiry: row.get(4)?,
                    far_expiry: row.get(5)?,
                    far_days_to_expiry: row.get(6)?,
                    bull_put_net_credit: row.get(7)?,
                    bear_call_net_credit: row.get(8)?,
                    iron_condor_net_credit: row.get(9)?,
                    bull_call_net_debit: row.get(10)?,
                    calendar_call_net_debit: row.get(11)?,
                    pmcc_net_debit: row.get(12)?,
                })
            })?;
            for row in rows {
                result.push(row?);
            }
        } else {
            let rows = stmt.query_map(params![limit as i64], |row| {
                Ok(StrategyMonitorSnapshotRow {
                    id: row.get(0)?,
                    captured_at: row.get(1)?,
                    symbol: row.get(2)?,
                    near_expiry: row.get(3)?,
                    near_days_to_expiry: row.get(4)?,
                    far_expiry: row.get(5)?,
                    far_days_to_expiry: row.get(6)?,
                    bull_put_net_credit: row.get(7)?,
                    bear_call_net_credit: row.get(8)?,
                    iron_condor_net_credit: row.get(9)?,
                    bull_call_net_debit: row.get(10)?,
                    calendar_call_net_debit: row.get(11)?,
                    pmcc_net_debit: row.get(12)?,
                })
            })?;
            for row in rows {
                result.push(row?);
            }
        }
        Ok(result)
    }

    pub fn list_market_tone_snapshots(
        &self,
        symbol: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MarketToneSnapshotRow>> {
        let mut stmt = if symbol.is_some() {
            self.conn.prepare(
                "SELECT
                    id,
                    captured_at,
                    symbol,
                    front_expiry,
                    delta_skew,
                    otm_skew,
                    front_atm_iv,
                    farthest_atm_iv,
                    term_structure_change_from_front,
                    open_interest_bias_ratio,
                    otm_open_interest_bias_ratio,
                    overall_tone,
                    summary_sentence
                 FROM market_tone_snapshots
                 WHERE symbol = ?1
                 ORDER BY captured_at DESC, id DESC
                 LIMIT ?2",
            )?
        } else {
            self.conn.prepare(
                "SELECT
                    id,
                    captured_at,
                    symbol,
                    front_expiry,
                    delta_skew,
                    otm_skew,
                    front_atm_iv,
                    farthest_atm_iv,
                    term_structure_change_from_front,
                    open_interest_bias_ratio,
                    otm_open_interest_bias_ratio,
                    overall_tone,
                    summary_sentence
                 FROM market_tone_snapshots
                 ORDER BY captured_at DESC, id DESC
                 LIMIT ?1",
            )?
        };

        let mapper = |row: &rusqlite::Row<'_>| {
            Ok(MarketToneSnapshotRow {
                id: row.get(0)?,
                captured_at: row.get(1)?,
                symbol: row.get(2)?,
                front_expiry: row.get(3)?,
                delta_skew: row.get(4)?,
                otm_skew: row.get(5)?,
                front_atm_iv: row.get(6)?,
                farthest_atm_iv: row.get(7)?,
                term_structure_change_from_front: row.get(8)?,
                open_interest_bias_ratio: row.get(9)?,
                otm_open_interest_bias_ratio: row.get(10)?,
                overall_tone: row.get(11)?,
                summary_sentence: row.get(12)?,
            })
        };

        let rows = if let Some(symbol) = symbol {
            stmt.query_map(params![symbol, limit as i64], mapper)?
        } else {
            stmt.query_map(params![limit as i64], mapper)?
        };

        let mut snapshots = Vec::new();
        for row in rows {
            snapshots.push(row?);
        }
        Ok(snapshots)
    }

    pub fn list_symbols(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT symbol FROM market_tone_snapshots ORDER BY symbol ASC")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut symbols = Vec::new();
        for row in rows {
            symbols.push(row?);
        }
        Ok(symbols)
    }

    pub fn compute_front_atm_iv_rank(
        &self,
        symbol: &str,
        limit: usize,
    ) -> Result<Option<FrontAtmIvRankRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                captured_at,
                front_expiry,
                front_atm_iv
             FROM market_tone_snapshots
             WHERE symbol = ?1
             ORDER BY captured_at DESC, id DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![symbol, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, f64>(2)?,
            ))
        })?;

        let mut samples = Vec::new();
        for row in rows {
            samples.push(row?);
        }

        Ok(compute_front_atm_iv_rank_from_samples(symbol, &samples))
    }

    pub fn compute_market_extreme(
        &self,
        symbol: &str,
        limit: usize,
    ) -> Result<Option<MarketExtremeRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                captured_at,
                front_expiry,
                delta_skew,
                otm_skew,
                front_atm_iv,
                term_structure_change_from_front,
                open_interest_bias_ratio,
                otm_open_interest_bias_ratio,
                average_iv_bias,
                otm_average_iv_bias
             FROM market_tone_snapshots
             WHERE symbol = ?1
             ORDER BY captured_at DESC, id DESC
             LIMIT ?2",
        )?;

        let rows = stmt.query_map(params![symbol, limit as i64], |row| {
            Ok(MarketExtremeSample {
                captured_at: row.get(0)?,
                front_expiry: row.get(1)?,
                delta_skew: row.get(2)?,
                otm_skew: row.get(3)?,
                front_atm_iv: row.get(4)?,
                term_structure_change_from_front: row.get(5)?,
                open_interest_bias_ratio: row.get(6)?,
                otm_open_interest_bias_ratio: row.get(7)?,
                average_iv_bias: row.get(8)?,
                otm_average_iv_bias: row.get(9)?,
            })
        })?;

        let mut samples = Vec::new();
        for row in rows {
            samples.push(row?);
        }

        Ok(compute_market_extreme_from_samples(symbol, &samples))
    }

    fn create_tables(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS market_tone_snapshots (
                id                              INTEGER PRIMARY KEY AUTOINCREMENT,
                captured_at                     TEXT NOT NULL,
                symbol                          TEXT NOT NULL,
                front_expiry                    TEXT NOT NULL,
                delta_skew                      REAL,
                otm_skew                        REAL,
                front_atm_iv                    REAL NOT NULL,
                farthest_atm_iv                 REAL,
                term_structure_change_from_front REAL,
                put_wing_slope                  REAL,
                call_wing_slope                 REAL,
                open_interest_bias_ratio        REAL,
                otm_open_interest_bias_ratio    REAL,
                average_iv_bias                 REAL,
                otm_average_iv_bias             REAL,
                downside_protection             TEXT NOT NULL,
                term_structure_shape            TEXT NOT NULL,
                wing_shape                      TEXT NOT NULL,
                positioning_bias                TEXT NOT NULL,
                overall_tone                    TEXT NOT NULL,
                summary_sentence                TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_market_tone_symbol_time
                ON market_tone_snapshots(symbol, captured_at);

            CREATE TABLE IF NOT EXISTS term_structure_snapshots (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                captured_at      TEXT NOT NULL,
                symbol           TEXT NOT NULL,
                front_expiry     TEXT NOT NULL,
                expiry           TEXT NOT NULL,
                days_to_expiry   INTEGER NOT NULL,
                atm_call_iv      REAL,
                atm_put_iv       REAL,
                atm_iv           REAL NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_term_structure_symbol_time
                ON term_structure_snapshots(symbol, captured_at);

            CREATE TABLE IF NOT EXISTS front_structure_anchors (
                id                   INTEGER PRIMARY KEY AUTOINCREMENT,
                captured_at          TEXT NOT NULL,
                symbol               TEXT NOT NULL,
                front_expiry         TEXT NOT NULL,
                delta25_put_symbol   TEXT,
                delta25_put_iv       REAL,
                delta25_call_symbol  TEXT,
                delta25_call_iv      REAL,
                otm5_put_symbol      TEXT,
                otm5_put_iv          REAL,
                otm5_call_symbol     TEXT,
                otm5_call_iv         REAL,
                otm10_put_symbol     TEXT,
                otm10_put_iv         REAL,
                otm10_call_symbol    TEXT,
                otm10_call_iv        REAL
            );

            CREATE INDEX IF NOT EXISTS idx_front_anchors_symbol_time
                ON front_structure_anchors(symbol, captured_at);

            CREATE TABLE IF NOT EXISTS strategy_monitor_snapshots (
                id                              INTEGER PRIMARY KEY AUTOINCREMENT,
                captured_at                     TEXT NOT NULL,
                symbol                          TEXT NOT NULL,
                near_expiry                     TEXT NOT NULL,
                near_days_to_expiry             INTEGER NOT NULL,
                far_expiry                      TEXT NOT NULL,
                far_days_to_expiry              INTEGER NOT NULL,
                bull_put_short_strike           REAL,
                bull_put_long_strike            REAL,
                bull_put_net_credit             REAL,
                bull_put_max_loss               REAL,
                bull_put_breakeven              REAL,
                bull_put_annualized_return      REAL,
                bear_call_short_strike          REAL,
                bear_call_long_strike           REAL,
                bear_call_net_credit            REAL,
                bear_call_max_loss              REAL,
                bear_call_breakeven             REAL,
                bear_call_annualized_return     REAL,
                iron_condor_short_put_strike    REAL,
                iron_condor_long_put_strike     REAL,
                iron_condor_short_call_strike   REAL,
                iron_condor_long_call_strike    REAL,
                iron_condor_net_credit          REAL,
                iron_condor_max_loss            REAL,
                iron_condor_breakeven_low       REAL,
                iron_condor_breakeven_high      REAL,
                iron_condor_annualized_return   REAL,
                bull_call_long_strike           REAL,
                bull_call_short_strike          REAL,
                bull_call_net_debit             REAL,
                bull_call_max_profit            REAL,
                bull_call_max_loss              REAL,
                bull_call_breakeven             REAL,
                bull_call_annualized_return     REAL,
                calendar_call_strike            REAL,
                calendar_call_net_debit         REAL,
                calendar_call_theta_carry_per_day REAL,
                calendar_call_annualized_carry  REAL,
                calendar_call_max_loss          REAL,
                pmcc_near_strike                REAL,
                pmcc_far_strike                 REAL,
                pmcc_net_debit                  REAL,
                pmcc_theta_carry_per_day        REAL,
                pmcc_annualized_carry           REAL,
                pmcc_max_loss                   REAL,
                report_json                     TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_strategy_monitor_symbol_time
                ON strategy_monitor_snapshots(symbol, captured_at);",
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct MarketExtremeSample {
    captured_at: String,
    front_expiry: String,
    delta_skew: Option<f64>,
    otm_skew: Option<f64>,
    front_atm_iv: f64,
    term_structure_change_from_front: Option<f64>,
    open_interest_bias_ratio: Option<f64>,
    otm_open_interest_bias_ratio: Option<f64>,
    average_iv_bias: Option<f64>,
    otm_average_iv_bias: Option<f64>,
}

fn compute_front_atm_iv_rank_from_samples(
    symbol: &str,
    samples: &[(String, String, f64)],
) -> Option<FrontAtmIvRankRow> {
    let (current_captured_at, current_front_expiry, current_front_atm_iv) =
        samples.first()?.clone();
    let min_front_atm_iv = samples
        .iter()
        .map(|(_, _, value)| *value)
        .fold(f64::INFINITY, f64::min);
    let max_front_atm_iv = samples
        .iter()
        .map(|(_, _, value)| *value)
        .fold(f64::NEG_INFINITY, f64::max);
    let iv_rank = if (max_front_atm_iv - min_front_atm_iv).abs() <= 1.0e-12 {
        None
    } else {
        Some((current_front_atm_iv - min_front_atm_iv) / (max_front_atm_iv - min_front_atm_iv))
    };

    Some(FrontAtmIvRankRow {
        symbol: symbol.to_string(),
        sample_count: samples.len(),
        current_captured_at,
        current_front_expiry,
        current_front_atm_iv,
        min_front_atm_iv,
        max_front_atm_iv,
        iv_rank,
    })
}

fn compute_market_extreme_from_samples(
    symbol: &str,
    samples: &[MarketExtremeSample],
) -> Option<MarketExtremeRow> {
    let current = samples.first()?;
    Some(MarketExtremeRow {
        symbol: symbol.to_string(),
        sample_count: samples.len(),
        current_captured_at: current.captured_at.clone(),
        current_front_expiry: current.front_expiry.clone(),
        delta_skew: compute_metric_stat(
            current.delta_skew,
            samples.iter().filter_map(|sample| sample.delta_skew),
        ),
        otm_skew: compute_metric_stat(
            current.otm_skew,
            samples.iter().filter_map(|sample| sample.otm_skew),
        ),
        front_atm_iv: compute_required_metric_stat(
            current.front_atm_iv,
            samples.iter().map(|sample| sample.front_atm_iv),
        ),
        term_structure_change_from_front: compute_metric_stat(
            current.term_structure_change_from_front,
            samples
                .iter()
                .filter_map(|sample| sample.term_structure_change_from_front),
        ),
        open_interest_bias_ratio: compute_metric_stat(
            current.open_interest_bias_ratio,
            samples
                .iter()
                .filter_map(|sample| sample.open_interest_bias_ratio),
        ),
        otm_open_interest_bias_ratio: compute_metric_stat(
            current.otm_open_interest_bias_ratio,
            samples
                .iter()
                .filter_map(|sample| sample.otm_open_interest_bias_ratio),
        ),
        average_iv_bias: compute_metric_stat(
            current.average_iv_bias,
            samples.iter().filter_map(|sample| sample.average_iv_bias),
        ),
        otm_average_iv_bias: compute_metric_stat(
            current.otm_average_iv_bias,
            samples
                .iter()
                .filter_map(|sample| sample.otm_average_iv_bias),
        ),
    })
}

fn compute_metric_stat(
    current: Option<f64>,
    values: impl Iterator<Item = f64>,
) -> Option<MarketExtremeMetricStat> {
    let current = current?;
    Some(compute_required_metric_stat(current, values))
}

fn compute_required_metric_stat(
    current: f64,
    values: impl Iterator<Item = f64>,
) -> MarketExtremeMetricStat {
    let values: Vec<f64> = values.collect();
    let sample_count = values.len();
    let mean = values.iter().sum::<f64>() / sample_count as f64;
    let variance = values
        .iter()
        .map(|value| {
            let delta = *value - mean;
            delta * delta
        })
        .sum::<f64>()
        / sample_count as f64;
    let std_dev = variance.sqrt();
    let z_score = if std_dev <= 1.0e-12 {
        None
    } else {
        Some((current - mean) / std_dev)
    };

    MarketExtremeMetricStat {
        current,
        mean,
        std_dev,
        z_score,
        sample_count,
    }
}

fn find_smile_point(points: &[SmilePoint], target: f64) -> Option<&SmilePoint> {
    points
        .iter()
        .find(|point| (point.target_otm_percent - target).abs() < 1.0e-9)
}

fn default_db_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("failed to resolve HOME")?;
    Ok(PathBuf::from(home).join(".theta").join("signals.db"))
}

#[cfg(test)]
mod tests {
    use super::{
        MarketExtremeSample, compute_front_atm_iv_rank_from_samples,
        compute_market_extreme_from_samples,
    };

    #[test]
    fn computes_front_atm_iv_rank_from_recent_samples() {
        let row = compute_front_atm_iv_rank_from_samples(
            "TSLA.US",
            &[
                (
                    "2026-03-02T10:00:00Z".to_string(),
                    "2026-03-20".to_string(),
                    0.72,
                ),
                (
                    "2026-03-02T09:55:00Z".to_string(),
                    "2026-03-20".to_string(),
                    0.60,
                ),
                (
                    "2026-03-02T09:50:00Z".to_string(),
                    "2026-03-20".to_string(),
                    0.80,
                ),
            ],
        )
        .expect("expected rank row");

        assert_eq!(row.symbol, "TSLA.US");
        assert_eq!(row.sample_count, 3);
        assert_eq!(row.current_captured_at, "2026-03-02T10:00:00Z");
        assert_eq!(row.current_front_expiry, "2026-03-20");
        assert!((row.current_front_atm_iv - 0.72).abs() < 1.0e-12);
        assert!((row.min_front_atm_iv - 0.60).abs() < 1.0e-12);
        assert!((row.max_front_atm_iv - 0.80).abs() < 1.0e-12);
        assert!((row.iv_rank.expect("rank should exist") - 0.60).abs() < 1.0e-12);
    }

    #[test]
    fn computes_market_extreme_z_scores_from_recent_samples() {
        let row = compute_market_extreme_from_samples(
            "TSLA.US",
            &[
                MarketExtremeSample {
                    captured_at: "2026-03-02T10:00:00Z".to_string(),
                    front_expiry: "2026-03-20".to_string(),
                    delta_skew: Some(0.12),
                    otm_skew: Some(0.18),
                    front_atm_iv: 0.70,
                    term_structure_change_from_front: Some(-0.04),
                    open_interest_bias_ratio: Some(1.8),
                    otm_open_interest_bias_ratio: Some(2.2),
                    average_iv_bias: Some(0.08),
                    otm_average_iv_bias: Some(0.12),
                },
                MarketExtremeSample {
                    captured_at: "2026-03-02T09:55:00Z".to_string(),
                    front_expiry: "2026-03-20".to_string(),
                    delta_skew: Some(0.08),
                    otm_skew: Some(0.12),
                    front_atm_iv: 0.62,
                    term_structure_change_from_front: Some(-0.01),
                    open_interest_bias_ratio: Some(1.4),
                    otm_open_interest_bias_ratio: Some(1.7),
                    average_iv_bias: Some(0.03),
                    otm_average_iv_bias: Some(0.05),
                },
                MarketExtremeSample {
                    captured_at: "2026-03-02T09:50:00Z".to_string(),
                    front_expiry: "2026-03-20".to_string(),
                    delta_skew: Some(0.10),
                    otm_skew: Some(0.15),
                    front_atm_iv: 0.66,
                    term_structure_change_from_front: Some(0.00),
                    open_interest_bias_ratio: Some(1.6),
                    otm_open_interest_bias_ratio: Some(1.9),
                    average_iv_bias: Some(0.05),
                    otm_average_iv_bias: Some(0.07),
                },
            ],
        )
        .expect("expected market extreme row");

        assert_eq!(row.symbol, "TSLA.US");
        assert_eq!(row.sample_count, 3);
        assert_eq!(row.current_captured_at, "2026-03-02T10:00:00Z");
        assert!((row.front_atm_iv.current - 0.70).abs() < 1.0e-12);
        assert!((row.front_atm_iv.mean - 0.66).abs() < 1.0e-12);
        assert!(row.front_atm_iv.z_score.expect("z-score should exist") > 0.0);
        assert!(
            row.delta_skew
                .expect("delta skew stats should exist")
                .z_score
                .expect("z-score should exist")
                > 0.0
        );
        assert!(
            row.otm_average_iv_bias
                .expect("otm iv bias stats should exist")
                .z_score
                .expect("z-score should exist")
                > 0.0
        );
    }
}
