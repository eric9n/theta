use anyhow::Result;
use async_trait::async_trait;
use rust_mcp_sdk::{
    error::SdkResult,
    macros::{self, mcp_tool},
    mcp_server::{server_runtime, McpServerOptions, ServerHandler},
    schema::*,
    StdioTransport, TransportOptions,
    McpServer, ToMcpServerHandler,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

use theta::ledger::{AccountSnapshot, Ledger, Position};
use theta::portfolio_service;
use theta::risk_engine;
use theta::margin_engine::{self, AccountContext};
use theta::signal_service::{
    MarketToneRequest, ThetaSignalService, SkewSignalRequest, TermStructureRequest,
    SmileSignalRequest, PutCallBiasRequest,
};
use theta::snapshot_store::SignalSnapshotStore;

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
struct GetPortfolioArgs {
    /// Associated brokerage account (default: "firstrade", alternative: "longbridge")
    account: Option<String>,
    /// Initial margin ratio assumption (default 0.3)
    margin_ratio: Option<f64>,
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

impl ThetaHandler {
    fn parse_args<T: serde::de::DeserializeOwned>(&self, args: Option<serde_json::Map<String, Value>>) -> Result<T, CallToolError> {
        serde_json::from_value(Value::Object(args.unwrap_or_default()))
            .map_err(|e| CallToolError::from_message(format!("Invalid arguments: {}", e)))
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
                
                match s.service.stock_quote(&args.symbol).await {
                    Ok(quote) => Ok(CallToolResult::text_content(vec![
                        serde_json::to_string_pretty(&quote).unwrap_or_default().into(),
                    ])),
                    Err(e) => Ok(CallToolResult::text_content(vec![format!("Error: {}", e).into()])),
                }
            }
            "get_market_tone" => {
                let args: GetMarketToneArgs = self.parse_args(params.arguments)?;
                
                let symbol = args.symbol.clone();
                let expiries_limit = args.expiries_limit.unwrap_or(4) as usize;
                
                let expiry = if let Some(dt) = args.expiry {
                    match theta::market_data::parse_expiry_date(&dt) {
                        Ok(exp) => exp,
                        Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error parsing expiry: {}", e).into()])),
                    }
                } else {
                    match s.service.front_expiry_for_symbol(&symbol).await {
                        Ok(exp) => exp,
                        Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error fetching front expiry: {}", e).into()])),
                    }
                };

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

                match s.service.market_tone(tone_req).await {
                    Ok(view) => Ok(CallToolResult::text_content(vec![
                        serde_json::to_string_pretty(&view).unwrap_or_default().into(),
                    ])),
                    Err(e) => Ok(CallToolResult::text_content(vec![format!("Error: {}", e).into()])),
                }
            }
            "get_signal_history" => {
                let args: GetSignalHistoryArgs = self.parse_args(params.arguments)?;
                
                let limit = args.limit.unwrap_or(20) as usize;
                let store = SignalSnapshotStore::open(&s.db_path).map_err(|e| CallToolError::from_message(e.to_string()))?;
                match store.list_market_tone_snapshots(args.symbol.as_deref(), limit) {
                    Ok(snapshots) => Ok(CallToolResult::text_content(vec![
                        serde_json::to_string_pretty(&snapshots).unwrap_or_default().into(),
                    ])),
                    Err(e) => Ok(CallToolResult::text_content(vec![format!("Error: {}", e).into()])),
                }
            }
            "get_skew" => {
                let args: GetSkewArgs = self.parse_args(params.arguments)?;
                
                let symbol = args.symbol.clone();
                let expiry_date = if let Some(dt) = args.expiry {
                    match theta::market_data::parse_expiry_date(&dt) {
                        Ok(exp) => exp,
                        Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error parsing expiry: {}", e).into()])),
                    }
                } else {
                    match s.service.front_expiry_for_symbol(&symbol).await {
                        Ok(exp) => exp,
                        Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error fetching front expiry: {}", e).into()])),
                    }
                };

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

                match s.service.skew(skew_req).await {
                    Ok(skew) => Ok(CallToolResult::text_content(vec![
                        serde_json::to_string_pretty(&skew).unwrap_or_default().into(),
                    ])),
                    Err(e) => Ok(CallToolResult::text_content(vec![format!("Error: {}", e).into()])),
                }
            }
            "get_smile" => {
                let args: GetSmileArgs = self.parse_args(params.arguments)?;
                
                let symbol = args.symbol.clone();
                let expiry_date = if let Some(dt) = args.expiry {
                    match theta::market_data::parse_expiry_date(&dt) {
                        Ok(exp) => exp,
                        Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error parsing expiry: {}", e).into()])),
                    }
                } else {
                    match s.service.front_expiry_for_symbol(&symbol).await {
                        Ok(exp) => exp,
                        Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error fetching front expiry: {}", e).into()])),
                    }
                };

                let smile_req = SmileSignalRequest {
                    symbol,
                    expiry: expiry_date,
                    rate: None,
                    dividend: 0.0,
                    iv: None,
                    iv_from_market_price: true,
                    target_otm_percents: vec![0.02, 0.05, 0.10, 0.15, 0.20],
                };

                match s.service.smile(smile_req).await {
                    Ok(smile) => Ok(CallToolResult::text_content(vec![
                        serde_json::to_string_pretty(&smile).unwrap_or_default().into(),
                    ])),
                    Err(e) => Ok(CallToolResult::text_content(vec![format!("Error: {}", e).into()])),
                }
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

                match s.service.term_structure(ts_req).await {
                    Ok(ts) => Ok(CallToolResult::text_content(vec![
                        serde_json::to_string_pretty(&ts).unwrap_or_default().into(),
                    ])),
                    Err(e) => Ok(CallToolResult::text_content(vec![format!("Error: {}", e).into()])),
                }
            }
            "get_put_call_bias" => {
                let args: GetPutCallBiasArgs = self.parse_args(params.arguments)?;
                
                let symbol = args.symbol.clone();
                let expiry_date = if let Some(dt) = args.expiry {
                    match theta::market_data::parse_expiry_date(&dt) {
                        Ok(exp) => exp,
                        Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error parsing expiry: {}", e).into()])),
                    }
                } else {
                    match s.service.front_expiry_for_symbol(&symbol).await {
                        Ok(exp) => exp,
                        Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error fetching front expiry: {}", e).into()])),
                    }
                };

                let bias_req = PutCallBiasRequest {
                    symbol,
                    expiry: expiry_date,
                    rate: None,
                    dividend: 0.0,
                    iv: None,
                    iv_from_market_price: true,
                    min_otm_percent: args.bias_min_otm_percent.unwrap_or(0.05),
                };

                match s.service.put_call_bias(bias_req).await {
                    Ok(bias) => Ok(CallToolResult::text_content(vec![
                        serde_json::to_string_pretty(&bias).unwrap_or_default().into(),
                    ])),
                    Err(e) => Ok(CallToolResult::text_content(vec![format!("Error: {}", e).into()])),
                }
            }
            "get_market_extreme" => {
                let args: GetMarketExtremeArgs = self.parse_args(params.arguments)?;
                
                let limit = args.limit.unwrap_or(20) as usize;
                let store = SignalSnapshotStore::open(&s.db_path).map_err(|e| CallToolError::from_message(e.to_string()))?;
                
                let symbols = store.list_symbols().map_err(|e| CallToolError::from_message(e.to_string()))?;
                let mut results = Vec::new();
                for symbol in symbols {
                    if let Ok(Some(row)) = store.compute_market_extreme(&symbol, limit) {
                        results.push(row);
                    }
                }

                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&results).unwrap_or_default().into(),
                ]))
            }
            "get_relative_extreme" => {
                let args: GetRelativeExtremeArgs = self.parse_args(params.arguments)?;
                
                let limit = args.limit.unwrap_or(20) as usize;
                let store = SignalSnapshotStore::open(&s.db_path).map_err(|e| CallToolError::from_message(e.to_string()))?;
                
                let symbols = store.list_symbols().map_err(|e| CallToolError::from_message(e.to_string()))?;
                let mut results = Vec::new();
                for symbol in symbols {
                    if let Ok(Some(row)) = store.compute_market_extreme(&symbol, limit) {
                        if let Some(z) = row.front_atm_iv.z_score {
                             if z.abs() >= 2.0 {
                                 results.push(row);
                             }
                        }
                    }
                }

                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&results).unwrap_or_default().into(),
                ]))
            }
            "get_portfolio" => {
                let args: GetPortfolioArgs = self.parse_args(params.arguments)?;
                
                // Determine ledger db path based on account arg
                let account_name = args.account.as_deref().unwrap_or("firstrade").to_lowercase();
                let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
                let ledger_db_path = if account_name == "longbridge" {
                    std::path::PathBuf::from(&home).join(".theta").join("longbridge.db")
                } else {
                    std::path::PathBuf::from(&home).join(".theta").join("portfolio.db")
                };

                let ledger = Ledger::open(&ledger_db_path).map_err(|e| CallToolError::from_message(format!("Failed to open ledger at {:?}: {}", ledger_db_path, e)))?; 
                
                let positions: Vec<Position> = match ledger.calculate_positions(None) {
                    Ok(p) => p,
                    Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error loading positions: {}", e).into()])),
                };
                
                let account_snapshot = match ledger.latest_account_snapshot() {
                    Ok(Some(a)) => a,
                    _ => AccountSnapshot {
                        id: 0,
                        snapshot_at: "".to_string(),
                        trade_date_cash: 100000.0,
                        settled_cash: 100000.0,
                        option_buying_power: None,
                        stock_buying_power: None,
                        margin_loan: None,
                        short_market_value: None,
                        margin_enabled: true,
                        notes: "dummy snapshot".to_string()
                    }
                };
                
                let account = AccountContext {
                    trade_date_cash: Some(account_snapshot.trade_date_cash),
                    settled_cash: Some(account_snapshot.settled_cash),
                    option_buying_power: account_snapshot.option_buying_power,
                    stock_buying_power: account_snapshot.stock_buying_power,
                    margin_loan: account_snapshot.margin_loan,
                    short_market_value: account_snapshot.short_market_value,
                    margin_enabled: account_snapshot.margin_enabled,
                };
                
                let analysis_service = match theta::analysis_service::ThetaAnalysisService::from_env().await {
                    Ok(service) => service,
                    Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error loading analysis service: {}", e).into()])),
                };
                
                let enriched = match portfolio_service::enrich_positions(&analysis_service, &positions).await {
                    Ok(e) => e,
                    Err(e) => return Ok(CallToolResult::text_content(vec![format!("Error enriching positions: {}", e).into()])),
                };
                
                let strategies = risk_engine::identify_strategies(&positions);
                let evaluated_strategies = margin_engine::evaluate_strategies(&strategies, &enriched, &account);
                let portfolio_greeks = risk_engine::aggregate_greeks(&enriched);
                
                let total_margin: f64 = evaluated_strategies.iter().map(|s| s.margin.margin_required).sum();
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
                
                Ok(CallToolResult::text_content(vec![
                    serde_json::to_string_pretty(&report).unwrap_or_default().into(),
                ]))
            }
            _ => Err(CallToolError::unknown_tool(params.name)),
        }
    }
}

// --- Implementation Helpers ---

fn default_db_path() -> Result<std::path::PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    Ok(std::path::PathBuf::from(home).join(".theta").join("signals.db"))
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
        ThetaSignalService::from_env()
    ).await;

    let service = match service_result {
        Ok(Ok(s)) => Arc::new(s),
        Ok(Err(e)) => {
            tracing::error!("Failed to initialize ThetaSignalService: {}", e);
            return Err(rust_mcp_sdk::error::McpSdkError::Internal { description: format!("Service init failed: {}", e) });
        }
        Err(_) => {
            tracing::error!("Timeout (30s) reached while initializing ThetaSignalService.");
            return Err(rust_mcp_sdk::error::McpSdkError::Internal { description: "Service initialization timed out".to_string() });
        }
    };

    let db_path = default_db_path().map_err(|e| rust_mcp_sdk::error::McpSdkError::Internal { description: e.to_string() })?;
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
        meta: None
    };

    let transport = StdioTransport::new(TransportOptions::default())
        .map_err(|e| rust_mcp_sdk::error::McpSdkError::Internal { description: e.to_string() })?;
    
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
