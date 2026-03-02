use crate::analytics::ContractSide;
use crate::domain::{OptionChainSnapshot, OptionContractSnapshot, ProviderGreeks, UnderlyingSnapshot};
use anyhow::{bail, Context, Result};
use longport::{
    quote::{CalcIndex, OptionDirection, OptionQuote, QuoteContext, SecurityCalcIndex},
    Config as LongportConfig, Decimal,
};
use std::sync::Arc;
use time::{Date, OffsetDateTime};

pub struct MarketDataClient {
    ctx: QuoteContext,
}

impl MarketDataClient {
    pub async fn from_env() -> Result<Self> {
        if !credentials_ready() {
            bail!(
                "missing API credentials; set LONGPORT_APP_KEY, LONGPORT_APP_SECRET, and LONGPORT_ACCESS_TOKEN"
            );
        }

        let config = Arc::new(
            LongportConfig::from_env()
                .context("failed to load API config")?
                .dont_print_quote_packages(),
        );
        let (ctx, _) = QuoteContext::try_new(config)
            .await
            .context("failed to initialize quote context")?;

        Ok(Self { ctx })
    }

    pub async fn probe(&self, symbol: &str) -> Result<usize> {
        let quotes = self
            .ctx
            .quote([symbol])
            .await
            .with_context(|| format!("failed to fetch quote for {}", symbol))?;
        Ok(quotes.len())
    }

    pub async fn fetch_underlying(&self, symbol: &str) -> Result<UnderlyingSnapshot> {
        let mut quotes = self
            .ctx
            .quote([symbol])
            .await
            .with_context(|| format!("failed to fetch quote for {}", symbol))?;
        let quote = quotes
            .pop()
            .with_context(|| format!("no quote returned for {}", symbol))?;
        let last_done_f64 = decimal_to_f64(&quote.last_done, "underlying last_done")?;
        let prev_close_f64 = decimal_to_f64(&quote.prev_close, "underlying prev_close")?;
        let open_f64 = decimal_to_f64(&quote.open, "underlying open")?;
        let high_f64 = decimal_to_f64(&quote.high, "underlying high")?;
        let low_f64 = decimal_to_f64(&quote.low, "underlying low")?;
        let turnover_f64 = decimal_to_f64(&quote.turnover, "underlying turnover")?;

        Ok(UnderlyingSnapshot {
            symbol: quote.symbol,
            last_done: quote.last_done.to_string(),
            last_done_f64,
            prev_close: quote.prev_close.to_string(),
            prev_close_f64,
            open: quote.open.to_string(),
            open_f64,
            high: quote.high.to_string(),
            high_f64,
            low: quote.low.to_string(),
            low_f64,
            volume: quote.volume,
            turnover: quote.turnover.to_string(),
            turnover_f64,
            timestamp: quote.timestamp.to_string(),
        })
    }

    pub async fn fetch_option_contract(&self, symbol: &str) -> Result<OptionContractSnapshot> {
        let mut quotes = self
            .ctx
            .option_quote([symbol])
            .await
            .with_context(|| format!("failed to fetch option quote for {}", symbol))?;
        let quote = quotes
            .pop()
            .with_context(|| format!("no option quote returned for {}", symbol))?;

        option_snapshot_from_quote(quote)
    }

    pub async fn fetch_option_expiries(&self, symbol: &str) -> Result<Vec<String>> {
        let expiries = self
            .ctx
            .option_chain_expiry_date_list(symbol)
            .await
            .with_context(|| format!("failed to fetch option expiries for {}", symbol))?;

        Ok(expiries.into_iter().map(|date| date.to_string()).collect())
    }

