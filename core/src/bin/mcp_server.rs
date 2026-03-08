use anyhow::Result;
use async_trait::async_trait;
use rust_mcp_sdk::{
    McpServer, StdioTransport, ToMcpServerHandler, TransportOptions,
    error::SdkResult,
    macros::{self, mcp_tool},
    mcp_server::{McpServerOptions, ServerHandler, server_runtime},
    schema::*,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::sync::Arc;

use theta::ledger::{AccountSnapshot, Ledger, Position};
use theta::margin_engine::{self, AccountContext};
use theta::portfolio_service;
use theta::risk_engine;
use theta::signal_service::{
    MarketToneRequest, PutCallBiasRequest, SkewSignalRequest, SmileSignalRequest,
    TermStructureRequest, ThetaSignalService,
};
use theta::snapshot_store::{MarketExtremeMetricStat, MarketExtremeRow, SignalSnapshotStore};

#[derive(Clone)]
struct ThetaServerState {
    service: Arc<ThetaSignalService>,
    db_path: std::path::PathBuf,
}

// --- Tool Argument Structs using mcp_tool macro ---

#[mcp_tool(
    name = "get_market_tone",
    description = "Get the current market tone summary and options structure for a given symbol"
)]
#[derive(Debug, Deserialize, Serialize, macros::JsonSchema)]
struct GetMarketToneArgs {
    /// Stock symbol (e.g. TSLA.US)
    symbol: String,
    /// Optional explicit expiry date (e.g. 2026-06-18)
    expiry: Option<String>,
    /// Number of expiries for term structure (default 4)
    expiries_limit: Option<u32>,
}

#[mcp_tool(
    name = "get_signal_history",
    description = "Get historically captured market tone snapshots from the local database"
)]
#[derive(Debug, Deserialize, Serialize, macros::JsonSchema)]
struct GetSignalHistoryArgs {
    /// Optional stock symbol to filter by (e.g. TSLA.US)
    symbol: Option<String>,
    /// Maximum number of records to return (default 20)
    limit: Option<u32>,
}

#[mcp_tool(
    name = "get_skew",
    description = "Get volatility skew analysis for a given symbol"
)]
#[derive(Debug, Deserialize, Serialize, macros::JsonSchema)]
struct GetSkewArgs {
    /// Stock symbol (e.g. TSLA.US)
    symbol: String,
    /// Optional explicit expiry date (e.g. 2026-06-18)
    expiry: Option<String>,
    /// Target absolute delta (default 0.25)
    target_delta: Option<f64>,
    /// Target OTM percent (default 0.05)
    target_otm_percent: Option<f64>,
}

#[mcp_tool(
    name = "get_smile",
    description = "Get volatility smile analysis for a given symbol"
)]
#[derive(Debug, Deserialize, Serialize, macros::JsonSchema)]
struct GetSmileArgs {
    /// Stock symbol (e.g. TSLA.US)
    symbol: String,
    /// Optional explicit expiry date
    expiry: Option<String>,
}

#[mcp_tool(
    name = "get_term_structure",
    description = "Get ATM volatility term structure across expiries"
)]
#[derive(Debug, Deserialize, Serialize, macros::JsonSchema)]
struct GetTermStructureArgs {
    /// Stock symbol (e.g. TSLA.US)
    symbol: String,
    /// Number of listed expiries to fetch (default 4)
    expiries_limit: Option<u32>,
}

#[mcp_tool(
    name = "get_put_call_bias",
    description = "Get volume and open interest bias between puts and calls"
)]
#[derive(Debug, Deserialize, Serialize, macros::JsonSchema)]
struct GetPutCallBiasArgs {
    /// Stock symbol (e.g. TSLA.US)
    symbol: String,
    /// Optional explicit expiry date
    expiry: Option<String>,
    /// Minimum OTM percent for inclusion (default 0.05)
    bias_min_otm_percent: Option<f64>,
}

#[mcp_tool(
    name = "get_market_extreme",
    description = "Screen for symbols hitting generalized market extremes today"
)]
#[derive(Debug, Deserialize, Serialize, macros::JsonSchema)]
struct GetMarketExtremeArgs {
    /// Max results to return (default 20)
    limit: Option<u32>,
}

