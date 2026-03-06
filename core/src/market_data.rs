use crate::analytics::ContractSide;
use crate::domain::{OptionChainSnapshot, OptionContractSnapshot, ProviderGreeks, UnderlyingSnapshot};
use crate::daemon_protocol::{DaemonRequest, DaemonResponse};
use anyhow::{bail, Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use time::{Date, OffsetDateTime};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::json;

// --- Local Mirror Types for LongPort SDK JSON structure ---
// These allow core to be independent of the longport crate.

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Market {
    US,
    HK,
    CN,
    SG,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum OptionDirection {
    #[serde(rename = "CALL")]
    Call,
    #[serde(rename = "PUT")]
    Put,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityQuote {
    pub symbol: String,
    pub last_done: String,
    pub prev_close: String,
    pub open: String,
    pub high: String,
    pub low: String,
    pub volume: i64,
    pub turnover: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OptionQuote {
    pub symbol: String,
    pub underlying_symbol: String,
    pub direction: OptionDirection,
    pub last_done: String,
    pub prev_close: String,
    pub open: String,
    pub high: String,
    pub low: String,
    pub volume: i64,
    pub turnover: String,
    pub timestamp: String,
    pub trade_status: serde_json::Value,
    pub strike_price: String,
    pub expiry_date: Date,
    pub implied_volatility: String,
    pub open_interest: i64,
    pub historical_volatility: String,
    pub contract_multiplier: String,
    pub contract_size: String,
    pub contract_type: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityCalcIndex {
    pub symbol: String,
    pub last_done: Option<String>,
    pub implied_volatility: Option<String>,
    pub delta: Option<String>,
    pub gamma: Option<String>,
    pub theta: Option<String>,
    pub vega: Option<String>,
    pub rho: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MarketTradingDays {
    pub market: Market,
    pub trading_days: Vec<Date>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrikePriceInfo {
    pub strike_price: String,
    pub call_symbol: String,
    pub put_symbol: String,
    pub standard: bool,
}

const SOCKET_PATH: &str = "/tmp/theta.sock";

/// A pure UDS Proxy Client for the LongPort SDK.
/// This is the only implementation allowed in the application layer.
pub struct MarketDataClient;

impl MarketDataClient {
    pub async fn connect() -> Result<Self> {
        if !std::path::Path::new(SOCKET_PATH).exists() {
            bail!("Theta daemon is not running (socket not found at {})", SOCKET_PATH);
        }
        // Verify connectivity
        let _ = UnixStream::connect(SOCKET_PATH).await
            .context("Failed to connect to Theta daemon")?;
        Ok(Self)
    }

    /// Generic UDS RPC call helper.
    /// Acts as a transparent gateway for any SDK method.
    async fn rpc_call<P: Serialize, R: DeserializeOwned>(&self, method: &str, params: P) -> Result<R> {
        let mut stream = UnixStream::connect(SOCKET_PATH).await
            .with_context(|| format!("Proxy call failed for method {}", method))?;
        
        let req = DaemonRequest {
            method: method.to_string(),
            params: serde_json::to_value(params)?,
        };
        
        let mut req_json = serde_json::to_string(&req)?;
        req_json.push('\n');
        stream.write_all(req_json.as_bytes()).await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).await?;

        if line.is_empty() {
            bail!("Daemon closed connection unexpectedly during {}", method);
        }

        let resp: DaemonResponse = serde_json::from_str(&line)
            .with_context(|| format!("Failed to parse daemon response for {}", method))?;

        if let Some(err) = resp.error {
            bail!("SDK Proxy Error [{}]: {}", method, err);
        }

        let result = resp.result.context("Daemon returned empty result")?;
        serde_json::from_value(result).context("Type mismatch in SDK proxy result")
    }

    // --- High-level Domain Logic ---
    // These methods implement the "Theta" domain specific logic by calling raw SDK methods via proxy.

    pub async fn fetch_underlying(&self, symbol: &str) -> Result<UnderlyingSnapshot> {
        let mut quotes: Vec<SecurityQuote> = self.rpc_call("quote", vec![symbol.to_string()]).await?;
        let quote = quotes.pop().context("no quote found for symbol")?;
        snapshot_from_quote(quote)
    }

    pub async fn fetch_option_contract(&self, symbol: &str) -> Result<OptionContractSnapshot> {
        let mut quotes: Vec<OptionQuote> = self.rpc_call("option_quote", vec![symbol.to_string()]).await?;
        let quote = quotes.pop().context("no option quote found")?;
        option_snapshot_from_quote(quote)
    }

    pub async fn fetch_option_expiries(&self, symbol: &str) -> Result<Vec<String>> {
        let expiries: Vec<Date> = self.rpc_call("option_chain_expiry_date_list", symbol.to_string()).await?;
        Ok(expiries.into_iter().map(|date| date.to_string()).collect())
    }

    pub async fn batch_quote(&self, symbols: &[String]) -> Result<std::collections::HashMap<String, f64>> {
        if symbols.is_empty() { return Ok(Default::default()); }
        let quotes: Vec<SecurityQuote> = self.rpc_call("quote", symbols.to_vec()).await?;
        let mut map = std::collections::HashMap::new();
        for q in quotes {
            if let Ok(price) = decimal_to_f64(&q.last_done, "last_done") {
                let sym = q.symbol.trim_end_matches(".US").to_string();
                map.insert(sym, price);
            }
        }
        Ok(map)
    }

    pub async fn batch_option_quote(&self, symbols: &[String]) -> Result<Vec<OptionQuote>> {
        if symbols.is_empty() { return Ok(vec![]); }
        self.rpc_call("option_quote", symbols.to_vec()).await
    }

    pub async fn fetch_provider_greeks(&self, symbol: &str) -> Result<ProviderGreeks> {
        let mut rows: Vec<SecurityCalcIndex> = self.rpc_call("calc_indexes", json!({
            "symbols": vec![symbol.to_string()],
            "indexes": vec!["ImpliedVolatility", "Delta", "Gamma", "Theta", "Vega", "Rho"]
        })).await?;
        let row = rows.pop().context("no calc data found")?;
        Ok(provider_greeks_view(row))
    }

    pub async fn fetch_trading_days(&self, market: Market, start: Date, end: Date) -> Result<Vec<Date>> {
        let days_resp: MarketTradingDays = self.rpc_call("trading_days", json!({
            "market": market,
            "start": start.to_string(),
            "end": end.to_string()
        })).await?;
        Ok(days_resp.trading_days)
    }

    pub async fn fetch_option_chain(&self, symbol: &str, expiry: Date) -> Result<OptionChainSnapshot> {
        let underlying = self.fetch_underlying(symbol).await?;
        let days_to_expiry = days_to_expiry(expiry);

        let info: Vec<StrikePriceInfo> = self.rpc_call("option_chain_info_by_date", json!({
            "symbol": symbol.to_string(),
            "expiry": expiry.to_string()
        })).await?;

        let option_symbols: Vec<String> = info.iter().flat_map(|s| [s.call_symbol.clone(), s.put_symbol.clone()]).collect();
        let option_quotes = self.batch_option_quote(&option_symbols).await?;
        
        let mut contracts = Vec::with_capacity(option_quotes.len());
        for quote in option_quotes {
            contracts.push(option_snapshot_from_quote(quote)?);
        }
        Ok(OptionChainSnapshot { underlying, expiry, days_to_expiry, contracts })
    }
}

// --- Domain Mappers (Public so Daemon can use them if needed, though daemon returns raw types) ---

pub fn snapshot_from_quote(quote: SecurityQuote) -> Result<UnderlyingSnapshot> {
    Ok(UnderlyingSnapshot {
        symbol: quote.symbol,
        last_done: quote.last_done.clone(),
        last_done_f64: decimal_to_f64(&quote.last_done, "last_done")?,
        prev_close: quote.prev_close.clone(),
        prev_close_f64: decimal_to_f64(&quote.prev_close, "prev_close")?,
        open: quote.open.clone(),
        open_f64: decimal_to_f64(&quote.open, "open")?,
        high: quote.high.clone(),
        high_f64: decimal_to_f64(&quote.high, "high")?,
        low: quote.low.clone(),
        low_f64: decimal_to_f64(&quote.low, "low")?,
        volume: quote.volume,
        turnover: quote.turnover.clone(),
        turnover_f64: decimal_to_f64(&quote.turnover, "turnover")?,
        timestamp: quote.timestamp.clone(),
    })
}

pub fn option_snapshot_from_quote(quote: OptionQuote) -> Result<OptionContractSnapshot> {
    Ok(OptionContractSnapshot {
        symbol: quote.symbol,
        underlying_symbol: quote.underlying_symbol,
        option_type: match quote.direction {
            OptionDirection::Call => ContractSide::Call,
            OptionDirection::Put => ContractSide::Put,
            _ => bail!("unknown direction"),
        },
        last_done: quote.last_done.clone(),
        last_done_f64: decimal_to_f64(&quote.last_done, "last_done")?,
        prev_close: quote.prev_close.clone(),
        prev_close_f64: decimal_to_f64(&quote.prev_close, "prev_close")?,
        open: quote.open.clone(),
        open_f64: decimal_to_f64(&quote.open, "open")?,
        high: quote.high.clone(),
        high_f64: decimal_to_f64(&quote.high, "high")?,
        low: quote.low.clone(),
        low_f64: decimal_to_f64(&quote.low, "low")?,
        timestamp: quote.timestamp.clone(),
        volume: quote.volume,
        turnover: quote.turnover.clone(),
        turnover_f64: decimal_to_f64(&quote.turnover, "turnover")?,
        trade_status: format!("{:?}", quote.trade_status),
        strike_price: quote.strike_price.clone(),
        strike_price_f64: decimal_to_f64(&quote.strike_price, "strike")?,
        expiry: quote.expiry_date,
        provider_reported_iv: quote.implied_volatility.clone(),
        provider_reported_iv_f64: decimal_to_f64(&quote.implied_volatility, "iv")?,
        open_interest: quote.open_interest,
        historical_volatility: quote.historical_volatility.clone(),
        historical_volatility_f64: decimal_to_f64(&quote.historical_volatility, "hv")?,
        contract_multiplier: quote.contract_multiplier.clone(),
        contract_multiplier_f64: decimal_to_f64(&quote.contract_multiplier, "mult")?,
        contract_size: quote.contract_size.clone(),
        contract_size_f64: decimal_to_f64(&quote.contract_size, "size")?,
        contract_style: format!("{:?}", quote.contract_type),
    })
}

pub fn provider_greeks_view(row: SecurityCalcIndex) -> ProviderGreeks {
    ProviderGreeks {
        delta: row.delta,
        gamma: row.gamma,
        theta: row.theta,
        vega: row.vega,
        rho: row.rho,
        implied_volatility: row.implied_volatility,
    }
}

pub fn decimal_to_f64(value: &str, field: &str) -> Result<f64> {
    value.parse().with_context(|| format!("parse {}", field))
}

pub fn parse_expiry_date(input: &str) -> Result<Date> {
    let format = time::format_description::parse("[year]-[month]-[day]")?;
    Date::parse(input, &format).context("parse expiry")
}

pub fn days_to_expiry(expiry: Date) -> i64 {
    let today = OffsetDateTime::now_utc().date();
    let days = (expiry - today).whole_days();
    if days < 1 { 1 } else { days }
}