    pub async fn fetch_option_chain(&self, symbol: &str, expiry: Date) -> Result<OptionChainSnapshot> {
        let underlying = self.fetch_underlying(symbol).await?;
        let days_to_expiry = days_to_expiry(expiry);

        let strikes = self
            .ctx
            .option_chain_info_by_date(symbol, expiry)
            .await
            .with_context(|| format!("failed to fetch option chain for {} @ {}", symbol, expiry))?;

        let option_symbols: Vec<String> = strikes
            .iter()
            .flat_map(|strike| [strike.call_symbol.clone(), strike.put_symbol.clone()])
            .collect();
        let option_quotes = self
            .ctx
            .option_quote(option_symbols.iter().map(String::as_str))
            .await
            .with_context(|| format!("failed to fetch option quotes for {} @ {}", symbol, expiry))?;

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

    pub async fn fetch_provider_greeks(&self, symbol: &str) -> Result<ProviderGreeks> {
        let mut rows = self
            .ctx
            .calc_indexes(
                [symbol],
                [
                    CalcIndex::ImpliedVolatility,
                    CalcIndex::Delta,
                    CalcIndex::Gamma,
                    CalcIndex::Theta,
                    CalcIndex::Vega,
                    CalcIndex::Rho,
                ],
            )
            .await?;
        let row = rows
            .pop()
            .with_context(|| format!("no calc_indexes row returned for {}", symbol))?;

        Ok(provider_greeks_view(row))
    }
}

pub fn credentials_ready() -> bool {
    env_var("LONGPORT_APP_KEY").is_some()
        && env_var("LONGPORT_APP_SECRET").is_some()
        && env_var("LONGPORT_ACCESS_TOKEN").is_some()
}

pub fn parse_expiry_date(input: &str) -> Result<Date> {
    let format = time::format_description::parse("[year]-[month]-[day]")
        .context("failed to build date parser")?;
    Date::parse(input, &format)
        .with_context(|| format!("invalid expiry date `{}`; expected YYYY-MM-DD", input))
}

pub fn days_to_expiry(expiry: Date) -> i64 {
    let today = OffsetDateTime::now_utc().date();
    let days = (expiry - today).whole_days();
    if days < 1 { 1 } else { days }
}

pub fn decimal_to_f64(value: &Decimal, field: &str) -> Result<f64> {
    value
        .to_string()
        .parse::<f64>()
        .with_context(|| format!("failed to parse {} as f64", field))
}

fn option_snapshot_from_quote(quote: OptionQuote) -> Result<OptionContractSnapshot> {
    let last_done_f64 = decimal_to_f64(&quote.last_done, "option last_done")?;
    let prev_close_f64 = decimal_to_f64(&quote.prev_close, "option prev_close")?;
    let open_f64 = decimal_to_f64(&quote.open, "option open")?;
    let high_f64 = decimal_to_f64(&quote.high, "option high")?;
    let low_f64 = decimal_to_f64(&quote.low, "option low")?;
    let turnover_f64 = decimal_to_f64(&quote.turnover, "option turnover")?;
    let strike_price_f64 = decimal_to_f64(&quote.strike_price, "strike_price")?;
    let provider_reported_iv_f64 = decimal_to_f64(&quote.implied_volatility, "implied_volatility")?;
    let historical_volatility_f64 =
        decimal_to_f64(&quote.historical_volatility, "historical_volatility")?;
    let contract_multiplier_f64 =
        decimal_to_f64(&quote.contract_multiplier, "contract_multiplier")?;
    let contract_size_f64 = decimal_to_f64(&quote.contract_size, "contract_size")?;

    Ok(OptionContractSnapshot {
        symbol: quote.symbol,
        underlying_symbol: quote.underlying_symbol,
        option_type: contract_side_from_provider(quote.direction)?,
        last_done: quote.last_done.to_string(),
        last_done_f64,
        prev_close: quote.prev_close.to_string(),
        prev_close_f64,
        open: quote.open.to_string(),
        open_f64,
        high: quote.high.to_string(),
        high_f64,
        low: quote.low.to_string(),
        low_f64,
        timestamp: quote.timestamp.to_string(),
        volume: quote.volume,
        turnover: quote.turnover.to_string(),
        turnover_f64,
        trade_status: format!("{:?}", quote.trade_status),
        strike_price: quote.strike_price.to_string(),
        strike_price_f64,
        expiry: quote.expiry_date,
        provider_reported_iv: quote.implied_volatility.to_string(),
        provider_reported_iv_f64,
        open_interest: quote.open_interest,
        historical_volatility: quote.historical_volatility.to_string(),
        historical_volatility_f64,
        contract_multiplier: quote.contract_multiplier.to_string(),
        contract_multiplier_f64,
        contract_size: quote.contract_size.to_string(),
        contract_size_f64,
        contract_style: format!("{:?}", quote.contract_type),
    })
}

fn contract_side_from_provider(direction: OptionDirection) -> Result<ContractSide> {
    match direction {
        OptionDirection::Call => Ok(ContractSide::Call),
        OptionDirection::Put => Ok(ContractSide::Put),
        OptionDirection::Unknown => bail!("provider returned unknown option direction"),
    }
}

fn provider_greeks_view(row: SecurityCalcIndex) -> ProviderGreeks {
    ProviderGreeks {
        delta: row.delta.map(|value| value.to_string()),
        gamma: row.gamma.map(|value| value.to_string()),
        theta: row.theta.map(|value| value.to_string()),
        vega: row.vega.map(|value| value.to_string()),
        rho: row.rho.map(|value| value.to_string()),
        implied_volatility: row.implied_volatility.map(|value| value.to_string()),
    }
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