#[mcp_tool(
    name = "get_relative_extreme",
    description = "Find symbols moving abnormally relative to their own history"
)]
#[derive(Debug, Deserialize, Serialize, macros::JsonSchema)]
struct GetRelativeExtremeArgs {
    /// Max results to return (default 20)
    limit: Option<u32>,
}

#[mcp_tool(
    name = "get_portfolio",
    description = "Get real-time portfolio holdings, P&L, and aggregated Greeks risk"
)]
#[derive(Debug, Deserialize, Serialize, macros::JsonSchema)]
#[serde(deny_unknown_fields)]
struct GetPortfolioArgs {
    /// Associated brokerage account (default: "firstrade", alternative: "longbridge")
    account: Option<String>,
}

#[mcp_tool(
    name = "get_stock_quote",
    description = "Get real-time stock quote for a given symbol"
)]
#[derive(Debug, Deserialize, Serialize, macros::JsonSchema)]
struct GetStockQuoteArgs {
    /// Stock symbol (e.g. TSLA.US)
    symbol: String,
}

// --- Implementation of ServerHandler ---

struct ThetaHandler {
    state: Arc<ThetaServerState>,
}

const MARKET_EXTREME_SAMPLE_LIMIT: usize = 252;
const ABNORMAL_Z_SCORE_THRESHOLD: f64 = 2.0;

impl ThetaHandler {
    fn parse_args<T: serde::de::DeserializeOwned>(
        &self,
        args: Option<serde_json::Map<String, Value>>,
    ) -> Result<T, CallToolError> {
        serde_json::from_value(Value::Object(args.unwrap_or_default()))
            .map_err(|e| CallToolError::from_message(format!("Invalid arguments: {}", e)))
    }

    fn json_content<T: Serialize>(&self, value: &T) -> Result<CallToolResult, CallToolError> {
        let text = serde_json::to_string_pretty(value)
            .map_err(|e| CallToolError::from_message(format!("Failed to serialize result: {e}")))?;
        Ok(CallToolResult::text_content(vec![text.into()]))
    }

    fn tool_error<E: std::fmt::Display>(&self, context: &str, err: E) -> CallToolError {
        CallToolError::from_message(format!("{context}: {err}"))
    }

    async fn resolve_expiry(
        &self,
        symbol: &str,
        expiry: Option<String>,
    ) -> Result<time::Date, CallToolError> {
        if let Some(raw) = expiry {
            theta::market_data::parse_expiry_date(&raw)
                .map_err(|e| self.tool_error("Error parsing expiry", e))
        } else {
            self.state
                .service
                .front_expiry_for_symbol(symbol)
                .await
                .map_err(|e| self.tool_error("Error fetching front expiry", e))
        }
    }
}

