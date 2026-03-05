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
use serde_json::json;
use std::sync::Arc;
use theta::signal_service::{MarketToneRequest, ThetaSignalService};
use theta::snapshot_store::SignalSnapshotStore;

struct ThetaServerState {
    service: ThetaSignalService,
    db_path: std::path::PathBuf,
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

                Ok(json!({
                    "tools": [get_market_tone_tool, get_signal_history_tool]
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

                        let expiry = match self.service.front_expiry_for_symbol(&symbol).await {
                            Ok(exp) => exp,
                            Err(e) => return Ok(json!({
                                "content": [{"type": "text", "text": format!("Error: {}", e)}],
                                "isError": true
                            })),
                        };

                        let tone_req = MarketToneRequest {
                            symbol: symbol.clone(),
                            expiry,
                            expiries_limit,
                            rate: None,
                            dividend: 0.0,
                            iv: None,
                            iv_from_market_price: false,
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
