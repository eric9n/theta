use crate::analytics::{
    PricingInput, calculate_metrics, calculate_metrics_batch, implied_volatility_from_price,
};
use crate::config::AppConfig;
use crate::diagnostics::analyze_contract;
use crate::domain::{ChainAnalysisRow, ChainAnalysisView, IvComparison, OptionAnalysisView};
use crate::market_data::{MarketDataClient, days_to_expiry};
use crate::rate::RateCurve;
use crate::screening_service::{
    ChainScreeningRequest, apply_chain_screening, validate_metric_bounds, validate_strike_bounds,
};
use anyhow::{bail, Result};

pub struct AnalyzeOptionRequest {
    pub symbol: String,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_option_price: Option<f64>,
    pub iv_from_market_price: bool,
    pub show_iv_diff: bool,
    pub use_provider_greeks: bool,
}

#[derive(Clone)]
pub struct AnalyzeChainRequest {
    pub symbol: String,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub screening: ChainScreeningRequest,
}

pub struct ThetaAnalysisService {
    market: MarketDataClient,
    rate_curve: RateCurve,
}

impl ThetaAnalysisService {
    pub async fn from_env() -> Result<Self> {
        let config = AppConfig::load()?;
        Ok(Self {
            market: MarketDataClient::connect().await?,
            rate_curve: config.rate_curve,
        })
    }

    pub fn market(&self) -> &MarketDataClient {
        &self.market
    }

    pub async fn analyze_option(&self, req: AnalyzeOptionRequest) -> Result<OptionAnalysisView> {
        let contract = self.market.fetch_option_contract(&req.symbol).await?;
        let underlying = self
            .market
            .fetch_underlying(&contract.underlying_symbol)
            .await?;
        let days = days_to_expiry(contract.expiry);
        let (rate, rate_source) = resolve_rate(req.rate, self.rate_curve, days);
        let (effective_iv, iv_source, iv_reference_price) = resolve_iv(
            req.iv,
            req.iv_from_option_price,
            req.iv_from_market_price,
            contract.last_done_f64,
            underlying.last_done_f64,
            contract.strike_price_f64,
            rate,
            days,
            req.dividend,
            contract.option_type,
            contract.provider_reported_iv_f64,
        )?;

        let input = PricingInput::new(
            underlying.last_done_f64,
            contract.strike_price_f64,
            rate,
            effective_iv,
            days as f64,
            req.dividend,
            contract.option_type,
        )?;

        let iv_comparison = if req.show_iv_diff {
            let solved_iv = implied_volatility_from_price(
                underlying.last_done_f64,
                contract.strike_price_f64,
                rate,
                days as f64,
                req.dividend,
                contract.option_type,
                contract.last_done_f64,
            )?;
            Some(IvComparison {
                provider_iv: contract.provider_reported_iv.clone(),
                solved_from_market_price_iv: solved_iv.to_string(),
                diff: (solved_iv - contract.provider_reported_iv_f64).to_string(),
                reference_price: contract.last_done.clone(),
            })
        } else {
            None
        };

        Ok(OptionAnalysisView {
            option_symbol: contract.symbol.clone(),
            underlying_symbol: contract.underlying_symbol.clone(),
            underlying_price: underlying.last_done.clone(),
            option_price: contract.last_done.clone(),
            strike_price: contract.strike_price.clone(),
            expiry: contract.expiry.to_string(),
            days_to_expiry: days,
            implied_volatility: effective_iv.to_string(),
            implied_volatility_source: iv_source,
            provider_reported_iv: contract.provider_reported_iv.clone(),
            iv_reference_price,
            rate,
            rate_source,
            dividend: req.dividend,
            option_type: contract.option_type,
            diagnostics: analyze_contract(&contract, underlying.last_done_f64, days),
            local_greeks: calculate_metrics(&input),
            provider_greeks: if req.use_provider_greeks {
                Some(self.market.fetch_provider_greeks(&req.symbol).await?)
            } else {
                None
            },
            iv_comparison,
        })
    }