#[async_trait]
impl ServerHandler for ThetaHandler {
    async fn handle_list_tools_request(
        &self,
        _request: Option<PaginatedRequestParams>,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<ListToolsResult, RpcError> {
        Ok(ListToolsResult {
            tools: vec![
                GetStockQuoteArgs::tool(),
                GetMarketToneArgs::tool(),
                GetSignalHistoryArgs::tool(),
                GetSkewArgs::tool(),
                GetSmileArgs::tool(),
                GetTermStructureArgs::tool(),
                GetPutCallBiasArgs::tool(),
                GetMarketExtremeArgs::tool(),
                GetRelativeExtremeArgs::tool(),
                GetPortfolioArgs::tool(),
            ],
            meta: None,
            next_cursor: None,
        })
    }

    async fn handle_call_tool_request(
        &self,
        params: CallToolRequestParams,
        _runtime: Arc<dyn McpServer>,
    ) -> Result<CallToolResult, CallToolError> {
        let s = &self.state;
        match params.name.as_str() {
            "get_stock_quote" => {
                let args: GetStockQuoteArgs = self.parse_args(params.arguments)?;
                let quote = s
                    .service
                    .stock_quote(&args.symbol)
                    .await
                    .map_err(|e| self.tool_error("Error fetching stock quote", e))?;
                self.json_content(&quote)
            }
            "get_market_tone" => {
                let args: GetMarketToneArgs = self.parse_args(params.arguments)?;

                let symbol = args.symbol.clone();
                let expiries_limit = args.expiries_limit.unwrap_or(4) as usize;
                let expiry = self.resolve_expiry(&symbol, args.expiry).await?;

                let tone_req = MarketToneRequest {
                    symbol: symbol.clone(),
                    expiry,
                    expiries_limit,
                    rate: None,
                    dividend: 0.0,
                    iv: None,
                    iv_from_market_price: true,
                    target_delta: 0.25,
                    target_otm_percent: 0.05,
                    smile_target_otm_percents: vec![0.05, 0.10, 0.15],
                    bias_min_otm_percent: 0.05,
                };
                let view = s
                    .service
                    .market_tone(tone_req)
                    .await
                    .map_err(|e| self.tool_error("Error computing market tone", e))?;
                self.json_content(&view)
            }
            "get_signal_history" => {
                let args: GetSignalHistoryArgs = self.parse_args(params.arguments)?;

                let limit = args.limit.unwrap_or(20) as usize;
                let store = SignalSnapshotStore::open(&s.db_path)
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;
                let snapshots = store
                    .list_market_tone_snapshots(args.symbol.as_deref(), limit)
                    .map_err(|e| self.tool_error("Error loading signal history", e))?;
                self.json_content(&snapshots)
            }
            "get_skew" => {
                let args: GetSkewArgs = self.parse_args(params.arguments)?;

                let symbol = args.symbol.clone();
                let expiry_date = self.resolve_expiry(&symbol, args.expiry).await?;

                let skew_req = SkewSignalRequest {
                    symbol,
                    expiry: expiry_date,
                    rate: None,
                    dividend: 0.0,
                    iv: None,
                    iv_from_market_price: true,
                    target_delta: args.target_delta.unwrap_or(0.25),
                    target_otm_percent: args.target_otm_percent.unwrap_or(0.05),
                };
                let skew = s
                    .service
                    .skew(skew_req)
                    .await
                    .map_err(|e| self.tool_error("Error computing skew", e))?;
                self.json_content(&skew)
            }
            "get_smile" => {
                let args: GetSmileArgs = self.parse_args(params.arguments)?;

                let symbol = args.symbol.clone();
                let expiry_date = self.resolve_expiry(&symbol, args.expiry).await?;

                let smile_req = SmileSignalRequest {
                    symbol,
                    expiry: expiry_date,
                    rate: None,
                    dividend: 0.0,
                    iv: None,
                    iv_from_market_price: true,
                    target_otm_percents: vec![0.02, 0.05, 0.10, 0.15, 0.20],
                };
                let smile = s
                    .service
                    .smile(smile_req)
                    .await
                    .map_err(|e| self.tool_error("Error computing smile", e))?;
                self.json_content(&smile)
            }
            "get_term_structure" => {
                let args: GetTermStructureArgs = self.parse_args(params.arguments)?;

                let ts_req = TermStructureRequest {
                    symbol: args.symbol.clone(),
                    expiries_limit: args.expiries_limit.unwrap_or(4) as usize,
                    rate: None,
                    dividend: 0.0,
                    iv: None,
                    iv_from_market_price: true,
                };
                let ts = s
                    .service
                    .term_structure(ts_req)
                    .await
                    .map_err(|e| self.tool_error("Error computing term structure", e))?;
                self.json_content(&ts)
            }
            "get_put_call_bias" => {
                let args: GetPutCallBiasArgs = self.parse_args(params.arguments)?;

                let symbol = args.symbol.clone();
                let expiry_date = self.resolve_expiry(&symbol, args.expiry).await?;

                let bias_req = PutCallBiasRequest {
                    symbol,
                    expiry: expiry_date,
                    rate: None,
                    dividend: 0.0,
                    iv: None,
                    iv_from_market_price: true,
                    min_otm_percent: args.bias_min_otm_percent.unwrap_or(0.05),
                };
                let bias = s
                    .service
                    .put_call_bias(bias_req)
                    .await
                    .map_err(|e| self.tool_error("Error computing put/call bias", e))?;
                self.json_content(&bias)
            }
            "get_market_extreme" => {
                let args: GetMarketExtremeArgs = self.parse_args(params.arguments)?;
                let limit = args.limit.unwrap_or(20) as usize;
                let store = SignalSnapshotStore::open(&s.db_path)
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;
                let results = screen_market_extremes(&store, MARKET_EXTREME_SAMPLE_LIMIT, limit)
                    .map_err(|e| self.tool_error("Error screening market extremes", e))?;
                self.json_content(&results)
            }
            "get_relative_extreme" => {
                let args: GetRelativeExtremeArgs = self.parse_args(params.arguments)?;
                let limit = args.limit.unwrap_or(20) as usize;
                let store = SignalSnapshotStore::open(&s.db_path)
                    .map_err(|e| CallToolError::from_message(e.to_string()))?;
                let results = screen_relative_extremes(&store, MARKET_EXTREME_SAMPLE_LIMIT, limit)
                    .map_err(|e| self.tool_error("Error screening relative extremes", e))?;
                self.json_content(&results)
            }
            "get_portfolio" => {
                let args: GetPortfolioArgs = self.parse_args(params.arguments)?;

                // Determine ledger db path based on account arg
                let account_name = args
                    .account
                    .as_deref()
                    .unwrap_or("firstrade")
                    .to_lowercase();
                let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
                let ledger_db_path = std::path::PathBuf::from(&home)
                    .join(".theta")
                    .join("portfolio.db");

                let ledger = Ledger::open(&ledger_db_path).map_err(|e| {
                    CallToolError::from_message(format!(
                        "Failed to open ledger at {:?}: {}",
                        ledger_db_path, e
                    ))
                })?;

                let positions: Vec<Position> = match ledger.calculate_positions(&account_name, None)
                {
                    Ok(p) => p,
                    Err(e) => return Err(self.tool_error("Error loading positions", e)),
                };

                let account_snapshot = latest_account_snapshot_required(&ledger, &account_name)
                    .map_err(CallToolError::from_message)?;

                let account = AccountContext {
                    trade_date_cash: Some(account_snapshot.trade_date_cash),
                    settled_cash: Some(account_snapshot.settled_cash),
                    option_buying_power: account_snapshot.option_buying_power,
                    stock_buying_power: account_snapshot.stock_buying_power,
                    margin_loan: account_snapshot.margin_loan,
                    short_market_value: account_snapshot.short_market_value,
                    margin_enabled: account_snapshot.margin_enabled,
                };

                let analysis_service = self.state.service.analysis();

                let enriched =
                    match portfolio_service::enrich_positions(analysis_service, &positions).await {
                        Ok(e) => e,
                        Err(e) => return Err(self.tool_error("Error enriching positions", e)),
                    };

                let strategies = risk_engine::identify_strategies(&positions);
                let evaluated_strategies =
                    margin_engine::evaluate_strategies(&strategies, &enriched, &account);
                let portfolio_greeks = risk_engine::aggregate_greeks(&enriched);

                let total_margin: f64 = evaluated_strategies
                    .iter()
                    .map(|s| s.margin.margin_required)
                    .sum();
                let unrealized_pnl: f64 = enriched.iter().map(|p| p.unrealized_pnl).sum();

                let report = serde_json::json!({
                    "account": account_snapshot,
                    "positions_count": enriched.len(),
                    "unrealized_pnl": unrealized_pnl,
                    "total_margin_required": total_margin,
                    "portfolio_greeks": portfolio_greeks,
                    "strategies": evaluated_strategies,
                    "enriched_positions": enriched,
                });
                self.json_content(&report)
            }
            _ => Err(CallToolError::unknown_tool(params.name)),
        }
    }
}

// --- Implementation Helpers ---

fn default_db_path() -> Result<std::path::PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    Ok(std::path::PathBuf::from(home)
        .join(".theta")
        .join("signals.db"))
}

