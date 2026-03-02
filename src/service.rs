use crate::analytics::{
    ContractSide, PricingInput, calculate_metrics, calculate_metrics_batch,
    implied_volatility_from_price,
};
use crate::domain::{ChainAnalysisRow, ChainAnalysisView, IvComparison, OptionAnalysisView};
use crate::market_data::{MarketDataClient, days_to_expiry};
use anyhow::{bail, Result};

#[derive(Copy, Clone, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum ChainSortField {
    Delta,
    Theta,
    Vega,
    Iv,
    Strike,
}

pub struct AnalyzeOptionRequest {
    pub symbol: String,
    pub rate: f64,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_option_price: Option<f64>,
    pub iv_from_market_price: bool,
    pub show_iv_diff: bool,
    pub use_provider_greeks: bool,
}

pub struct AnalyzeChainRequest {
    pub symbol: String,
    pub rate: f64,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub side: Option<ContractSide>,
    pub min_strike: Option<f64>,
    pub max_strike: Option<f64>,
    pub sort_by: Option<ChainSortField>,
    pub limit: Option<usize>,
}

pub struct ThetaService {
    market: MarketDataClient,
}

impl ThetaService {
    pub async fn from_env() -> Result<Self> {
        Ok(Self {
            market: MarketDataClient::from_env().await?,
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
        let (effective_iv, iv_source, iv_reference_price) = resolve_iv(
            req.iv,
            req.iv_from_option_price,
            req.iv_from_market_price,
            contract.price_f64,
            underlying.price_f64,
            contract.strike_price_f64,
            req.rate,
            days,
            req.dividend,
            contract.option_type,
            contract.provider_reported_iv_f64,
        )?;

        let input = PricingInput::new(
            underlying.price_f64,
            contract.strike_price_f64,
            req.rate,
            effective_iv,
            days as f64,
            req.dividend,
            contract.option_type,
        )?;

        let iv_comparison = if req.show_iv_diff {
            let solved_iv = implied_volatility_from_price(
                underlying.price_f64,
                contract.strike_price_f64,
                req.rate,
                days as f64,
                req.dividend,
                contract.option_type,
                contract.price_f64,
            )?;
            Some(IvComparison {
                provider_iv: contract.provider_reported_iv.clone(),
                solved_from_market_price_iv: solved_iv.to_string(),
                diff: (solved_iv - contract.provider_reported_iv_f64).to_string(),
                reference_price: contract.price.clone(),
            })
        } else {
            None
        };

        Ok(OptionAnalysisView {
            option_symbol: contract.symbol.clone(),
            underlying_symbol: contract.underlying_symbol.clone(),
            underlying_price: underlying.price.clone(),
            option_price: contract.price.clone(),
            strike_price: contract.strike_price.clone(),
            expiry: contract.expiry.to_string(),
            days_to_expiry: days,
            implied_volatility: effective_iv.to_string(),
            implied_volatility_source: iv_source,
            provider_reported_iv: contract.provider_reported_iv.clone(),
            iv_reference_price,
            rate: req.rate,
            dividend: req.dividend,
            option_type: contract.option_type,
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
        if let (Some(min), Some(max)) = (req.min_strike, req.max_strike)
            && min > max
        {
            bail!("min_strike must be less than or equal to max_strike");
        }

        let chain = self.market.fetch_option_chain(&req.symbol, expiry).await?;
        let mut metas = Vec::with_capacity(chain.contracts.len());
        let mut inputs = Vec::with_capacity(chain.contracts.len());

        for contract in chain.contracts {
            let (effective_iv, iv_source, _) = resolve_iv(
                req.iv,
                None,
                req.iv_from_market_price,
                contract.price_f64,
                chain.underlying.price_f64,
                contract.strike_price_f64,
                req.rate,
                chain.days_to_expiry,
                req.dividend,
                contract.option_type,
                contract.provider_reported_iv_f64,
            )?;
            inputs.push(PricingInput::new(
                chain.underlying.price_f64,
                contract.strike_price_f64,
                req.rate,
                effective_iv,
                chain.days_to_expiry as f64,
                req.dividend,
                contract.option_type,
            )?);
            metas.push((
                contract.symbol,
                contract.option_type,
                contract.price,
                contract.strike_price,
                effective_iv.to_string(),
                iv_source,
                contract.provider_reported_iv,
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
                strike_price: meta.3,
                implied_volatility: meta.4,
                implied_volatility_source: meta.5,
                provider_reported_iv: meta.6,
                local_greeks,
            })
            .collect();

        rows.retain(|row| matches_side(row, req.side) && matches_strike(row, req.min_strike, req.max_strike));
        sort_chain_rows(&mut rows, req.sort_by);
        if let Some(limit) = req.limit {
            rows.truncate(limit);
        }

        Ok(ChainAnalysisView {
            underlying_symbol: chain.underlying.symbol,
            underlying_price: chain.underlying.price,
            expiry: chain.expiry.to_string(),
            days_to_expiry: chain.days_to_expiry,
            rows,
        })
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
    option_type: ContractSide,
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

fn matches_side(row: &ChainAnalysisRow, side: Option<ContractSide>) -> bool {
    side.is_none_or(|side| row.option_type == side)
}

fn matches_strike(row: &ChainAnalysisRow, min_strike: Option<f64>, max_strike: Option<f64>) -> bool {
    let strike = row.strike_price.parse::<f64>().unwrap_or(f64::NAN);

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

fn sort_chain_rows(rows: &mut [ChainAnalysisRow], sort_by: Option<ChainSortField>) {
    let Some(sort_by) = sort_by else {
        return;
    };

    rows.sort_by(|a, b| {
        let ordering = match sort_by {
            ChainSortField::Delta => b.local_greeks.delta.total_cmp(&a.local_greeks.delta),
            ChainSortField::Theta => b
                .local_greeks
                .theta_per_day
                .total_cmp(&a.local_greeks.theta_per_day),
            ChainSortField::Vega => b.local_greeks.vega.total_cmp(&a.local_greeks.vega),
            ChainSortField::Iv => {
                let a_iv = a.implied_volatility.parse::<f64>().unwrap_or(f64::NAN);
                let b_iv = b.implied_volatility.parse::<f64>().unwrap_or(f64::NAN);
                b_iv.total_cmp(&a_iv)
            }
            ChainSortField::Strike => {
                let a_strike = a.strike_price.parse::<f64>().unwrap_or(f64::NAN);
                let b_strike = b.strike_price.parse::<f64>().unwrap_or(f64::NAN);
                a_strike.total_cmp(&b_strike)
            }
        };

        ordering.then_with(|| a.option_symbol.cmp(&b.option_symbol))
    });
}
