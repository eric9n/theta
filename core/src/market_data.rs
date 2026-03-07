use crate::analytics::ContractSide;
use crate::daemon_protocol::{
    DaemonRequest, DaemonResponse, MAX_DAEMON_RESPONSE_BYTES, encode_json_line, read_bounded_frame,
};
use crate::domain::{
    OptionChainSnapshot, OptionContractSnapshot, ProviderGreeks, UnderlyingSnapshot,
};
use crate::runtime::theta_socket_path;
use anyhow::{Context, Result, anyhow, bail};
use serde::Deserializer;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::json;
use std::path::{Path, PathBuf};
use time::{Date, OffsetDateTime};
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::{Duration, timeout};

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
    #[serde(alias = "Call")]
    Call,
    #[serde(rename = "PUT")]
    #[serde(alias = "Put")]
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
    #[serde(deserialize_with = "deserialize_date")]
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
    #[serde(deserialize_with = "deserialize_date_vec")]
    pub trading_days: Vec<Date>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StrikePriceInfo {
    #[serde(alias = "price")]
    pub strike_price: String,
    pub call_symbol: String,
    pub put_symbol: String,
    pub standard: bool,
}

const CLIENT_CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const CLIENT_RPC_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_OPTION_QUOTE_SYMBOLS_PER_REQUEST: usize = 500;

#[derive(Debug, Clone, Copy, Default)]
pub struct OptionChainFetchFilter {
    pub side: Option<ContractSide>,
    pub min_strike: Option<f64>,
    pub max_strike: Option<f64>,
    pub min_otm_percent: Option<f64>,
    pub max_otm_percent: Option<f64>,
}

/// A pure UDS Proxy Client for the LongPort SDK.
/// This is the only implementation allowed in the application layer.
pub struct MarketDataClient {
    socket_path: PathBuf,
}

impl MarketDataClient {
    pub async fn connect() -> Result<Self> {
        let socket_path = theta_socket_path()?;
        verify_socket_path_exists(&socket_path)?;

        let _ = timeout(CLIENT_CONNECT_TIMEOUT, UnixStream::connect(&socket_path))
            .await
            .with_context(|| {
                format!(
                    "Timed out connecting to theta daemon at {}",
                    socket_path.display()
                )
            })?
            .with_context(|| {
                format!(
                    "Failed to connect to theta daemon at {}",
                    socket_path.display()
                )
            })?;

        Ok(Self { socket_path })
    }