fn latest_account_snapshot_required(
    ledger: &Ledger,
    account_id: &str,
) -> std::result::Result<AccountSnapshot, String> {
    match ledger.latest_account_snapshot(account_id) {
        Ok(Some(snapshot)) => Ok(snapshot),
        Ok(None) => Err(format!(
            "Error loading account snapshot: no account snapshot found for account {}",
            account_id
        )),
        Err(e) => Err(format!("Error loading account snapshot: {}", e)),
    }
}

fn screen_market_extremes(
    store: &SignalSnapshotStore,
    sample_limit: usize,
    result_limit: usize,
) -> anyhow::Result<Vec<MarketExtremeRow>> {
    let today = time::OffsetDateTime::now_utc().date();
    screen_market_extremes_for_date(store, sample_limit, result_limit, today)
}

fn screen_market_extremes_for_date(
    store: &SignalSnapshotStore,
    sample_limit: usize,
    result_limit: usize,
    today: time::Date,
) -> anyhow::Result<Vec<MarketExtremeRow>> {
    let mut rows = load_market_extreme_rows(store, sample_limit)?;
    rows.retain(|row| market_extreme_row_is_from_date(row, today));
    rows.sort_by(compare_market_extreme_rows);
    rows.truncate(result_limit);
    Ok(rows)
}