    pub async fn analyze_chain(
        &self,
        expiry: time::Date,
        req: AnalyzeChainRequest,
    ) -> Result<ChainAnalysisView> {
        validate_strike_bounds(req.screening.min_strike, req.screening.max_strike)?;
        validate_metric_bounds(req.screening.min_delta, req.screening.max_delta, "delta")?;
        validate_metric_bounds(req.screening.min_theta, req.screening.max_theta, "theta")?;
        validate_metric_bounds(req.screening.min_vega, req.screening.max_vega, "vega")?;
        validate_metric_bounds(req.screening.min_iv, req.screening.max_iv, "iv")?;
        validate_metric_bounds(
            req.screening.min_option_price,
            req.screening.max_option_price,
            "option_price",
        )?;
        validate_metric_bounds(
            req.screening.min_otm_percent,
            req.screening.max_otm_percent,
            "otm_percent",
        )?;

        let chain = self.market.fetch_option_chain(&req.symbol, expiry).await?;
        let (rate, rate_source) = resolve_rate(req.rate, self.rate_curve, chain.days_to_expiry);
        let mut metas = Vec::with_capacity(chain.contracts.len());
        let mut inputs = Vec::with_capacity(chain.contracts.len());
        let underlying_spot = chain.underlying.last_done_f64;
        let days_to_expiry = chain.days_to_expiry;

        for contract in chain.contracts {
            let (effective_iv, iv_source, _) = match resolve_iv(
                req.iv,
                None,
                req.iv_from_market_price,
                contract.last_done_f64,
                underlying_spot,
                contract.strike_price_f64,
                rate,
                days_to_expiry,
                req.dividend,
                contract.option_type,
                contract.provider_reported_iv_f64,
            ) {
                Ok(res) => res,
                Err(e) => {
                    tracing::warn!("Skipping contract {} due to IV resolution error: {}", contract.symbol, e);
                    continue;
                }
            };

            if effective_iv <= 0.0 {
                tracing::warn!("Skipping contract {} due to invalid 0.0 IV", contract.symbol);
                continue;
            }

            inputs.push(PricingInput::new(
                underlying_spot,
                contract.strike_price_f64,
                rate,
                effective_iv,
                days_to_expiry as f64,
                req.dividend,
                contract.option_type,
            )?);
            let diagnostics = analyze_contract(&contract, underlying_spot, days_to_expiry);
            metas.push((
                contract.symbol,
                contract.option_type,
                contract.last_done,
                contract.volume,
                contract.open_interest,
                contract.strike_price,
                effective_iv.to_string(),
                iv_source,
                contract.provider_reported_iv,
                diagnostics,
            ));
        }

        let metrics = calculate_metrics_batch(&inputs);
        let mut rows: Vec<ChainAnalysisRow> = metas
            .into_iter()
            .zip(metrics)
            .map(|(meta, local_greeks)| ChainAnalysisRow {
                option_symbol: meta.0,
                option_type: meta.1,
                option_price: meta.2,
                volume: meta.3,
                open_interest: meta.4,
                strike_price: meta.5,
                implied_volatility: meta.6,
                implied_volatility_source: meta.7,
                provider_reported_iv: meta.8,
                diagnostics: meta.9,
                local_greeks,
            })
            .collect();

        apply_chain_screening(&mut rows, &req.screening, underlying_spot);

        Ok(ChainAnalysisView {
            underlying_symbol: chain.underlying.symbol,
            underlying_price: chain.underlying.last_done,
            expiry: chain.expiry.to_string(),
            days_to_expiry: chain.days_to_expiry,
            rate,
            rate_source,
            rows,
        })
    }
}

fn resolve_rate(explicit_rate: Option<f64>, curve: RateCurve, days_to_expiry: i64) -> (f64, String) {
    if let Some(rate) = explicit_rate {
        (rate, "manual".to_string())
    } else {
        (curve.rate_for_days(days_to_expiry), "curve_default".to_string())
    }
}

fn resolve_iv(
    manual_iv: Option<f64>,
    iv_from_option_price: Option<f64>,
    iv_from_market_price: bool,
    market_option_price: f64,
    spot: f64,
    strike: f64,
    rate: f64,
    days: i64,
    dividend: f64,
    option_type: crate::analytics::ContractSide,
    provider_iv: f64,
) -> Result<(f64, String, Option<String>)> {
    if let Some(iv) = manual_iv {
        if iv <= 0.0 {
            bail!("iv must be greater than 0");
        }
        return Ok((iv, "manual".to_string(), None));
    }

    if let Some(price) = iv_from_option_price {
        let solved = implied_volatility_from_price(
            spot,
            strike,
            rate,
            days as f64,
            dividend,
            option_type,
            price,
        )?;
        return Ok((solved, "solved_from_price".to_string(), Some(price.to_string())));
    }

    if iv_from_market_price {
        let solved = implied_volatility_from_price(
            spot,
            strike,
            rate,
            days as f64,
            dividend,
            option_type,
            market_option_price,
        )?;
        return Ok((
            solved,
            "solved_from_market_price".to_string(),
            Some(market_option_price.to_string()),
        ));
    }

    Ok((provider_iv, "provider".to_string(), None))
}