    #[cfg(test)]
    pub(crate) fn for_tests(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    /// Generic UDS RPC call helper.
    /// Acts as a transparent gateway for any SDK method.
    async fn rpc_call<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R> {
        verify_socket_path_exists(&self.socket_path)?;

        timeout(CLIENT_RPC_TIMEOUT, self.rpc_call_inner(method, params))
            .await
            .with_context(|| format!("Theta daemon RPC timed out for {}", method))?
    }

    async fn rpc_call_inner<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R> {
        let mut stream = timeout(
            CLIENT_CONNECT_TIMEOUT,
            UnixStream::connect(&self.socket_path),
        )
        .await
        .with_context(|| {
            format!(
                "Timed out connecting to theta daemon for {} at {}",
                method,
                self.socket_path.display()
            )
        })?
        .with_context(|| {
            format!(
                "Failed to connect to theta daemon for {} at {}",
                method,
                self.socket_path.display()
            )
        })?;

        let req = DaemonRequest {
            method: method.to_string(),
            params: serde_json::to_value(params)?,
        };

        let req_bytes = encode_json_line(&req)?;
        stream.write_all(&req_bytes).await.with_context(|| {
            format!(
                "Failed to send request to theta daemon for {} at {}",
                method,
                self.socket_path.display()
            )
        })?;

        let mut reader = BufReader::new(stream);
        let frame = read_bounded_frame(
            &mut reader,
            MAX_DAEMON_RESPONSE_BYTES,
            "daemon response frame",
        )
        .await
        .map_err(|err| anyhow!("Failed to read daemon response for {}: {err}", method))?
        .context(format!(
            "Theta daemon closed connection unexpectedly during {}",
            method
        ))?;

        let resp: DaemonResponse = serde_json::from_slice(&frame)
            .with_context(|| format!("Failed to parse daemon response for {}", method))?;

        if let Some(err) = resp.error {
            return Err(anyhow!(err));
        }

        let result = resp.result.context("Daemon returned empty result")?;
        serde_json::from_value(result).context("Type mismatch in SDK proxy result")
    }

    // --- High-level Domain Logic ---
    // These methods implement the "Theta" domain specific logic by calling raw SDK methods via proxy.

    pub async fn fetch_underlying(&self, symbol: &str) -> Result<UnderlyingSnapshot> {
        let mut quotes: Vec<SecurityQuote> =
            self.rpc_call("quote", vec![symbol.to_string()]).await?;
        let quote = quotes.pop().context("no quote found for symbol")?;
        snapshot_from_quote(quote)
    }

    pub async fn fetch_option_contract(&self, symbol: &str) -> Result<OptionContractSnapshot> {
        let mut quotes: Vec<OptionQuote> = self
            .rpc_call("option_quote", vec![symbol.to_string()])
            .await?;
        let quote = quotes.pop().context("no option quote found")?;
        option_snapshot_from_quote(quote)
    }

    pub async fn fetch_option_expiries(&self, symbol: &str) -> Result<Vec<String>> {
        self.rpc_call("option_chain_expiry_date_list", symbol.to_string())
            .await
    }

    pub async fn batch_quote(
        &self,
        symbols: &[String],
    ) -> Result<std::collections::HashMap<String, f64>> {
        if symbols.is_empty() {
            return Ok(Default::default());
        }
        let quotes: Vec<SecurityQuote> = self.rpc_call("quote", symbols.to_vec()).await?;
        let mut map = std::collections::HashMap::new();
        for q in quotes {
            let price = decimal_to_f64(&q.last_done, "last_done")
                .with_context(|| format!("invalid batch quote price for {}", q.symbol))?;
            let sym = q.symbol.trim_end_matches(".US").to_string();
            map.insert(sym, price);
        }
        Ok(map)
    }

    pub async fn batch_option_quote(&self, symbols: &[String]) -> Result<Vec<OptionQuote>> {
        if symbols.is_empty() {
            return Ok(vec![]);
        }
        let symbols = dedup_symbols(symbols);
        let mut quotes = Vec::new();

        for chunk in symbols.chunks(MAX_OPTION_QUOTE_SYMBOLS_PER_REQUEST) {
            let mut chunk_quotes: Vec<OptionQuote> =
                self.rpc_call("option_quote", chunk.to_vec()).await?;
            quotes.append(&mut chunk_quotes);
        }

        Ok(quotes)
    }

    pub async fn fetch_provider_greeks(&self, symbol: &str) -> Result<ProviderGreeks> {
        let mut rows: Vec<SecurityCalcIndex> = self
            .rpc_call(
                "calc_indexes",
                json!({
                    "symbols": vec![symbol.to_string()],
                    "indexes": vec!["ImpliedVolatility", "Delta", "Gamma", "Theta", "Vega", "Rho"]
                }),
            )
            .await?;
        let row = rows.pop().context("no calc data found")?;
        Ok(provider_greeks_view(row))
    }

    pub async fn fetch_trading_days(
        &self,
        market: Market,
        start: Date,
        end: Date,
    ) -> Result<Vec<Date>> {
        let days_resp: MarketTradingDays = self
            .rpc_call(
                "trading_days",
                json!({
                    "market": market,
                    "start": start.to_string(),
                    "end": end.to_string()
                }),
            )
            .await?;
        Ok(days_resp.trading_days)
    }

    pub async fn fetch_option_chain(
        &self,
        symbol: &str,
        expiry: Date,
    ) -> Result<OptionChainSnapshot> {
        self.fetch_option_chain_filtered(symbol, expiry, OptionChainFetchFilter::default())
            .await
    }

    pub async fn fetch_option_chain_filtered(
        &self,
        symbol: &str,
        expiry: Date,
        filter: OptionChainFetchFilter,
    ) -> Result<OptionChainSnapshot> {
        let underlying = self.fetch_underlying(symbol).await?;
        let days_to_expiry = days_to_expiry(expiry);

        let info: Vec<StrikePriceInfo> = self
            .rpc_call(
                "option_chain_info_by_date",
                json!({
                    "symbol": symbol.to_string(),
                    "expiry": expiry.to_string()
                }),
            )
            .await?;

        let option_symbols = select_option_symbols(&info, underlying.last_done_f64, filter);
        let option_quotes = self.batch_option_quote(&option_symbols).await?;

        let mut contracts = Vec::with_capacity(option_quotes.len());
        for quote in option_quotes {
            contracts.push(option_snapshot_from_quote(quote)?);
        }
        Ok(OptionChainSnapshot {
            underlying,
            expiry,
            days_to_expiry,
            contracts,
        })
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
        trade_status: json_value_as_string(&quote.trade_status),
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
        contract_style: json_value_as_string(&quote.contract_type),
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
    let parsed = value
        .parse::<f64>()
        .with_context(|| format!("parse {}", field))?;
    if !parsed.is_finite() {
        bail!("{field} must be finite");
    }
    Ok(parsed)
}

pub fn parse_expiry_date(input: &str) -> Result<Date> {
    let format = time::format_description::parse("[year]-[month]-[day]")?;
    Date::parse(input, &format).context("parse expiry")
}

pub fn days_to_expiry(expiry: Date) -> i64 {
    days_to_expiry_from(expiry, OffsetDateTime::now_utc().date())
}

fn days_to_expiry_from(expiry: Date, today: Date) -> i64 {
    let days = (expiry - today).whole_days();
    if days < 0 {
        0
    } else if days == 0 {
        1
    } else {
        days
    }
}

fn dedup_symbols(symbols: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut deduped = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        if seen.insert(symbol.as_str()) {
            deduped.push(symbol.clone());
        }
    }
    deduped
}

fn select_option_symbols(
    info: &[StrikePriceInfo],
    underlying_spot: f64,
    filter: OptionChainFetchFilter,
) -> Vec<String> {
    let mut selected = Vec::new();

    for row in info {
        let Ok(strike) = decimal_to_f64(&row.strike_price, "strike_price") else {
            continue;
        };

        if !matches_strike_filter(strike, filter.min_strike, filter.max_strike) {
            continue;
        }

        match filter.side {
            Some(ContractSide::Call) => {
                if matches_otm_filter(
                    ContractSide::Call,
                    strike,
                    underlying_spot,
                    filter.min_otm_percent,
                    filter.max_otm_percent,
                ) {
                    selected.push(row.call_symbol.clone());
                }
            }
            Some(ContractSide::Put) => {
                if matches_otm_filter(
                    ContractSide::Put,
                    strike,
                    underlying_spot,
                    filter.min_otm_percent,
                    filter.max_otm_percent,
                ) {
                    selected.push(row.put_symbol.clone());
                }
            }
            None => {
                if matches_otm_filter(
                    ContractSide::Call,
                    strike,
                    underlying_spot,
                    filter.min_otm_percent,
                    filter.max_otm_percent,
                ) {
                    selected.push(row.call_symbol.clone());
                }
                if matches_otm_filter(
                    ContractSide::Put,
                    strike,
                    underlying_spot,
                    filter.min_otm_percent,
                    filter.max_otm_percent,
                ) {
                    selected.push(row.put_symbol.clone());
                }
            }
        }
    }

    dedup_symbols(&selected)
}

fn matches_strike_filter(strike: f64, min_strike: Option<f64>, max_strike: Option<f64>) -> bool {
    if let Some(min) = min_strike
        && strike < min
    {
        return false;
    }
    if let Some(max) = max_strike
        && strike > max
    {
        return false;
    }
    true
}

fn matches_otm_filter(
    side: ContractSide,
    strike: f64,
    underlying_spot: f64,
    min_otm_percent: Option<f64>,
    max_otm_percent: Option<f64>,
) -> bool {
    if min_otm_percent.is_none() && max_otm_percent.is_none() {
        return true;
    }
    if !underlying_spot.is_finite() || underlying_spot <= 0.0 {
        return false;
    }

    let otm_percent = match side {
        ContractSide::Call => (strike - underlying_spot) / underlying_spot,
        ContractSide::Put => (underlying_spot - strike) / underlying_spot,
    };

    if let Some(min) = min_otm_percent
        && otm_percent < min
    {
        return false;
    }
    if let Some(max) = max_otm_percent
        && otm_percent > max
    {
        return false;
    }
    true
}

fn verify_socket_path_exists(socket_path: &Path) -> Result<()> {
    if !socket_path.exists() {
        bail!(
            "Theta daemon is not running (socket not found at {}). Start theta-daemon or set THETA_SOCKET_PATH.",
            socket_path.display()
        );
    }
    Ok(())
}

fn json_value_as_string(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn deserialize_date<'de, D>(deserializer: D) -> std::result::Result<Date, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    parse_expiry_date(&raw).map_err(serde::de::Error::custom)
}

fn deserialize_date_vec<'de, D>(deserializer: D) -> std::result::Result<Vec<Date>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Vec::<String>::deserialize(deserializer)?;
    raw.into_iter()
        .map(|value| parse_expiry_date(&value).map_err(serde::de::Error::custom))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        CLIENT_RPC_TIMEOUT, MAX_OPTION_QUOTE_SYMBOLS_PER_REQUEST, MarketDataClient,
        MarketTradingDays, OptionChainFetchFilter, OptionDirection, OptionQuote, SecurityQuote,
        StrikePriceInfo, decimal_to_f64, dedup_symbols, option_snapshot_from_quote,
        select_option_symbols,
    };
    use crate::analytics::ContractSide;
    use crate::daemon_protocol::{DaemonError, DaemonErrorKind, MAX_DAEMON_RESPONSE_BYTES};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixListener;
    use tokio::time::{Duration, sleep};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    async fn bind_test_listener() -> (tempfile::TempDir, PathBuf, UnixListener) {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("theta.sock");
        let listener = UnixListener::bind(&path).expect("bind test socket");
        (dir, path, listener)
    }

    #[test]
    fn option_quote_deserializes_string_expiry_date() {
        let quote: OptionQuote = serde_json::from_value(serde_json::json!({
            "symbol": "TSLA250321C00300000.US",
            "underlying_symbol": "TSLA.US",
            "direction": "CALL",
            "last_done": "1.23",
            "prev_close": "1.20",
            "open": "1.21",
            "high": "1.30",
            "low": "1.10",
            "volume": 10,
            "turnover": "1234.5",
            "timestamp": "2026-03-06T12:00:00Z",
            "trade_status": "NORMAL",
            "strike_price": "300",
            "expiry_date": "2026-03-21",
            "implied_volatility": "0.55",
            "open_interest": 100,
            "historical_volatility": "0.40",
            "contract_multiplier": "100",
            "contract_size": "100",
            "contract_type": "AMERICAN"
        }))
        .expect("option quote should deserialize");

        assert!(matches!(quote.direction, OptionDirection::Call));
        assert_eq!(quote.expiry_date.to_string(), "2026-03-21");
    }

    #[test]
    fn option_quote_deserializes_title_case_direction() {
        let quote: OptionQuote = serde_json::from_value(serde_json::json!({
            "symbol": "TSLA260320C400000.US",
            "underlying_symbol": "TSLA.US",
            "direction": "Call",
            "last_done": "17.05",
            "prev_close": "17.90",
            "open": "14.65",
            "high": "19.05",
            "low": "14.50",
            "volume": 2872,
            "turnover": "4717016.00",
            "timestamp": "2026-03-05T21:00:00Z",
            "trade_status": "Normal",
            "strike_price": "400.00",
            "expiry_date": "2026-03-20",
            "implied_volatility": "0.417",
            "open_interest": 9259,
            "historical_volatility": "0.3453",
            "contract_multiplier": "100",
            "contract_size": "100",
            "contract_type": "American"
        }))
        .expect("option quote should deserialize");

        assert!(matches!(quote.direction, OptionDirection::Call));
    }

    #[test]
    fn trading_days_deserialize_from_string_dates() {
        let payload = serde_json::json!({
            "market": "U_S",
            "trading_days": ["2026-03-06", "2026-03-09"]
        });

        let days: MarketTradingDays =
            serde_json::from_value(payload).expect("trading days should deserialize");
        let rendered: Vec<String> = days
            .trading_days
            .into_iter()
            .map(|day| day.to_string())
            .collect();
        assert_eq!(rendered, vec!["2026-03-06", "2026-03-09"]);
    }

    #[test]
    fn strike_price_info_deserializes_price_alias() {
        let row: super::StrikePriceInfo = serde_json::from_value(serde_json::json!({
            "price": "400",
            "call_symbol": "TSLA260320C400000.US",
            "put_symbol": "TSLA260320P400000.US",
            "standard": true
        }))
        .expect("strike info should deserialize");

        assert_eq!(row.strike_price, "400");
    }

    #[test]
    fn option_snapshot_normalizes_json_string_fields() {
        let quote: OptionQuote = serde_json::from_value(serde_json::json!({
            "symbol": "TSLA260320C400000.US",
            "underlying_symbol": "TSLA.US",
            "direction": "Call",
            "last_done": "17.05",
            "prev_close": "17.90",
            "open": "14.65",
            "high": "19.05",
            "low": "14.50",
            "volume": 2872,
            "turnover": "4717016.00",
            "timestamp": "2026-03-05T21:00:00Z",
            "trade_status": "Normal",
            "strike_price": "400.00",
            "expiry_date": "2026-03-20",
            "implied_volatility": "0.417",
            "open_interest": 9259,
            "historical_volatility": "0.3453",
            "contract_multiplier": "100",
            "contract_size": "100",
            "contract_type": "American"
        }))
        .expect("option quote should deserialize");

        let snapshot =
            option_snapshot_from_quote(quote).expect("snapshot conversion should succeed");
        assert_eq!(snapshot.trade_status, "Normal");
        assert_eq!(snapshot.contract_style, "American");
    }

    #[test]
    fn decimal_to_f64_rejects_non_finite_values() {
        assert!(decimal_to_f64("NaN", "last_done").is_err());
        assert!(decimal_to_f64("inf", "last_done").is_err());
    }

    #[test]
    fn snapshot_from_quote_rejects_non_finite_prices() {
        let err = super::snapshot_from_quote(SecurityQuote {
            symbol: "TSLA.US".to_string(),
            last_done: "NaN".to_string(),
            prev_close: "390".to_string(),
            open: "395".to_string(),
            high: "405".to_string(),
            low: "392".to_string(),
            volume: 1,
            turnover: "1".to_string(),
            timestamp: "2026-02-28T09:30:00Z".to_string(),
        })
        .unwrap_err();

        assert!(err.to_string().contains("last_done must be finite"));
    }

    #[test]
    fn socket_path_prefers_env_override() {
        let _guard = env_lock().lock().expect("lock poisoned");
        unsafe {
            std::env::set_var("THETA_SOCKET_PATH", "/tmp/test-theta.sock");
            std::env::remove_var("HOME");
        }

        let resolved = crate::runtime::theta_socket_path().expect("socket path");
        assert_eq!(resolved, PathBuf::from("/tmp/test-theta.sock"));
    }

    #[test]
    fn dedup_symbols_preserves_first_seen_order() {
        let symbols = vec![
            "A.US".to_string(),
            "B.US".to_string(),
            "A.US".to_string(),
            "C.US".to_string(),
            "B.US".to_string(),
        ];

        assert_eq!(
            dedup_symbols(&symbols),
            vec!["A.US".to_string(), "B.US".to_string(), "C.US".to_string()]
        );
    }

    #[test]
    fn select_option_symbols_applies_side_and_otm_filters_before_quote() {
        let info = vec![
            StrikePriceInfo {
                strike_price: "350".to_string(),
                call_symbol: "TSLA250101C350000.US".to_string(),
                put_symbol: "TSLA250101P350000.US".to_string(),
                standard: true,
            },
            StrikePriceInfo {
                strike_price: "400".to_string(),
                call_symbol: "TSLA250101C400000.US".to_string(),
                put_symbol: "TSLA250101P400000.US".to_string(),
                standard: true,
            },
            StrikePriceInfo {
                strike_price: "450".to_string(),
                call_symbol: "TSLA250101C450000.US".to_string(),
                put_symbol: "TSLA250101P450000.US".to_string(),
                standard: true,
            },
        ];

        let selected = select_option_symbols(
            &info,
            400.0,
            OptionChainFetchFilter {
                side: Some(ContractSide::Call),
                min_strike: None,
                max_strike: None,
                min_otm_percent: Some(0.0),
                max_otm_percent: Some(0.10),
            },
        );

        assert_eq!(selected, vec!["TSLA250101C400000.US".to_string()]);
    }

    #[test]
    fn option_quote_batch_size_matches_longport_limit() {
        assert_eq!(MAX_OPTION_QUOTE_SYMBOLS_PER_REQUEST, 500);
    }

    #[test]
    fn days_to_expiry_returns_zero_for_expired_contracts() {
        let today = time::macros::date!(2026 - 03 - 07);
        let expiry = time::macros::date!(2026 - 03 - 06);

        assert_eq!(super::days_to_expiry_from(expiry, today), 0);
    }

    #[test]
    fn days_to_expiry_treats_same_day_as_one_day() {
        let today = time::macros::date!(2026 - 03 - 07);
        let expiry = time::macros::date!(2026 - 03 - 07);

        assert_eq!(super::days_to_expiry_from(expiry, today), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_malformed_daemon_response() {
        let (_dir, path, listener) = bind_test_listener().await;
        let client = MarketDataClient::for_tests(path);

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            stream
                .write_all(b"not-json\n")
                .await
                .expect("write malformed frame");
        });

        let err = client
            .fetch_option_expiries("TSLA.US")
            .await
            .expect_err("malformed response should fail");
        assert!(err.to_string().contains("Failed to parse daemon response"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn surfaces_structured_daemon_errors() {
        let (_dir, path, listener) = bind_test_listener().await;
        let client = MarketDataClient::for_tests(path);

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let payload = serde_json::json!({
                "result": null,
                "error": {
                    "kind": "rate_limit",
                    "method": "option_quote",
                    "provider_code": 301607,
                    "message": "Too many option securities request within one minute"
                }
            });
            let payload = serde_json::to_vec(&payload).expect("serialize payload");
            stream.write_all(&payload).await.expect("write payload");
            stream.write_all(b"\n").await.expect("write newline");
        });

        let err = client
            .fetch_option_expiries("TSLA.US")
            .await
            .expect_err("structured error should fail");
        let daemon_err = err
            .downcast_ref::<DaemonError>()
            .expect("daemon error should be downcastable");
        assert_eq!(daemon_err.kind, DaemonErrorKind::RateLimit);
        assert_eq!(daemon_err.provider_code, Some(301607));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rejects_oversized_daemon_response() {
        let (_dir, path, listener) = bind_test_listener().await;
        let client = MarketDataClient::for_tests(path);

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let mut payload = vec![b'x'; MAX_DAEMON_RESPONSE_BYTES + 1];
            payload.push(b'\n');
            stream
                .write_all(&payload)
                .await
                .expect("write oversized frame");
        });

        let err = client
            .fetch_option_expiries("TSLA.US")
            .await
            .expect_err("oversized response should fail");
        assert!(err.to_string().contains("exceeded"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn rpc_times_out_when_server_never_replies() {
        let (_dir, path, listener) = bind_test_listener().await;
        let client = MarketDataClient::for_tests(path);

        tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("accept");
            sleep(CLIENT_RPC_TIMEOUT + Duration::from_secs(1)).await;
        });

        let err = client
            .fetch_option_expiries("TSLA.US")
            .await
            .expect_err("missing response should time out");
        assert!(err.to_string().contains("timed out"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn batch_quote_rejects_non_finite_prices() {
        let (_dir, path, listener) = bind_test_listener().await;
        let client = MarketDataClient::for_tests(path);

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let payload = serde_json::json!({
                "result": [{
                    "symbol": "TSLA.US",
                    "last_done": "NaN",
                    "prev_close": "390",
                    "open": "395",
                    "high": "405",
                    "low": "392",
                    "volume": 1,
                    "turnover": "1",
                    "timestamp": "2026-02-28T09:30:00Z"
                }],
                "error": null
            });
            let mut bytes = serde_json::to_vec(&payload).expect("serialize payload");
            bytes.push(b'\n');
            stream.write_all(&bytes).await.expect("write response");
        });

        let err = client
            .batch_quote(&["TSLA.US".to_string()])
            .await
            .expect_err("non-finite prices should fail");
        assert!(
            err.to_string()
                .contains("invalid batch quote price for TSLA.US")
        );
    }
}