fn screen_relative_extremes(
    store: &SignalSnapshotStore,
    sample_limit: usize,
    result_limit: usize,
) -> anyhow::Result<Vec<MarketExtremeRow>> {
    let mut rows = load_market_extreme_rows(store, sample_limit)?;
    rows.retain(market_extreme_is_abnormal);
    rows.sort_by(compare_market_extreme_rows);
    rows.truncate(result_limit);
    Ok(rows)
}

fn load_market_extreme_rows(
    store: &SignalSnapshotStore,
    sample_limit: usize,
) -> anyhow::Result<Vec<MarketExtremeRow>> {
    let symbols = store.list_symbols()?;
    let mut rows = Vec::new();
    for symbol in symbols {
        if let Some(row) = store.compute_market_extreme(&symbol, sample_limit)? {
            rows.push(row);
        }
    }
    Ok(rows)
}

fn compare_market_extreme_rows(left: &MarketExtremeRow, right: &MarketExtremeRow) -> Ordering {
    market_extreme_score(right)
        .partial_cmp(&market_extreme_score(left))
        .unwrap_or(Ordering::Equal)
        .then_with(|| right.sample_count.cmp(&left.sample_count))
        .then_with(|| left.symbol.cmp(&right.symbol))
}

fn market_extreme_row_is_from_date(row: &MarketExtremeRow, date: time::Date) -> bool {
    row.current_captured_at
        .get(..10)
        .is_some_and(|prefix| prefix == date.to_string())
}

fn market_extreme_score(row: &MarketExtremeRow) -> f64 {
    market_extreme_z_scores(row)
        .into_iter()
        .flatten()
        .map(f64::abs)
        .fold(0.0, f64::max)
}

fn market_extreme_is_abnormal(row: &MarketExtremeRow) -> bool {
    market_extreme_z_scores(row)
        .into_iter()
        .flatten()
        .any(|z| z.abs() >= ABNORMAL_Z_SCORE_THRESHOLD)
}

fn market_extreme_z_scores(row: &MarketExtremeRow) -> [Option<f64>; 6] {
    [
        metric_z_score(row.delta_skew.as_ref()),
        metric_z_score(row.otm_skew.as_ref()),
        row.front_atm_iv.z_score,
        metric_z_score(row.term_structure_change_from_front.as_ref()),
        metric_z_score(row.open_interest_bias_ratio.as_ref()),
        metric_z_score(row.otm_open_interest_bias_ratio.as_ref()),
    ]
}

fn metric_z_score(metric: Option<&MarketExtremeMetricStat>) -> Option<f64> {
    metric.and_then(|metric| metric.z_score)
}

