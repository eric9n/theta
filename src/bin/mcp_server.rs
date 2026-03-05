use anyhow::Result;
use async_trait::async_trait;
use mcp_sdk_rs::{
    error::Error,
    server::{Server, ServerHandler},
    transport::stdio::StdioTransport,
    types::{
        ClientCapabilities, Implementation, ServerCapabilities, Tool, ToolSchema,
    },
};
use clap::Parser;
use serde_json::{json, Value};
use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use theta::ledger::{AccountSnapshot, Ledger};
use theta::portfolio_service;
use theta::risk_domain::EnrichedPosition;
use theta::risk_engine;
use theta::margin_engine::{self, AccountContext};
use theta::signal_service::{
    MarketToneRequest, PutCallBiasRequest, SkewSignalRequest,
    SmileSignalRequest, TermStructureRequest, ThetaSignalService,
};
use theta::snapshot_store::SignalSnapshotStore;

struct ThetaServerState {
    service: ThetaSignalService,
    db_path: std::path::PathBuf,
}

// Helper functions to safely extract arguments
fn get_string_arg(args: Option<&Value>, key: &str) -> Result<Option<String>, Error> {
    Ok(args
        .and_then(|a| a.get(key))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

fn get_f64_arg(args: Option<&Value>, key: &str) -> Result<Option<f64>, Error> {
    Ok(args
        .and_then(|a| a.get(key))
        .and_then(|v| v.as_f64()))
}

#[async_trait]
impl ServerHandler for ThetaServerState {
    async fn initialize(
        &self,
        _implementation: Implementation,
        _capabilities: ClientCapabilities,
    ) -> Result<ServerCapabilities, Error> {
        Ok(ServerCapabilities {
            tools: Some(json!({
                "listChanged": false
            })),
            ..Default::default()
        })
    }

    async fn shutdown(&self) -> Result<(), Error> {
        Ok(())
    }

    async fn handle_method(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, Error> {
        let service = &self.service; // Clone Arc for use in async blocks if needed, or pass &self
        match method {
            "tools/list" => {
                let get_market_tone_tool = Tool {
                    name: "get_market_tone".to_string(),
                    description: "Get the current market tone summary and options structure for a given symbol".to_string(),
                    input_schema: Some(ToolSchema {
                        properties: Some(json!({
                            "symbol": {
                                "type": "string",
                                "description": "Stock symbol (e.g. TSLA.US)"
                            },
                            "expiry": {
                                "type": "string",
                                "description": "Optional explicit expiry date (e.g. 2026-06-18)"
                            },
                            "expiries_limit": {
                                "type": "number",
                                "description": "Number of expiries for term structure (default 4)",
                                "default": 4
                            }
                        })),
                        required: Some(vec!["symbol".to_string()]),
                    }),
                    annotations: None,
                };

                let get_signal_history_tool = Tool {
                    name: "get_signal_history".to_string(),
                    description: "Get historically captured market tone snapshots from the local database".to_string(),
                    input_schema: Some(ToolSchema {
                        properties: Some(json!({
                            "symbol": {
                                "type": "string",
                                "description": "Optional stock symbol to filter by (e.g. TSLA.US)"
                            },
                            "limit": {
                                "type": "number",
                                "description": "Maximum number of records to return (default 20)",
                                "default": 20
                            }
                        })),
                        required: None,
                    }),
                    annotations: None,
                };

                let get_skew_tool = Tool {
                    name: "get_skew".to_string(),
                    description: "Get volatility skew analysis for a given symbol".to_string(),
                    input_schema: Some(ToolSchema {
                        properties: Some(json!({
                            "symbol": { "type": "string", "description": "Stock symbol (e.g. TSLA.US)" },
                            "expiry": { "type": "string", "description": "Optional explicit expiry date (e.g. 2026-06-18)" },
                            "target_delta": { "type": "number", "description": "Target absolute delta", "default": 0.25 },
                            "target_otm_percent": { "type": "number", "description": "Target OTM percent", "default": 0.05 }
                        })),
                        required: Some(vec!["symbol".to_string()]),
                    }),
                    annotations: None,
                };

                let get_smile_tool = Tool {
                    name: "get_smile".to_string(),
                    description: "Get volatility smile analysis for a given symbol".to_string(),
                    input_schema: Some(ToolSchema {
                        properties: Some(json!({
                            "symbol": { "type": "string", "description": "Stock symbol (e.g. TSLA.US)" },
                            "expiry": { "type": "string", "description": "Optional explicit expiry date" }
                        })),
                        required: Some(vec!["symbol".to_string()]),
                    }),
                    annotations: None,
                };

                let get_term_structure_tool = Tool {
                    name: "get_term_structure".to_string(),
                    description: "Get ATM volatility term structure across expiries".to_string(),
                    input_schema: Some(ToolSchema {
                        properties: Some(json!({
                            "symbol": { "type": "string", "description": "Stock symbol (e.g. TSLA.US)" },
                            "expiries_limit": { "type": "number", "description": "Number of listed expiries to fetch", "default": 4 }
                        })),
                        required: Some(vec!["symbol".to_string()]),
                    }),
                    annotations: None,
                };

                let get_put_call_bias_tool = Tool {
                    name: "get_put_call_bias".to_string(),
                    description: "Get volume and open interest bias between puts and calls".to_string(),
                    input_schema: Some(ToolSchema {
                        properties: Some(json!({
                            "symbol": { "type": "string", "description": "Stock symbol (e.g. TSLA.US)" },
                            "expiry": { "type": "string", "description": "Optional explicit expiry date" },
                            "bias_min_otm_percent": { "type": "number", "description": "Minimum OTM percent for inclusion", "default": 0.05 }
                        })),
                        required: Some(vec!["symbol".to_string()]),
                    }),
                    annotations: None,
                };

                let get_market_extreme_tool = Tool {
                    name: "get_market_extreme".to_string(),
                    description: "Screen for symbols hitting generalized market extremes today".to_string(),
                    input_schema: Some(ToolSchema {
                        properties: Some(json!({
                            "limit": { "type": "number", "description": "Max results to return", "default": 20 }
                        })),
                        required: None,
                    }),
                    annotations: None,
                };

                let get_relative_extreme_tool = Tool {
                    name: "get_relative_extreme".to_string(),
                    description: "Find symbols moving abnormally relative to their own history".to_string(),
                    input_schema: Some(ToolSchema {
                        properties: Some(json!({
                            "limit": { "type": "number", "description": "Max results to return", "default": 20 }
                        })),
                        required: None,
                    }),
                    annotations: None,
                };

                let get_portfolio_tool = Tool {
                    name: "get_portfolio".to_string(),
                    description: "Get real-time portfolio holdings, P&L, and aggregated Greeks risk".to_string(),
                    input_schema: Some(ToolSchema {
                        properties: Some(json!({
                            "margin_ratio": { "type": "number", "description": "Initial margin ratio assumption", "default": 0.3 }
                        })),
                        required: None,
                    }),
                    annotations: None,
                };

                Ok(json!({
                    "tools": [
                        get_market_tone_tool,
                        get_signal_history_tool,
                        get_skew_tool,
                        get_smile_tool,
                        get_term_structure_tool,
                        get_put_call_bias_tool,
                        get_market_extreme_tool,
                        get_relative_extreme_tool,
                        get_portfolio_tool
                    ]
                }))
            }
            "tools/call" => {
                let params = params.unwrap_or_default();
                let tool_name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params.get("arguments");

                match tool_name {
                    "get_market_tone" => {
                        let symbol = args
                            .and_then(|a| a.get("symbol"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("TSLA.US")
                            .to_string();

                        let expiries_limit = args
                            .and_then(|a| a.get("expiries_limit"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(4) as usize;

                        let explicit_expiry = args
                            .and_then(|a| a.get("expiry"))
                            .and_then(|v| v.as_str());

                        let expiry = if let Some(dt) = explicit_expiry {
                            match theta::market_data::parse_expiry_date(dt) {
                                Ok(exp) => exp,
                                Err(e) => return Ok(json!({
                                    "content": [{"type": "text", "text": format!("Error parsing expiry: {}", e)}],
                                    "isError": true
                                })),
                            }
                        } else {
                            match self.service.front_expiry_for_symbol(&symbol).await {
                                Ok(exp) => exp,
                                Err(e) => return Ok(json!({
                                    "content": [{"type": "text", "text": format!("Error fetching front expiry: {}", e)}],
                                    "isError": true
                                })),
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

                        match self.service.market_tone(tone_req).await {
                            Ok(view) => {
                                let json_val = serde_json::to_string_pretty(&view).unwrap_or_default();
                                Ok(json!({
                                    "content": [{"type": "text", "text": json_val}],
                                    "isError": false
                                }))
                            }
                            Err(e) => Ok(json!({
                                "content": [{"type": "text", "text": format!("Error: {}", e)}],
                                "isError": true
                            })),
                        }
                    }
                    "get_signal_history" => {
                        let symbol = args
                            .and_then(|a| a.get("symbol"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        let limit = args
                            .and_then(|a| a.get("limit"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(20) as usize;

                        let store = match SignalSnapshotStore::open(&self.db_path) {
                            Ok(s) => s,
                            Err(e) => return Ok(json!({
                                "content": [{"type": "text", "text": format!("Failed to open db: {}", e)}],
                                "isError": true
                            })),
                        };

                        match store.list_market_tone_snapshots(symbol.as_deref(), limit) {
                            Ok(rows) => {
                                let json_val = serde_json::to_string_pretty(&rows).unwrap_or_default();
                                Ok(json!({
                                    "content": [{"type": "text", "text": json_val}],
                                    "isError": false
                                }))
                            }
                            Err(e) => Ok(json!({
                                "content": [{"type": "text", "text": format!("Error: {}", e)}],
                                "isError": true
                            })),
                        }
                    }
                    "get_skew" => {
                        let symbol = args.and_then(|a| a.get("symbol")).and_then(|v| v.as_str()).unwrap_or("TSLA.US").to_string();
                        let explicit_expiry = args.and_then(|a| a.get("expiry")).and_then(|v| v.as_str());
                        let target_delta = args.and_then(|a| a.get("target_delta")).and_then(|v| v.as_f64()).unwrap_or(0.25);
                        let target_otm_percent = args.and_then(|a| a.get("target_otm_percent")).and_then(|v| v.as_f64()).unwrap_or(0.05);

                        let expiry = if let Some(dt) = explicit_expiry {
                            match theta::market_data::parse_expiry_date(dt) {
                                Ok(exp) => exp,
                                Err(e) => return Ok(json!({ "content": [{"type": "text", "text": format!("Error parsing expiry: {}", e)}], "isError": true })),
                            }
                        } else {
                            match self.service.front_expiry_for_symbol(&symbol).await {
                                Ok(exp) => exp,
                                Err(e) => return Ok(json!({ "content": [{"type": "text", "text": format!("Error fetching front expiry: {}", e)}], "isError": true })),
                            }
                        };

                        let req = theta::signal_service::SkewSignalRequest { symbol, expiry, rate: None, dividend: 0.0, iv: None, iv_from_market_price: true, target_delta, target_otm_percent };
                        match self.service.skew(req).await {
                            Ok(view) => Ok(json!({ "content": [{"type": "text", "text": serde_json::to_string_pretty(&view).unwrap_or_default()}], "isError": false })),
                            Err(e) => Ok(json!({ "content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true })),
                        }
                    }
                    "get_smile" => {
                        let symbol = args.and_then(|a| a.get("symbol")).and_then(|v| v.as_str()).unwrap_or("TSLA.US").to_string();
                        let explicit_expiry = args.and_then(|a| a.get("expiry")).and_then(|v| v.as_str());

                        let expiry = if let Some(dt) = explicit_expiry {
                            match theta::market_data::parse_expiry_date(dt) {
                                Ok(exp) => exp,
                                Err(e) => return Ok(json!({ "content": [{"type": "text", "text": format!("Error parsing expiry: {}", e)}], "isError": true })),
                            }
                        } else {
                            match self.service.front_expiry_for_symbol(&symbol).await {
                                Ok(exp) => exp,
                                Err(e) => return Ok(json!({ "content": [{"type": "text", "text": format!("Error fetching front expiry: {}", e)}], "isError": true })),
                            }
                        };

                        let req = theta::signal_service::SmileSignalRequest { symbol, expiry, rate: None, dividend: 0.0, iv: None, iv_from_market_price: true, target_otm_percents: vec![0.05, 0.10, 0.15] };
                        match self.service.smile(req).await {
                            Ok(view) => Ok(json!({ "content": [{"type": "text", "text": serde_json::to_string_pretty(&view).unwrap_or_default()}], "isError": false })),
                            Err(e) => Ok(json!({ "content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true })),
                        }
                    }
                    "get_term_structure" => {
                        let symbol = args.and_then(|a| a.get("symbol")).and_then(|v| v.as_str()).unwrap_or("TSLA.US").to_string();
                        let expiries_limit = args.and_then(|a| a.get("expiries_limit")).and_then(|v| v.as_u64()).unwrap_or(4) as usize;

                        let req = theta::signal_service::TermStructureRequest { symbol, expiries_limit, rate: None, dividend: 0.0, iv: None, iv_from_market_price: true };
                        match self.service.term_structure(req).await {
                            Ok(view) => Ok(json!({ "content": [{"type": "text", "text": serde_json::to_string_pretty(&view).unwrap_or_default()}], "isError": false })),
                            Err(e) => Ok(json!({ "content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true })),
                        }
                    }
                    "get_put_call_bias" => {
                        let symbol = args.and_then(|a| a.get("symbol")).and_then(|v| v.as_str()).unwrap_or("TSLA.US").to_string();
                        let explicit_expiry = args.and_then(|a| a.get("expiry")).and_then(|v| v.as_str());
                        let min_otm_percent = args.and_then(|a| a.get("bias_min_otm_percent")).and_then(|v| v.as_f64()).unwrap_or(0.05);

                        let expiry = if let Some(dt) = explicit_expiry {
                            match theta::market_data::parse_expiry_date(dt) {
                                Ok(exp) => exp,
                                Err(e) => return Ok(json!({ "content": [{"type": "text", "text": format!("Error parsing expiry: {}", e)}], "isError": true })),
                            }
                        } else {
                            match self.service.front_expiry_for_symbol(&symbol).await {
                                Ok(exp) => exp,
                                Err(e) => return Ok(json!({ "content": [{"type": "text", "text": format!("Error fetching front expiry: {}", e)}], "isError": true })),
                            }
                        };

                        let req = theta::signal_service::PutCallBiasRequest { symbol, expiry, rate: None, dividend: 0.0, iv: None, iv_from_market_price: true, min_otm_percent };
                        match self.service.put_call_bias(req).await {
                            Ok(view) => Ok(json!({ "content": [{"type": "text", "text": serde_json::to_string_pretty(&view).unwrap_or_default()}], "isError": false })),
                            Err(e) => Ok(json!({ "content": [{"type": "text", "text": format!("Error: {}", e)}], "isError": true })),
                        }
                    }
                    _ => Ok(json!({
                        "error": format!("Unknown tool: {}", tool_name)
                    })),
                }
            }
            _ => Err(Error::protocol(
                mcp_sdk_rs::error::ErrorCode::MethodNotFound,
                "Method not implemented",
            )),
        }
    }
}

fn default_db_path() -> Result<std::path::PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    Ok(std::path::PathBuf::from(home).join(".theta").join("signals.db"))
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting theta MCP server via mcp-sdk-rs");

    let service = ThetaSignalService::from_env().await?;
    let db_path = default_db_path()?;
    let state = Arc::new(ThetaServerState { service, db_path });

    let (tx, rx) = tokio::sync::mpsc::channel(100);
    let (tx_out, mut rx_out) = tokio::sync::mpsc::channel::<String>(100);

    let transport = Arc::new(StdioTransport::new(rx, tx_out));

    let server = Server::new(transport.clone(), state);

    tokio::spawn(async move {
        while let Some(msg) = rx_out.recv().await {
            println!("{}", msg);
        }
    });

    tokio::spawn(async move {
        use tokio::io::AsyncBufReadExt;
        let stdin = tokio::io::stdin();
        let mut reader = tokio::io::BufReader::new(stdin);
        let mut buffer = String::new();
        while let Ok(n) = reader.read_line(&mut buffer).await {
            if n == 0 {
                break;
            }
            if !buffer.trim().is_empty() {
                let _ = tx.send(buffer.clone()).await;
            }
            buffer.clear();
        }
    });

    if let Err(e) = server.start().await {
        tracing::error!("Server error: {}", e);
    }

    Ok(())
}
