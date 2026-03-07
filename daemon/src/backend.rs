use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use time::Date;

#[async_trait]
pub trait QuoteBackend: Send + Sync {
    async fn quote(&self, symbols: Vec<String>) -> Result<Vec<Value>>;
    async fn option_quote(&self, symbols: Vec<String>) -> Result<Vec<Value>>;
    async fn option_chain_expiry_date_list(&self, symbol: String) -> Result<Vec<String>>;
    async fn option_chain_info_by_date(&self, symbol: String, expiry: Date) -> Result<Vec<Value>>;
}