#[tokio::main]
async fn main() -> SdkResult<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Attempt to load environment from standard config paths
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    let config_dir = std::path::PathBuf::from(home).join(".config").join("theta");

    // Try config.env first, then capture-signals.env
    let config_paths = [
        config_dir.join("config.env"),
        config_dir.join("capture-signals.env"),
    ];

    for path in config_paths {
        if path.exists() {
            if let Err(e) = dotenvy::from_path(&path) {
                tracing::warn!("Failed to load config from {:?}: {}", path, e);
            } else {
                tracing::info!("Loaded config from {:?}", path);
                break;
            }
        }
    }

    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    tracing::info!("Starting theta MCP server via rust-mcp-sdk");

    tracing::info!("Initializing ThetaSignalService (with 30s timeout)...");
    let service_result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        ThetaSignalService::from_env(),
    )
    .await;

    let service = match service_result {
        Ok(Ok(s)) => Arc::new(s),
        Ok(Err(e)) => {
            tracing::error!("Failed to initialize ThetaSignalService: {}", e);
            return Err(rust_mcp_sdk::error::McpSdkError::Internal {
                description: format!("Service init failed: {}", e),
            });
        }
        Err(_) => {
            tracing::error!("Timeout (30s) reached while initializing ThetaSignalService.");
            return Err(rust_mcp_sdk::error::McpSdkError::Internal {
                description: "Service initialization timed out".to_string(),
            });
        }
    };

    let db_path = default_db_path().map_err(|e| rust_mcp_sdk::error::McpSdkError::Internal {
        description: e.to_string(),
    })?;
    let state = Arc::new(ThetaServerState { service, db_path });

    let server_details = InitializeResult {
        server_info: Implementation {
            name: "theta-vps".into(),
            version: "0.2.0".into(),
            title: Some("Theta Market Analysis Server".into()),
            description: Some("Exposes theta project tools as MCP endpoints".into()),
            icons: vec![],
            website_url: None,
        },
        capabilities: ServerCapabilities {
            tools: Some(ServerCapabilitiesTools { list_changed: None }),
            ..Default::default()
        },
        protocol_version: ProtocolVersion::V2025_06_18.into(),
        instructions: None,
        meta: None,
    };

    let transport = StdioTransport::new(TransportOptions::default()).map_err(|e| {
        rust_mcp_sdk::error::McpSdkError::Internal {
            description: e.to_string(),
        }
    })?;

    let handler = ThetaHandler { state }.to_mcp_server_handler();

    let server = server_runtime::create_server(McpServerOptions {
        transport,
        handler,
        server_details,
        task_store: None,
        client_task_store: None,
    });

    tracing::info!("Running MCP server on stdio...");
    server.start().await
}

#[cfg(test)]
mod tests {
    use super::{
        GetPortfolioArgs, latest_account_snapshot_required, market_extreme_is_abnormal,
        market_extreme_row_is_from_date, market_extreme_score, screen_market_extremes,
        screen_market_extremes_for_date, screen_relative_extremes,
    };
    use theta::domain::{
        MarketToneSummary, MarketToneView, PutCallBiasView, PutCallSideTotals, SkewSignalView,
        SmileSignalView, TermStructureView,
    };
    use theta::ledger::Ledger;
    use theta::snapshot_store::{MarketExtremeMetricStat, MarketExtremeRow, SignalSnapshotStore};

    #[test]
    fn latest_account_snapshot_required_errors_when_snapshot_is_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("portfolio.db");
        let ledger = Ledger::open(&db_path).expect("ledger opens");

        let err = latest_account_snapshot_required(&ledger, "firstrade").expect_err("no snapshot");
        assert!(err.contains("no account snapshot found for account firstrade"));
    }

    #[test]
    fn latest_account_snapshot_required_returns_existing_snapshot() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("portfolio.db");
        let ledger = Ledger::open(&db_path).expect("ledger opens");
        ledger
            .record_account_snapshot(
                "2026-03-08T09:30:00Z",
                10_000.0,
                9_500.0,
                Some(8_000.0),
                Some(12_000.0),
                Some(500.0),
                None,
                true,
                "seed snapshot",
                "firstrade",
            )
            .expect("snapshot records");

        let snapshot =
            latest_account_snapshot_required(&ledger, "firstrade").expect("snapshot exists");
        assert_eq!(snapshot.account_id, "firstrade");
        assert_eq!(snapshot.settled_cash, 9_500.0);
    }

    #[test]
    fn get_portfolio_args_reject_unknown_margin_ratio() {
        let err = serde_json::from_value::<GetPortfolioArgs>(serde_json::json!({
            "account": "firstrade",
            "margin_ratio": 0.3
        }))
        .expect_err("margin_ratio should be rejected");
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn market_extreme_helpers_rank_and_filter_by_max_abs_z_score() {
        let strong = sample_market_extreme_row("TSLA.US", Some(2.8), Some(0.7));
        let mild = sample_market_extreme_row("QQQ.US", Some(1.6), Some(1.1));
        let flat = sample_market_extreme_row("SPY.US", None, None);

        assert!(market_extreme_score(&strong) > market_extreme_score(&mild));
        assert!(market_extreme_is_abnormal(&strong));
        assert!(!market_extreme_is_abnormal(&mild));
        assert_eq!(market_extreme_score(&flat), 0.0);
    }

    #[test]
    fn screening_helpers_sort_by_severity_and_respect_result_limit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("signals.db");
        let store = SignalSnapshotStore::open(&db_path).expect("store opens");
        seed_market_extreme_samples(&store, "QQQ.US", 1.2, 1.1, 3);
        seed_market_extreme_samples(&store, "TSLA.US", 2.9, 0.4, 3);
        seed_market_extreme_samples(&store, "SPY.US", 2.1, 0.3, 3);

        let market = screen_market_extremes(&store, 252, 2).expect("market extremes");
        let relative = screen_relative_extremes(&store, 252, 10).expect("relative extremes");

        assert_eq!(market.len(), 2);
        assert!(market[0].sample_count >= market[1].sample_count);
        assert!(market_extreme_score(&market[0]) >= market_extreme_score(&market[1]));
        assert!(relative.iter().all(market_extreme_is_abnormal));
        assert!(
            relative
                .windows(2)
                .all(|pair| { market_extreme_score(&pair[0]) >= market_extreme_score(&pair[1]) })
        );
    }

    #[test]
    fn market_extreme_screening_only_returns_rows_from_requested_day() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("signals.db");
        let store = SignalSnapshotStore::open(&db_path).expect("store opens");
        let today =
            time::Date::from_calendar_date(2026, time::Month::March, 8).expect("valid date");
        seed_market_extreme_samples(&store, "TSLA.US", 2.9, 0.4, 3);
        seed_market_extreme_samples(&store, "QQQ.US", 1.2, 1.1, 1);

        let rows = screen_market_extremes_for_date(&store, 252, 10, today)
            .expect("market extremes for date");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].symbol, "TSLA.US");
        assert!(market_extreme_row_is_from_date(&rows[0], today));
    }

    fn sample_market_extreme_row(
        symbol: &str,
        front_iv_z: Option<f64>,
        delta_skew_z: Option<f64>,
    ) -> MarketExtremeRow {
        MarketExtremeRow {
            symbol: symbol.to_string(),
            sample_count: 252,
            current_captured_at: "2026-03-08T09:30:00Z".to_string(),
            current_front_expiry: "2026-03-20".to_string(),
            delta_skew: delta_skew_z.map(|z| sample_metric_stat(0.0, z)),
            otm_skew: None,
            front_atm_iv: MarketExtremeMetricStat {
                current: 0.0,
                mean: 0.0,
                std_dev: 1.0,
                z_score: front_iv_z,
                sample_count: 252,
            },
            term_structure_change_from_front: None,
            open_interest_bias_ratio: None,
            otm_open_interest_bias_ratio: None,
        }
    }

    fn sample_metric_stat(current: f64, z_score: f64) -> MarketExtremeMetricStat {
        MarketExtremeMetricStat {
            current,
            mean: 0.0,
            std_dev: 1.0,
            z_score: Some(z_score),
            sample_count: 252,
        }
    }

    fn seed_market_extreme_samples(
        store: &SignalSnapshotStore,
        symbol: &str,
        current_front_atm_iv: f64,
        baseline_front_atm_iv: f64,
        start_day: u8,
    ) {
        for idx in 0..6 {
            let captured_at = format!("2026-03-{:02}T09:30:00Z", usize::from(start_day) + idx);
            let front_atm_iv = if idx == 5 {
                current_front_atm_iv
            } else {
                baseline_front_atm_iv
            };
            let view = MarketToneView {
                underlying_symbol: symbol.to_string(),
                front_expiry: "2026-03-20".to_string(),
                summary: MarketToneSummary {
                    delta_skew: Some(front_atm_iv / 10.0),
                    otm_skew: None,
                    front_atm_iv,
                    farthest_atm_iv: None,
                    term_structure_change_from_front: None,
                    put_wing_slope: None,
                    call_wing_slope: None,
                    open_interest_bias_ratio: None,
                    otm_open_interest_bias_ratio: None,
                    average_iv_bias: None,
                    otm_average_iv_bias: None,
                    downside_protection: "balanced".to_string(),
                    term_structure_shape: "flat".to_string(),
                    wing_shape: "balanced".to_string(),
                    positioning_bias: "balanced".to_string(),
                    overall_tone: "neutral".to_string(),
                    summary_sentence: "seed".to_string(),
                },
                skew: SkewSignalView {
                    underlying_symbol: symbol.to_string(),
                    underlying_price: "0".to_string(),
                    expiry: "2026-03-20".to_string(),
                    days_to_expiry: 12,
                    rate: 0.0,
                    rate_source: "seed".to_string(),
                    target_delta: 0.25,
                    target_otm_percent: 0.05,
                    atm_strike_price: "0".to_string(),
                    atm_iv: front_atm_iv,
                    delta_put: None,
                    delta_call: None,
                    delta_skew: Some(front_atm_iv / 10.0),
                    delta_put_wing_vs_atm: None,
                    delta_call_wing_vs_atm: None,
                    otm_put: None,
                    otm_call: None,
                    otm_skew: None,
                    otm_put_wing_vs_atm: None,
                    otm_call_wing_vs_atm: None,
                },
                smile: SmileSignalView {
                    underlying_symbol: symbol.to_string(),
                    underlying_price: "0".to_string(),
                    expiry: "2026-03-20".to_string(),
                    days_to_expiry: 12,
                    rate: 0.0,
                    rate_source: "seed".to_string(),
                    atm_strike_price: "0".to_string(),
                    atm_iv: front_atm_iv,
                    put_points: vec![],
                    call_points: vec![],
                    put_wing_slope: None,
                    call_wing_slope: None,
                },
                put_call_bias: PutCallBiasView {
                    underlying_symbol: symbol.to_string(),
                    underlying_price: "0".to_string(),
                    expiry: "2026-03-20".to_string(),
                    days_to_expiry: 12,
                    rate: 0.0,
                    rate_source: "seed".to_string(),
                    min_otm_percent: 0.05,
                    all_puts: empty_put_call_totals(),
                    all_calls: empty_put_call_totals(),
                    otm_puts: empty_put_call_totals(),
                    otm_calls: empty_put_call_totals(),
                    volume_bias_ratio: None,
                    open_interest_bias_ratio: None,
                    otm_volume_bias_ratio: None,
                    otm_open_interest_bias_ratio: None,
                    average_iv_bias: None,
                    otm_average_iv_bias: None,
                },
                term_structure: TermStructureView {
                    underlying_symbol: symbol.to_string(),
                    target_expiries: 0,
                    points: vec![],
                },
            };
            store
                .record_market_tone(&captured_at, &view)
                .expect("seed record");
        }
    }

    fn empty_put_call_totals() -> PutCallSideTotals {
        PutCallSideTotals {
            contracts: 0,
            total_volume: 0,
            total_open_interest: 0,
            average_iv: None,
        }
    }
}
