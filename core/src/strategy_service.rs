use crate::analysis_service::{AnalyzeChainRequest, ThetaAnalysisService};
use crate::analytics::{ContractSide, implied_volatility_from_price};
use crate::domain::{
    BearCallSpreadCandidate, BearCallSpreadView, BearPutSpreadCandidate, BearPutSpreadView,
    BullCallSpreadCandidate, BullCallSpreadView, BullPutSpreadCandidate, BullPutSpreadView,
    CalendarCallSpreadCandidate, CalendarCallSpreadView, CalendarPutSpreadCandidate,
    CalendarPutSpreadView, CashSecuredPutCandidate, CashSecuredPutView, ChainAnalysisRow,
    ChainAnalysisView, CoveredCallCandidate, CoveredCallView, DiagonalCallSpreadCandidate,
    DiagonalCallSpreadView, DiagonalPutSpreadCandidate, DiagonalPutSpreadView, MispricingCandidate,
    MispricingView, SellOpportunitiesView, SellOpportunityCandidate, SellOpportunityReturnBasis,
};
use crate::market_data::decimal_to_f64;
use crate::screening_service::ChainScreeningRequest;
use anyhow::{Result, bail};
use std::collections::HashMap;
use time::Date;

pub struct CashSecuredPutRequest {
    pub symbol: String,
    pub expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub direction: Option<MispricingDirection>,
    pub iv_direction: Option<IvDiffDirection>,
    pub min_delta: Option<f64>,
    pub max_delta: Option<f64>,
    pub min_otm_percent: Option<f64>,
    pub max_otm_percent: Option<f64>,
    pub min_option_price: Option<f64>,
    pub max_option_price: Option<f64>,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub min_premium_per_contract: Option<f64>,
    pub max_cash_required_per_contract: Option<f64>,
    pub min_annualized_return_on_cash: Option<f64>,
    pub max_annualized_return_on_cash: Option<f64>,
    pub min_abs_mispricing_percent: Option<f64>,
    pub min_abs_iv_diff_percent: Option<f64>,
    pub limit: Option<usize>,
}

pub struct CoveredCallRequest {
    pub symbol: String,
    pub expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub direction: Option<MispricingDirection>,
    pub iv_direction: Option<IvDiffDirection>,
    pub min_delta: Option<f64>,
    pub max_delta: Option<f64>,
    pub min_otm_percent: Option<f64>,
    pub max_otm_percent: Option<f64>,
    pub min_option_price: Option<f64>,
    pub max_option_price: Option<f64>,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub min_premium_per_contract: Option<f64>,
    pub min_annualized_premium_yield: Option<f64>,
    pub max_annualized_premium_yield: Option<f64>,
    pub min_abs_mispricing_percent: Option<f64>,
    pub min_abs_iv_diff_percent: Option<f64>,
    pub limit: Option<usize>,
}

pub struct BullPutSpreadRequest {
    pub symbol: String,
    pub expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub direction: Option<MispricingDirection>,
    pub iv_direction: Option<IvDiffDirection>,
    pub min_short_delta: Option<f64>,
    pub max_short_delta: Option<f64>,
    pub min_short_otm_percent: Option<f64>,
    pub max_short_otm_percent: Option<f64>,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub min_net_credit_per_spread: Option<f64>,
    pub min_width_per_spread: Option<f64>,
    pub max_width_per_spread: Option<f64>,
    pub min_annualized_return_on_risk: Option<f64>,
    pub max_annualized_return_on_risk: Option<f64>,
    pub min_abs_mispricing_percent: Option<f64>,
    pub min_abs_iv_diff_percent: Option<f64>,
    pub limit: Option<usize>,
}

pub struct BearCallSpreadRequest {
    pub symbol: String,
    pub expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub direction: Option<MispricingDirection>,
    pub iv_direction: Option<IvDiffDirection>,
    pub min_short_delta: Option<f64>,
    pub max_short_delta: Option<f64>,
    pub min_short_otm_percent: Option<f64>,
    pub max_short_otm_percent: Option<f64>,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub min_net_credit_per_spread: Option<f64>,
    pub min_width_per_spread: Option<f64>,
    pub max_width_per_spread: Option<f64>,
    pub min_annualized_return_on_risk: Option<f64>,
    pub max_annualized_return_on_risk: Option<f64>,
    pub min_abs_mispricing_percent: Option<f64>,
    pub min_abs_iv_diff_percent: Option<f64>,
    pub limit: Option<usize>,
}

pub struct BearPutSpreadRequest {
    pub symbol: String,
    pub expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub max_net_debit_per_spread: Option<f64>,
    pub min_width_per_spread: Option<f64>,
    pub max_width_per_spread: Option<f64>,
    pub min_annualized_return_on_risk: Option<f64>,
    pub max_annualized_return_on_risk: Option<f64>,
    pub limit: Option<usize>,
}

pub struct BullCallSpreadRequest {
    pub symbol: String,
    pub expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub max_net_debit_per_spread: Option<f64>,
    pub min_width_per_spread: Option<f64>,
    pub max_width_per_spread: Option<f64>,
    pub min_annualized_return_on_risk: Option<f64>,
    pub max_annualized_return_on_risk: Option<f64>,
    pub limit: Option<usize>,
}

pub struct CalendarCallSpreadRequest {
    pub symbol: String,
    pub near_expiry: Date,
    pub far_expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub max_net_debit_per_spread: Option<f64>,
    pub min_net_theta_carry_per_day: Option<f64>,
    pub min_days_gap: Option<i64>,
    pub max_days_gap: Option<i64>,
    pub min_strike_gap: Option<f64>,
    pub max_strike_gap: Option<f64>,
    pub sort_by: Option<CrossExpirySortField>,
    pub limit: Option<usize>,
}

pub struct CalendarPutSpreadRequest {
    pub symbol: String,
    pub near_expiry: Date,
    pub far_expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub max_net_debit_per_spread: Option<f64>,
    pub min_net_theta_carry_per_day: Option<f64>,
    pub min_days_gap: Option<i64>,
    pub max_days_gap: Option<i64>,
    pub min_strike_gap: Option<f64>,
    pub max_strike_gap: Option<f64>,
    pub sort_by: Option<CrossExpirySortField>,
    pub limit: Option<usize>,
}

pub struct DiagonalCallSpreadRequest {
    pub symbol: String,
    pub near_expiry: Date,
    pub far_expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub max_net_debit_per_spread: Option<f64>,
    pub min_net_theta_carry_per_day: Option<f64>,
    pub min_days_gap: Option<i64>,
    pub max_days_gap: Option<i64>,
    pub min_strike_gap: Option<f64>,
    pub max_strike_gap: Option<f64>,
    pub sort_by: Option<CrossExpirySortField>,
    pub limit: Option<usize>,
}

pub struct DiagonalPutSpreadRequest {
    pub symbol: String,
    pub near_expiry: Date,
    pub far_expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub max_net_debit_per_spread: Option<f64>,
    pub min_net_theta_carry_per_day: Option<f64>,
    pub min_days_gap: Option<i64>,
    pub max_days_gap: Option<i64>,
    pub min_strike_gap: Option<f64>,
    pub max_strike_gap: Option<f64>,
    pub sort_by: Option<CrossExpirySortField>,
    pub limit: Option<usize>,
}

pub struct MispricingRequest {
    pub symbol: String,
    pub expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub side: Option<ContractSide>,
    pub direction: Option<MispricingDirection>,
    pub iv_direction: Option<IvDiffDirection>,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub min_abs_mispricing_percent: Option<f64>,
    pub min_abs_iv_diff_percent: Option<f64>,
    pub sort_by: Option<MispricingSortField>,
    pub limit: Option<usize>,
}

pub struct SellOpportunitiesRequest {
    pub symbol: String,
    pub expiry: Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub direction: Option<MispricingDirection>,
    pub iv_direction: Option<IvDiffDirection>,
    pub include_calendars: Option<bool>,
    pub include_diagonals: Option<bool>,
    pub min_open_interest: Option<i64>,
    pub min_volume: Option<i64>,
    pub min_abs_mispricing_percent: Option<f64>,
    pub min_abs_iv_diff_percent: Option<f64>,
    pub min_days_gap: Option<i64>,
    pub max_days_gap: Option<i64>,
    pub min_strike_gap: Option<f64>,
    pub max_strike_gap: Option<f64>,
    pub strategy_filter: Vec<SellOpportunityStrategy>,
    pub exclude_strategy_filter: Vec<SellOpportunityStrategy>,
    pub return_basis_filter: Vec<SellOpportunityReturnBasis>,
    pub exclude_return_basis_filter: Vec<SellOpportunityReturnBasis>,
    pub min_premium_or_credit: Option<f64>,
    pub max_risk: Option<f64>,
    pub min_annualized_return: Option<f64>,
    pub max_annualized_return: Option<f64>,
    pub limit_per_strategy: Option<usize>,
    pub sort_by: Option<SellOpportunitySortField>,
    pub limit: Option<usize>,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum MispricingDirection {
    Overpriced,
    Underpriced,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum IvDiffDirection {
    Higher,
    Lower,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum MispricingSortField {
    Mispricing,
    IvDiff,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum CrossExpirySortField {
    Carry,
    Theta,
    VegaToDebit,
    Debit,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum SellOpportunitySortField {
    AnnualizedReturn,
    Mispricing,
    IvDiff,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum SellOpportunityStrategy {
    CashSecuredPut,
    CoveredCall,
    BullPutSpread,
    BearCallSpread,
    CalendarCallSpread,
    CalendarPutSpread,
    DiagonalCallSpread,
    DiagonalPutSpread,
}

pub struct ThetaStrategyService {
    analysis: ThetaAnalysisService,
}

impl ThetaStrategyService {
    pub async fn from_env() -> Result<Self> {
        Ok(Self {
            analysis: ThetaAnalysisService::from_env().await?,
        })
    }

    pub async fn screen_cash_secured_puts(
        &self,
        req: CashSecuredPutRequest,
    ) -> Result<CashSecuredPutView> {
        validate_return_bounds(
            req.min_annualized_return_on_cash,
            req.max_annualized_return_on_cash,
        )?;

        let analysis = self
            .analysis
            .analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Put),
                        min_strike: None,
                        max_strike: None,
                        min_delta: req.min_delta,
                        max_delta: req.max_delta,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: req.min_option_price,
                        max_option_price: req.max_option_price,
                        min_otm_percent: req.min_otm_percent,
                        max_otm_percent: req.max_otm_percent,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        build_cash_secured_put_view(
            analysis,
            req.dividend,
            req.direction,
            req.iv_direction,
            req.min_open_interest,
            req.min_volume,
            req.min_premium_per_contract,
            req.max_cash_required_per_contract,
            req.min_annualized_return_on_cash,
            req.max_annualized_return_on_cash,
            req.min_abs_mispricing_percent,
            req.min_abs_iv_diff_percent,
            req.limit,
        )
    }

    pub async fn screen_covered_calls(&self, req: CoveredCallRequest) -> Result<CoveredCallView> {
        validate_return_bounds(
            req.min_annualized_premium_yield,
            req.max_annualized_premium_yield,
        )?;

        let analysis = self
            .analysis
            .analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Call),
                        min_strike: None,
                        max_strike: None,
                        min_delta: req.min_delta,
                        max_delta: req.max_delta,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: req.min_option_price,
                        max_option_price: req.max_option_price,
                        min_otm_percent: req.min_otm_percent,
                        max_otm_percent: req.max_otm_percent,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        build_covered_call_view(
            analysis,
            req.dividend,
            req.direction,
            req.iv_direction,
            req.min_open_interest,
            req.min_volume,
            req.min_premium_per_contract,
            req.min_annualized_premium_yield,
            req.max_annualized_premium_yield,
            req.min_abs_mispricing_percent,
            req.min_abs_iv_diff_percent,
            req.limit,
        )
    }

    pub async fn screen_bull_put_spreads(
        &self,
        req: BullPutSpreadRequest,
    ) -> Result<BullPutSpreadView> {
        validate_return_bounds(
            req.min_annualized_return_on_risk,
            req.max_annualized_return_on_risk,
        )?;

        let analysis = self
            .analysis
            .analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Put),
                        min_strike: None,
                        max_strike: None,
                        min_delta: req.min_short_delta,
                        max_delta: req.max_short_delta,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: req.min_short_otm_percent,
                        max_otm_percent: req.max_short_otm_percent,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        build_bull_put_spread_view(
            analysis,
            req.dividend,
            req.direction,
            req.iv_direction,
            req.min_open_interest,
            req.min_volume,
            req.min_net_credit_per_spread,
            req.min_width_per_spread,
            req.max_width_per_spread,
            req.min_annualized_return_on_risk,
            req.max_annualized_return_on_risk,
            req.min_abs_mispricing_percent,
            req.min_abs_iv_diff_percent,
            req.limit,
        )
    }

    pub async fn screen_bear_call_spreads(
        &self,
        req: BearCallSpreadRequest,
    ) -> Result<BearCallSpreadView> {
        validate_return_bounds(
            req.min_annualized_return_on_risk,
            req.max_annualized_return_on_risk,
        )?;

        let analysis = self
            .analysis
            .analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Call),
                        min_strike: None,
                        max_strike: None,
                        min_delta: req.min_short_delta,
                        max_delta: req.max_short_delta,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: req.min_short_otm_percent,
                        max_otm_percent: req.max_short_otm_percent,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        build_bear_call_spread_view(
            analysis,
            req.dividend,
            req.direction,
            req.iv_direction,
            req.min_open_interest,
            req.min_volume,
            req.min_net_credit_per_spread,
            req.min_width_per_spread,
            req.max_width_per_spread,
            req.min_annualized_return_on_risk,
            req.max_annualized_return_on_risk,
            req.min_abs_mispricing_percent,
            req.min_abs_iv_diff_percent,
            req.limit,
        )
    }

    pub async fn screen_bear_put_spreads(
        &self,
        req: BearPutSpreadRequest,
    ) -> Result<BearPutSpreadView> {
        validate_return_bounds(
            req.min_annualized_return_on_risk,
            req.max_annualized_return_on_risk,
        )?;

        let analysis = self
            .analysis
            .analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Put),
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        build_bear_put_spread_view(
            analysis,
            req.min_open_interest,
            req.min_volume,
            req.max_net_debit_per_spread,
            req.min_width_per_spread,
            req.max_width_per_spread,
            req.min_annualized_return_on_risk,
            req.max_annualized_return_on_risk,
            req.limit,
        )
    }

    pub async fn screen_bull_call_spreads(
        &self,
        req: BullCallSpreadRequest,
    ) -> Result<BullCallSpreadView> {
        validate_return_bounds(
            req.min_annualized_return_on_risk,
            req.max_annualized_return_on_risk,
        )?;

        let analysis = self
            .analysis
            .analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Call),
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        build_bull_call_spread_view(
            analysis,
            req.min_open_interest,
            req.min_volume,
            req.max_net_debit_per_spread,
            req.min_width_per_spread,
            req.max_width_per_spread,
            req.min_annualized_return_on_risk,
            req.max_annualized_return_on_risk,
            req.limit,
        )
    }

    pub async fn screen_calendar_call_spreads(
        &self,
        req: CalendarCallSpreadRequest,
    ) -> Result<CalendarCallSpreadView> {
        if req.far_expiry <= req.near_expiry {
            bail!("far expiry must be later than near expiry");
        }

        let near = self
            .analysis
            .analyze_chain(
                req.near_expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol.clone(),
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Call),
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        let far = self
            .analysis
            .analyze_chain(
                req.far_expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Call),
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        let mut view = build_calendar_call_spread_view(
            near,
            far,
            req.min_open_interest,
            req.min_volume,
            req.max_net_debit_per_spread,
            req.min_net_theta_carry_per_day,
            req.min_days_gap,
            req.max_days_gap,
            req.min_strike_gap,
            req.max_strike_gap,
            None,
        )?;
        sort_calendar_call_candidates(&mut view.candidates, req.sort_by);
        if let Some(limit) = req.limit {
            view.candidates.truncate(limit);
        }
        Ok(view)
    }

    pub async fn screen_calendar_put_spreads(
        &self,
        req: CalendarPutSpreadRequest,
    ) -> Result<CalendarPutSpreadView> {
        if req.far_expiry <= req.near_expiry {
            bail!("far expiry must be later than near expiry");
        }

        let near = self
            .analysis
            .analyze_chain(
                req.near_expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol.clone(),
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Put),
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        let far = self
            .analysis
            .analyze_chain(
                req.far_expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Put),
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        let mut view = build_calendar_put_spread_view(
            near,
            far,
            req.min_open_interest,
            req.min_volume,
            req.max_net_debit_per_spread,
            req.min_net_theta_carry_per_day,
            req.min_days_gap,
            req.max_days_gap,
            req.min_strike_gap,
            req.max_strike_gap,
            None,
        )?;
        sort_calendar_put_candidates(&mut view.candidates, req.sort_by);
        if let Some(limit) = req.limit {
            view.candidates.truncate(limit);
        }
        Ok(view)
    }

    pub async fn screen_diagonal_call_spreads(
        &self,
        req: DiagonalCallSpreadRequest,
    ) -> Result<DiagonalCallSpreadView> {
        if req.far_expiry <= req.near_expiry {
            bail!("far expiry must be later than near expiry");
        }
        let near = self
            .analysis
            .analyze_chain(
                req.near_expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol.clone(),
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Call),
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;
        let far = self
            .analysis
            .analyze_chain(
                req.far_expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Call),
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;
        let mut view = build_diagonal_call_spread_view(
            near,
            far,
            req.min_open_interest,
            req.min_volume,
            req.max_net_debit_per_spread,
            req.min_net_theta_carry_per_day,
            req.min_days_gap,
            req.max_days_gap,
            req.min_strike_gap,
            req.max_strike_gap,
            None,
        )?;
        sort_diagonal_call_candidates(&mut view.candidates, req.sort_by);
        if let Some(limit) = req.limit {
            view.candidates.truncate(limit);
        }
        Ok(view)
    }

    pub async fn screen_diagonal_put_spreads(
        &self,
        req: DiagonalPutSpreadRequest,
    ) -> Result<DiagonalPutSpreadView> {
        if req.far_expiry <= req.near_expiry {
            bail!("far expiry must be later than near expiry");
        }
        let near = self
            .analysis
            .analyze_chain(
                req.near_expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol.clone(),
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Put),
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;
        let far = self
            .analysis
            .analyze_chain(
                req.far_expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        side: Some(ContractSide::Put),
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;
        let mut view = build_diagonal_put_spread_view(
            near,
            far,
            req.min_open_interest,
            req.min_volume,
            req.max_net_debit_per_spread,
            req.min_net_theta_carry_per_day,
            req.min_days_gap,
            req.max_days_gap,
            req.min_strike_gap,
            req.max_strike_gap,
            None,
        )?;
        sort_diagonal_put_candidates(&mut view.candidates, req.sort_by);
        if let Some(limit) = req.limit {
            view.candidates.truncate(limit);
        }
        Ok(view)
    }

    pub async fn screen_mispricing(&self, req: MispricingRequest) -> Result<MispricingView> {
        let analysis = self
            .analysis
            .analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol,
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: None,
                    iv_from_market_price: false,
                    screening: ChainScreeningRequest {
                        side: req.side,
                        min_strike: None,
                        max_strike: None,
                        min_delta: None,
                        max_delta: None,
                        min_theta: None,
                        max_theta: None,
                        min_vega: None,
                        max_vega: None,
                        min_iv: None,
                        max_iv: None,
                        min_option_price: None,
                        max_option_price: None,
                        min_otm_percent: None,
                        max_otm_percent: None,
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        sort_by: None,
                        limit: None,
                    },
                },
            )
            .await?;

        build_mispricing_view(
            analysis,
            req.dividend,
            req.direction,
            req.iv_direction,
            req.min_open_interest,
            req.min_volume,
            req.min_abs_mispricing_percent,
            req.min_abs_iv_diff_percent,
            req.sort_by,
            req.limit,
        )
    }

    pub async fn screen_sell_opportunities(
        &self,
        req: SellOpportunitiesRequest,
    ) -> Result<SellOpportunitiesView> {
        let (include_calendars, include_diagonals) =
            resolve_cross_expiry_inclusion(req.include_calendars, req.include_diagonals);

        let csp = self
            .screen_cash_secured_puts(CashSecuredPutRequest {
                symbol: req.symbol.clone(),
                expiry: req.expiry,
                rate: req.rate,
                dividend: req.dividend,
                iv: req.iv,
                iv_from_market_price: req.iv_from_market_price,
                direction: req.direction,
                iv_direction: req.iv_direction,
                min_delta: None,
                max_delta: None,
                min_otm_percent: None,
                max_otm_percent: None,
                min_option_price: None,
                max_option_price: None,
                min_open_interest: req.min_open_interest,
                min_volume: req.min_volume,
                min_premium_per_contract: None,
                max_cash_required_per_contract: None,
                min_annualized_return_on_cash: None,
                max_annualized_return_on_cash: None,
                min_abs_mispricing_percent: req.min_abs_mispricing_percent,
                min_abs_iv_diff_percent: req.min_abs_iv_diff_percent,
                limit: None,
            })
            .await?;

        let covered_call = self
            .screen_covered_calls(CoveredCallRequest {
                symbol: req.symbol.clone(),
                expiry: req.expiry,
                rate: req.rate,
                dividend: req.dividend,
                iv: req.iv,
                iv_from_market_price: req.iv_from_market_price,
                direction: req.direction,
                iv_direction: req.iv_direction,
                min_delta: None,
                max_delta: None,
                min_otm_percent: None,
                max_otm_percent: None,
                min_option_price: None,
                max_option_price: None,
                min_open_interest: req.min_open_interest,
                min_volume: req.min_volume,
                min_premium_per_contract: None,
                min_annualized_premium_yield: None,
                max_annualized_premium_yield: None,
                min_abs_mispricing_percent: req.min_abs_mispricing_percent,
                min_abs_iv_diff_percent: req.min_abs_iv_diff_percent,
                limit: None,
            })
            .await?;

        let bull_put_spread = self
            .screen_bull_put_spreads(BullPutSpreadRequest {
                symbol: req.symbol.clone(),
                expiry: req.expiry,
                rate: req.rate,
                dividend: req.dividend,
                iv: req.iv,
                iv_from_market_price: req.iv_from_market_price,
                direction: req.direction,
                iv_direction: req.iv_direction,
                min_short_delta: None,
                max_short_delta: None,
                min_short_otm_percent: None,
                max_short_otm_percent: None,
                min_open_interest: req.min_open_interest,
                min_volume: req.min_volume,
                min_net_credit_per_spread: None,
                min_width_per_spread: None,
                max_width_per_spread: None,
                min_annualized_return_on_risk: None,
                max_annualized_return_on_risk: None,
                min_abs_mispricing_percent: req.min_abs_mispricing_percent,
                min_abs_iv_diff_percent: req.min_abs_iv_diff_percent,
                limit: None,
            })
            .await?;

        let bear_call_spread = self
            .screen_bear_call_spreads(BearCallSpreadRequest {
                symbol: req.symbol,
                expiry: req.expiry,
                rate: req.rate,
                dividend: req.dividend,
                iv: req.iv,
                iv_from_market_price: req.iv_from_market_price,
                direction: req.direction,
                iv_direction: req.iv_direction,
                min_short_delta: None,
                max_short_delta: None,
                min_short_otm_percent: None,
                max_short_otm_percent: None,
                min_open_interest: req.min_open_interest,
                min_volume: req.min_volume,
                min_net_credit_per_spread: None,
                min_width_per_spread: None,
                max_width_per_spread: None,
                min_annualized_return_on_risk: None,
                max_annualized_return_on_risk: None,
                min_abs_mispricing_percent: req.min_abs_mispricing_percent,
                min_abs_iv_diff_percent: req.min_abs_iv_diff_percent,
                limit: None,
            })
            .await?;

        let far_expiry = self
            .analysis
            .market()
            .fetch_option_expiries(&csp.underlying_symbol)
            .await?
            .into_iter()
            .filter_map(|value| crate::market_data::parse_expiry_date(&value).ok())
            .find(|date| *date > req.expiry);

        let (calendar_call, calendar_put, diagonal_call, diagonal_put) =
            if let Some(far_expiry) = far_expiry {
                let symbol = csp.underlying_symbol.clone();
                let calendar_call = if include_calendars {
                    self.screen_calendar_call_spreads(CalendarCallSpreadRequest {
                        symbol: symbol.clone(),
                        near_expiry: req.expiry,
                        far_expiry,
                        rate: req.rate,
                        dividend: req.dividend,
                        iv: req.iv,
                        iv_from_market_price: req.iv_from_market_price,
                        min_open_interest: req.min_open_interest,
                        min_volume: req.min_volume,
                        max_net_debit_per_spread: None,
                        min_net_theta_carry_per_day: None,
                        min_days_gap: req.min_days_gap,
                        max_days_gap: req.max_days_gap,
                        min_strike_gap: req.min_strike_gap,
                        max_strike_gap: req.max_strike_gap,
                        sort_by: None,
                        limit: None,
                    })
                    .await
                    .ok()
                } else {
                    None
                };
                let calendar_put = if include_calendars {
                    self.screen_calendar_put_spreads(CalendarPutSpreadRequest {
                        symbol: symbol.clone(),
                        near_expiry: req.expiry,
                        far_expiry,
                        rate: req.rate,
                        dividend: req.dividend,
                        iv: req.iv,
                        iv_from_market_price: req.iv_from_market_price,
                        min_open_interest: req.min_open_interest,
                        min_volume: req.min_volume,
                        max_net_debit_per_spread: None,
                        min_net_theta_carry_per_day: None,
                        min_days_gap: req.min_days_gap,
                        max_days_gap: req.max_days_gap,
                        min_strike_gap: req.min_strike_gap,
                        max_strike_gap: req.max_strike_gap,
                        sort_by: None,
                        limit: None,
                    })
                    .await
                    .ok()
                } else {
                    None
                };
                let diagonal_call = if include_diagonals {
                    self.screen_diagonal_call_spreads(DiagonalCallSpreadRequest {
                        symbol: symbol.clone(),
                        near_expiry: req.expiry,
                        far_expiry,
                        rate: req.rate,
                        dividend: req.dividend,
                        iv: req.iv,
                        iv_from_market_price: req.iv_from_market_price,
                        min_open_interest: req.min_open_interest,
                        min_volume: req.min_volume,
                        max_net_debit_per_spread: None,
                        min_net_theta_carry_per_day: None,
                        min_days_gap: req.min_days_gap,
                        max_days_gap: req.max_days_gap,
                        min_strike_gap: req.min_strike_gap,
                        max_strike_gap: req.max_strike_gap,
                        sort_by: None,
                        limit: None,
                    })
                    .await
                    .ok()
                } else {
                    None
                };
                let diagonal_put = if include_diagonals {
                    self.screen_diagonal_put_spreads(DiagonalPutSpreadRequest {
                        symbol,
                        near_expiry: req.expiry,
                        far_expiry,
                        rate: req.rate,
                        dividend: req.dividend,
                        iv: req.iv,
                        iv_from_market_price: req.iv_from_market_price,
                        min_open_interest: req.min_open_interest,
                        min_volume: req.min_volume,
                        max_net_debit_per_spread: None,
                        min_net_theta_carry_per_day: None,
                        min_days_gap: req.min_days_gap,
                        max_days_gap: req.max_days_gap,
                        min_strike_gap: req.min_strike_gap,
                        max_strike_gap: req.max_strike_gap,
                        sort_by: None,
                        limit: None,
                    })
                    .await
                    .ok()
                } else {
                    None
                };
                (calendar_call, calendar_put, diagonal_call, diagonal_put)
            } else {
                (None, None, None, None)
            };

        Ok(merge_sell_opportunities(
            csp,
            covered_call,
            bull_put_spread,
            bear_call_spread,
            calendar_call,
            calendar_put,
            diagonal_call,
            diagonal_put,
            &req.strategy_filter,
            &req.exclude_strategy_filter,
            &req.return_basis_filter,
            &req.exclude_return_basis_filter,
            req.min_premium_or_credit,
            req.max_risk,
            req.min_abs_mispricing_percent,
            req.min_abs_iv_diff_percent,
            req.min_annualized_return,
            req.max_annualized_return,
            req.limit_per_strategy,
            req.sort_by,
            req.limit,
        ))
    }
}

fn resolve_cross_expiry_inclusion(
    include_calendars: Option<bool>,
    include_diagonals: Option<bool>,
) -> (bool, bool) {
    match (include_calendars, include_diagonals) {
        (None, None) => (true, true),
        (Some(true), None) => (true, false),
        (None, Some(true)) => (false, true),
        (Some(true), Some(true)) => (true, true),
        _ => (true, true),
    }
}

fn sort_sell_opportunities(
    candidates: &mut [SellOpportunityCandidate],
    sort_by: Option<SellOpportunitySortField>,
) {
    let sort_by = sort_by.unwrap_or(SellOpportunitySortField::AnnualizedReturn);
    candidates.sort_by(|a, b| {
        let primary = match sort_by {
            SellOpportunitySortField::AnnualizedReturn => sell_opportunity_annualized_sort_key(b)
                .cmp(&sell_opportunity_annualized_sort_key(a)),
            SellOpportunitySortField::Mispricing => b
                .mispricing_percent
                .abs()
                .total_cmp(&a.mispricing_percent.abs()),
            SellOpportunitySortField::IvDiff => {
                b.iv_diff_percent.abs().total_cmp(&a.iv_diff_percent.abs())
            }
        };

        primary
            .then_with(|| {
                sell_opportunity_annualized_sort_key(b)
                    .cmp(&sell_opportunity_annualized_sort_key(a))
            })
            .then_with(|| {
                b.mispricing_percent
                    .abs()
                    .total_cmp(&a.mispricing_percent.abs())
            })
            .then_with(|| b.iv_diff_percent.abs().total_cmp(&a.iv_diff_percent.abs()))
            .then_with(|| a.strategy.cmp(&b.strategy))
            .then_with(|| a.primary_symbol.cmp(&b.primary_symbol))
    });
}

fn sell_opportunity_annualized_sort_key(candidate: &SellOpportunityCandidate) -> (bool, u64) {
    (
        candidate.return_basis != SellOpportunityReturnBasis::ThetaCarryRunRate,
        candidate.annualized_return.to_bits(),
    )
}

fn matches_sell_opportunity_strategy(
    name: &str,
    strategy_filter: &[SellOpportunityStrategy],
) -> bool {
    if strategy_filter.is_empty() {
        return true;
    }

    strategy_filter
        .iter()
        .any(|strategy| sell_opportunity_strategy_name(*strategy) == name)
}

fn matches_sell_opportunity_return_basis(
    basis: SellOpportunityReturnBasis,
    return_basis_filter: &[SellOpportunityReturnBasis],
) -> bool {
    if return_basis_filter.is_empty() {
        return true;
    }
    return_basis_filter.contains(&basis)
}

fn sell_opportunity_strategy_name(strategy: SellOpportunityStrategy) -> &'static str {
    match strategy {
        SellOpportunityStrategy::CashSecuredPut => "cash_secured_put",
        SellOpportunityStrategy::CoveredCall => "covered_call",
        SellOpportunityStrategy::BullPutSpread => "bull_put_spread",
        SellOpportunityStrategy::BearCallSpread => "bear_call_spread",
        SellOpportunityStrategy::CalendarCallSpread => "calendar_call_spread",
        SellOpportunityStrategy::CalendarPutSpread => "calendar_put_spread",
        SellOpportunityStrategy::DiagonalCallSpread => "diagonal_call_spread",
        SellOpportunityStrategy::DiagonalPutSpread => "diagonal_put_spread",
    }
}

fn should_include_sell_opportunity_strategy(
    name: &str,
    strategy_filter: &[SellOpportunityStrategy],
    exclude_strategy_filter: &[SellOpportunityStrategy],
) -> bool {
    matches_sell_opportunity_strategy(name, strategy_filter)
        && !exclude_strategy_filter
            .iter()
            .any(|strategy| sell_opportunity_strategy_name(*strategy) == name)
}

fn should_include_sell_opportunity_return_basis(
    basis: SellOpportunityReturnBasis,
    return_basis_filter: &[SellOpportunityReturnBasis],
    exclude_return_basis_filter: &[SellOpportunityReturnBasis],
) -> bool {
    matches_sell_opportunity_return_basis(basis, return_basis_filter)
        && !exclude_return_basis_filter.contains(&basis)
}

fn matches_range(value: f64, min: Option<f64>, max: Option<f64>) -> bool {
    if let Some(min) = min {
        if value < min {
            return false;
        }
    }
    if let Some(max) = max {
        if value > max {
            return false;
        }
    }
    true
}

fn apply_limit_per_strategy(
    candidates: Vec<SellOpportunityCandidate>,
    limit_per_strategy: Option<usize>,
) -> Vec<SellOpportunityCandidate> {
    let Some(limit_per_strategy) = limit_per_strategy else {
        return candidates;
    };
    if limit_per_strategy == 0 {
        return Vec::new();
    }

    let mut counts = HashMap::new();
    let mut filtered = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        let count = counts.entry(candidate.strategy.clone()).or_insert(0usize);
        if *count >= limit_per_strategy {
            continue;
        }
        *count += 1;
        filtered.push(candidate);
    }

    filtered
}

fn sort_mispricing_candidates(
    candidates: &mut [MispricingCandidate],
    sort_by: Option<MispricingSortField>,
) {
    let sort_by = sort_by.unwrap_or(MispricingSortField::Mispricing);
    candidates.sort_by(|a, b| {
        let primary = match sort_by {
            MispricingSortField::Mispricing => b
                .mispricing_percent
                .abs()
                .total_cmp(&a.mispricing_percent.abs()),
            MispricingSortField::IvDiff => {
                b.iv_diff_percent.abs().total_cmp(&a.iv_diff_percent.abs())
            }
        };

        primary
            .then_with(|| {
                b.mispricing_percent
                    .abs()
                    .total_cmp(&a.mispricing_percent.abs())
            })
            .then_with(|| b.iv_diff_percent.abs().total_cmp(&a.iv_diff_percent.abs()))
            .then_with(|| a.option_symbol.cmp(&b.option_symbol))
    });
}

fn cross_expiry_sort_metric(
    annualized_theta_carry_return_on_debit: f64,
    net_theta_carry_per_day: f64,
    vega_to_debit_ratio: f64,
    net_debit_per_spread: f64,
    sort_by: CrossExpirySortField,
) -> f64 {
    match sort_by {
        CrossExpirySortField::Carry => annualized_theta_carry_return_on_debit,
        CrossExpirySortField::Theta => net_theta_carry_per_day,
        CrossExpirySortField::VegaToDebit => vega_to_debit_ratio,
        CrossExpirySortField::Debit => -net_debit_per_spread,
    }
}

fn sort_calendar_call_candidates(
    candidates: &mut [crate::domain::CalendarCallSpreadCandidate],
    sort_by: Option<CrossExpirySortField>,
) {
    let sort_by = sort_by.unwrap_or(CrossExpirySortField::Carry);
    candidates.sort_by(|a, b| {
        cross_expiry_sort_metric(
            b.annualized_theta_carry_return_on_debit,
            b.net_theta_carry_per_day,
            b.vega_to_debit_ratio,
            b.net_debit_per_spread,
            sort_by,
        )
        .total_cmp(&cross_expiry_sort_metric(
            a.annualized_theta_carry_return_on_debit,
            a.net_theta_carry_per_day,
            a.vega_to_debit_ratio,
            a.net_debit_per_spread,
            sort_by,
        ))
        .then_with(|| {
            b.annualized_theta_carry_return_on_debit
                .total_cmp(&a.annualized_theta_carry_return_on_debit)
        })
        .then_with(|| a.net_debit_per_spread.total_cmp(&b.net_debit_per_spread))
        .then_with(|| a.near_option_symbol.cmp(&b.near_option_symbol))
    });
}

fn sort_calendar_put_candidates(
    candidates: &mut [crate::domain::CalendarPutSpreadCandidate],
    sort_by: Option<CrossExpirySortField>,
) {
    let sort_by = sort_by.unwrap_or(CrossExpirySortField::Carry);
    candidates.sort_by(|a, b| {
        cross_expiry_sort_metric(
            b.annualized_theta_carry_return_on_debit,
            b.net_theta_carry_per_day,
            b.vega_to_debit_ratio,
            b.net_debit_per_spread,
            sort_by,
        )
        .total_cmp(&cross_expiry_sort_metric(
            a.annualized_theta_carry_return_on_debit,
            a.net_theta_carry_per_day,
            a.vega_to_debit_ratio,
            a.net_debit_per_spread,
            sort_by,
        ))
        .then_with(|| {
            b.annualized_theta_carry_return_on_debit
                .total_cmp(&a.annualized_theta_carry_return_on_debit)
        })
        .then_with(|| a.net_debit_per_spread.total_cmp(&b.net_debit_per_spread))
        .then_with(|| a.near_option_symbol.cmp(&b.near_option_symbol))
    });
}

fn sort_diagonal_call_candidates(
    candidates: &mut [crate::domain::DiagonalCallSpreadCandidate],
    sort_by: Option<CrossExpirySortField>,
) {
    let sort_by = sort_by.unwrap_or(CrossExpirySortField::Carry);
    candidates.sort_by(|a, b| {
        cross_expiry_sort_metric(
            b.annualized_theta_carry_return_on_debit,
            b.net_theta_carry_per_day,
            b.vega_to_debit_ratio,
            b.net_debit_per_spread,
            sort_by,
        )
        .total_cmp(&cross_expiry_sort_metric(
            a.annualized_theta_carry_return_on_debit,
            a.net_theta_carry_per_day,
            a.vega_to_debit_ratio,
            a.net_debit_per_spread,
            sort_by,
        ))
        .then_with(|| {
            b.annualized_theta_carry_return_on_debit
                .total_cmp(&a.annualized_theta_carry_return_on_debit)
        })
        .then_with(|| a.net_debit_per_spread.total_cmp(&b.net_debit_per_spread))
        .then_with(|| a.near_option_symbol.cmp(&b.near_option_symbol))
    });
}

fn sort_diagonal_put_candidates(
    candidates: &mut [crate::domain::DiagonalPutSpreadCandidate],
    sort_by: Option<CrossExpirySortField>,
) {
    let sort_by = sort_by.unwrap_or(CrossExpirySortField::Carry);
    candidates.sort_by(|a, b| {
        cross_expiry_sort_metric(
            b.annualized_theta_carry_return_on_debit,
            b.net_theta_carry_per_day,
            b.vega_to_debit_ratio,
            b.net_debit_per_spread,
            sort_by,
        )
        .total_cmp(&cross_expiry_sort_metric(
            a.annualized_theta_carry_return_on_debit,
            a.net_theta_carry_per_day,
            a.vega_to_debit_ratio,
            a.net_debit_per_spread,
            sort_by,
        ))
        .then_with(|| {
            b.annualized_theta_carry_return_on_debit
                .total_cmp(&a.annualized_theta_carry_return_on_debit)
        })
        .then_with(|| a.net_debit_per_spread.total_cmp(&b.net_debit_per_spread))
        .then_with(|| a.near_option_symbol.cmp(&b.near_option_symbol))
    });
}

fn parse_strategy_decimal(value: &str, field: &str) -> Result<f64> {
    decimal_to_f64(value, field).map_err(|err| anyhow::anyhow!("failed to parse {field}: {err}"))
}

fn build_cash_secured_put_view(
    analysis: ChainAnalysisView,
    dividend: f64,
    direction: Option<MispricingDirection>,
    iv_direction: Option<IvDiffDirection>,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    min_premium_per_contract: Option<f64>,
    max_cash_required_per_contract: Option<f64>,
    min_annualized_return_on_cash: Option<f64>,
    max_annualized_return_on_cash: Option<f64>,
    min_abs_mispricing_percent: Option<f64>,
    min_abs_iv_diff_percent: Option<f64>,
    limit: Option<usize>,
) -> Result<CashSecuredPutView> {
    let spot = parse_strategy_decimal(&analysis.underlying_price, "underlying price")?;
    let mut candidates = Vec::with_capacity(analysis.rows.len());
    for row in analysis.rows {
        let strike_price_f64 = parse_strategy_decimal(&row.strike_price, "strike price")?;
        let option_price_f64 = parse_strategy_decimal(&row.option_price, "option price")?;
        let provider_iv = parse_strategy_decimal(&row.provider_reported_iv, "provider iv")?;

        let solved_iv = implied_volatility_from_price(
            spot,
            strike_price_f64,
            analysis.rate,
            analysis.days_to_expiry as f64,
            dividend,
            row.option_type,
            option_price_f64,
        )?;
        let iv_diff = solved_iv - provider_iv;
        let iv_diff_percent = if provider_iv.abs() > 1.0e-12 {
            iv_diff / provider_iv
        } else {
            0.0
        };
        let fair_value = row.local_greeks.fair_value;
        let mispricing = option_price_f64 - fair_value;
        let mispricing_percent = if fair_value.abs() > 1.0e-12 {
            mispricing / fair_value
        } else {
            0.0
        };

        let premium_per_contract = option_price_f64 * 100.0;
        let cash_required_per_contract = strike_price_f64 * 100.0;
        let return_on_cash = if cash_required_per_contract > 0.0 {
            premium_per_contract / cash_required_per_contract
        } else {
            0.0
        };
        let annualized_return_on_cash = if analysis.days_to_expiry > 0 {
            return_on_cash * (365.0 / analysis.days_to_expiry as f64)
        } else {
            0.0
        };

        if !matches_min_count(row.open_interest, min_open_interest)
            || !matches_min_count(row.volume, min_volume)
            || !matches_min_threshold(premium_per_contract, min_premium_per_contract)
            || !matches_max_threshold(cash_required_per_contract, max_cash_required_per_contract)
            || !matches_mispricing_direction(mispricing_percent, direction)
            || !matches_iv_diff_direction(iv_diff, iv_direction)
            || !matches_min_abs_threshold(mispricing_percent, min_abs_mispricing_percent)
            || !matches_min_abs_threshold(iv_diff_percent, min_abs_iv_diff_percent)
        {
            continue;
        }

        if !matches_return_bounds(
            annualized_return_on_cash,
            min_annualized_return_on_cash,
            max_annualized_return_on_cash,
        ) {
            continue;
        }

        candidates.push(CashSecuredPutCandidate {
            option_symbol: row.option_symbol,
            strike_price: row.strike_price,
            option_price: row.option_price,
            implied_volatility: row.implied_volatility,
            provider_reported_iv: row.provider_reported_iv,
            days_to_expiry: analysis.days_to_expiry,
            delta: row.local_greeks.delta,
            theta_per_day: row.local_greeks.theta_per_day,
            otm_percent: row.diagnostics.otm_percent,
            breakeven: row.diagnostics.breakeven,
            premium_per_contract,
            cash_required_per_contract,
            return_on_cash,
            annualized_return_on_cash,
            fair_value,
            mispricing,
            mispricing_percent,
            solved_iv_from_market_price: solved_iv,
            iv_diff,
            iv_diff_percent,
            diagnostics: row.diagnostics,
        });
    }

    candidates.sort_by(|a, b| {
        b.annualized_return_on_cash
            .total_cmp(&a.annualized_return_on_cash)
            .then_with(|| b.theta_per_day.total_cmp(&a.theta_per_day))
            .then_with(|| a.option_symbol.cmp(&b.option_symbol))
    });

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(CashSecuredPutView {
        underlying_symbol: analysis.underlying_symbol,
        underlying_price: analysis.underlying_price,
        expiry: analysis.expiry,
        days_to_expiry: analysis.days_to_expiry,
        rate: analysis.rate,
        rate_source: analysis.rate_source,
        candidates,
    })
}

fn validate_return_bounds(min: Option<f64>, max: Option<f64>) -> Result<()> {
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        bail!("annualized return min must be less than or equal to max");
    }
    Ok(())
}

fn matches_return_bounds(value: f64, min: Option<f64>, max: Option<f64>) -> bool {
    if let Some(min) = min
        && value < min
    {
        return false;
    }
    if let Some(max) = max
        && value > max
    {
        return false;
    }
    true
}

fn matches_min_count(value: i64, min: Option<i64>) -> bool {
    min.is_none_or(|min| value >= min)
}

fn matches_min_threshold(value: f64, min: Option<f64>) -> bool {
    min.is_none_or(|min| value >= min)
}

fn matches_max_threshold(value: f64, max: Option<f64>) -> bool {
    max.is_none_or(|max| value <= max)
}

fn matches_min_abs_threshold(value: f64, min_abs: Option<f64>) -> bool {
    min_abs.is_none_or(|min| value.abs() >= min)
}

#[derive(Copy, Clone)]
enum CreditSpreadKind {
    BullPut,
    BearCall,
}

#[derive(Copy, Clone)]
enum DebitSpreadKind {
    BearPut,
    BullCall,
}

struct CreditSpreadCandidateCore {
    short_option_symbol: String,
    long_option_symbol: String,
    short_strike_price: String,
    long_strike_price: String,
    short_option_price: String,
    long_option_price: String,
    short_provider_reported_iv: String,
    days_to_expiry: i64,
    short_delta: f64,
    short_otm_percent: f64,
    short_fair_value: f64,
    short_mispricing: f64,
    short_mispricing_percent: f64,
    short_solved_iv_from_market_price: f64,
    short_iv_diff: f64,
    short_iv_diff_percent: f64,
    width_per_spread: f64,
    net_credit_per_spread: f64,
    max_profit_per_spread: f64,
    max_loss_per_spread: f64,
    breakeven: f64,
    return_on_risk: f64,
    annualized_return_on_risk: f64,
    short_diagnostics: crate::domain::ContractDiagnostics,
    long_diagnostics: crate::domain::ContractDiagnostics,
}

struct DebitSpreadCandidateCore {
    long_option_symbol: String,
    short_option_symbol: String,
    long_strike_price: String,
    short_strike_price: String,
    long_option_price: String,
    short_option_price: String,
    days_to_expiry: i64,
    long_delta: f64,
    long_otm_percent: f64,
    width_per_spread: f64,
    net_debit_per_spread: f64,
    max_profit_per_spread: f64,
    max_loss_per_spread: f64,
    breakeven: f64,
    return_on_risk: f64,
    annualized_return_on_risk: f64,
    long_diagnostics: crate::domain::ContractDiagnostics,
    short_diagnostics: crate::domain::ContractDiagnostics,
}

struct CalendarSpreadCandidateCore {
    near_option_symbol: String,
    far_option_symbol: String,
    near_strike_price: String,
    far_strike_price: String,
    near_expiry: String,
    far_expiry: String,
    days_gap: i64,
    strike_gap: f64,
    near_option_price: String,
    far_option_price: String,
    net_debit_per_spread: f64,
    net_theta_carry_per_day: f64,
    theta_carry_return_on_debit_per_day: f64,
    annualized_theta_carry_return_on_debit: f64,
    net_vega: f64,
    vega_to_debit_ratio: f64,
    max_loss_per_spread: f64,
    near_diagnostics: crate::domain::ContractDiagnostics,
    far_diagnostics: crate::domain::ContractDiagnostics,
}

#[derive(Copy, Clone)]
enum DualExpiryPairing {
    CalendarSameStrike,
    DiagonalCall,
    DiagonalPut,
}

fn build_mispricing_view(
    analysis: ChainAnalysisView,
    dividend: f64,
    direction: Option<MispricingDirection>,
    iv_direction: Option<IvDiffDirection>,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    min_abs_mispricing_percent: Option<f64>,
    min_abs_iv_diff_percent: Option<f64>,
    sort_by: Option<MispricingSortField>,
    limit: Option<usize>,
) -> Result<MispricingView> {
    let spot = parse_strategy_decimal(&analysis.underlying_price, "underlying price")?;

    let mut candidates = Vec::with_capacity(analysis.rows.len());
    for row in analysis.rows {
        if !matches_min_count(row.open_interest, min_open_interest)
            || !matches_min_count(row.volume, min_volume)
        {
            continue;
        }

        let strike = parse_strategy_decimal(&row.strike_price, "strike price")?;
        let option_price = parse_strategy_decimal(&row.option_price, "option price")?;
        let provider_iv = parse_strategy_decimal(&row.provider_reported_iv, "provider iv")?;

        let solved_iv = implied_volatility_from_price(
            spot,
            strike,
            analysis.rate,
            analysis.days_to_expiry as f64,
            dividend,
            row.option_type,
            option_price,
        )?;

        let iv_diff = solved_iv - provider_iv;
        let iv_diff_percent = if provider_iv.abs() > 1.0e-12 {
            iv_diff / provider_iv
        } else {
            0.0
        };

        if !matches_iv_diff_direction(iv_diff, iv_direction) {
            continue;
        }

        let fair_value = row.local_greeks.fair_value;
        let mispricing = option_price - fair_value;
        let mispricing_percent = if fair_value.abs() > 1.0e-12 {
            mispricing / fair_value
        } else {
            0.0
        };

        if !matches_mispricing_direction(mispricing_percent, direction) {
            continue;
        }

        if !matches_min_abs_threshold(mispricing_percent, min_abs_mispricing_percent)
            || !matches_min_abs_threshold(iv_diff_percent, min_abs_iv_diff_percent)
        {
            continue;
        }

        candidates.push(MispricingCandidate {
            option_symbol: row.option_symbol,
            option_type: row.option_type,
            strike_price: row.strike_price,
            option_price: row.option_price,
            provider_reported_iv: row.provider_reported_iv,
            solved_iv_from_market_price: solved_iv,
            iv_diff,
            iv_diff_percent,
            fair_value,
            mispricing,
            mispricing_percent,
            diagnostics: row.diagnostics,
            local_greeks: row.local_greeks,
        });
    }

    sort_mispricing_candidates(&mut candidates, sort_by);

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(MispricingView {
        underlying_symbol: analysis.underlying_symbol,
        underlying_price: analysis.underlying_price,
        expiry: analysis.expiry,
        days_to_expiry: analysis.days_to_expiry,
        rate: analysis.rate,
        rate_source: analysis.rate_source,
        candidates,
    })
}

fn matches_mispricing_direction(
    mispricing_percent: f64,
    direction: Option<MispricingDirection>,
) -> bool {
    match direction {
        None => true,
        Some(MispricingDirection::Overpriced) => mispricing_percent > 0.0,
        Some(MispricingDirection::Underpriced) => mispricing_percent < 0.0,
    }
}

fn matches_iv_diff_direction(iv_diff: f64, direction: Option<IvDiffDirection>) -> bool {
    match direction {
        None => true,
        Some(IvDiffDirection::Higher) => iv_diff > 0.0,
        Some(IvDiffDirection::Lower) => iv_diff < 0.0,
    }
}

fn build_credit_spread_candidates(
    mut rows: Vec<ChainAnalysisRow>,
    kind: CreditSpreadKind,
    spot: f64,
    rate: f64,
    dividend: f64,
    days_to_expiry: i64,
    direction: Option<MispricingDirection>,
    iv_direction: Option<IvDiffDirection>,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    min_net_credit_per_spread: Option<f64>,
    min_width_per_spread: Option<f64>,
    max_width_per_spread: Option<f64>,
    min_annualized_return_on_risk: Option<f64>,
    max_annualized_return_on_risk: Option<f64>,
    min_abs_mispricing_percent: Option<f64>,
    min_abs_iv_diff_percent: Option<f64>,
) -> Result<Vec<CreditSpreadCandidateCore>> {
    for row in &rows {
        parse_strategy_decimal(&row.strike_price, "spread strike price")?;
    }
    rows.sort_by(|a, b| {
        let a_strike = parse_strategy_decimal(&a.strike_price, "spread strike price")
            .expect("spread strike price validated before sorting");
        let b_strike = parse_strategy_decimal(&b.strike_price, "spread strike price")
            .expect("spread strike price validated before sorting");
        match kind {
            CreditSpreadKind::BullPut => b_strike.total_cmp(&a_strike),
            CreditSpreadKind::BearCall => a_strike.total_cmp(&b_strike),
        }
    });

    let mut candidates = Vec::new();
    for (short_index, short_row) in rows.iter().enumerate() {
        if !matches_min_count(short_row.open_interest, min_open_interest)
            || !matches_min_count(short_row.volume, min_volume)
        {
            continue;
        }

        let short_strike = parse_strategy_decimal(&short_row.strike_price, "short strike price")?;
        let short_price = parse_strategy_decimal(&short_row.option_price, "short option price")?;
        let short_provider_iv =
            parse_strategy_decimal(&short_row.provider_reported_iv, "short provider iv")?;
        let short_solved_iv = implied_volatility_from_price(
            spot,
            short_strike,
            rate,
            days_to_expiry as f64,
            dividend,
            short_row.option_type,
            short_price,
        )?;
        let short_iv_diff = short_solved_iv - short_provider_iv;
        let short_iv_diff_percent = if short_provider_iv.abs() > 1.0e-12 {
            short_iv_diff / short_provider_iv
        } else {
            0.0
        };
        let short_fair_value = short_row.local_greeks.fair_value;
        let short_mispricing = short_price - short_fair_value;
        let short_mispricing_percent = if short_fair_value.abs() > 1.0e-12 {
            short_mispricing / short_fair_value
        } else {
            0.0
        };

        if !matches_mispricing_direction(short_mispricing_percent, direction)
            || !matches_iv_diff_direction(short_iv_diff, iv_direction)
            || !matches_min_abs_threshold(short_mispricing_percent, min_abs_mispricing_percent)
            || !matches_min_abs_threshold(short_iv_diff_percent, min_abs_iv_diff_percent)
        {
            continue;
        }

        for long_row in rows.iter().skip(short_index + 1) {
            if !matches_min_count(long_row.open_interest, min_open_interest)
                || !matches_min_count(long_row.volume, min_volume)
            {
                continue;
            }

            let long_strike = parse_strategy_decimal(&long_row.strike_price, "long strike price")?;
            let long_price = parse_strategy_decimal(&long_row.option_price, "long option price")?;

            let width_per_spread = match kind {
                CreditSpreadKind::BullPut => (short_strike - long_strike) * 100.0,
                CreditSpreadKind::BearCall => (long_strike - short_strike) * 100.0,
            };
            if width_per_spread <= 0.0 {
                continue;
            }
            if !matches_min_threshold(width_per_spread, min_width_per_spread)
                || !matches_max_threshold(width_per_spread, max_width_per_spread)
            {
                continue;
            }

            let net_credit_per_spread = (short_price - long_price) * 100.0;
            if net_credit_per_spread <= 0.0
                || !matches_min_threshold(net_credit_per_spread, min_net_credit_per_spread)
            {
                continue;
            }

            let max_profit_per_spread = net_credit_per_spread;
            let max_loss_per_spread = width_per_spread - net_credit_per_spread;
            if max_loss_per_spread <= 0.0 {
                continue;
            }

            let return_on_risk = max_profit_per_spread / max_loss_per_spread;
            let annualized_return_on_risk = if days_to_expiry > 0 {
                return_on_risk * (365.0 / days_to_expiry as f64)
            } else {
                0.0
            };

            if !matches_return_bounds(
                annualized_return_on_risk,
                min_annualized_return_on_risk,
                max_annualized_return_on_risk,
            ) {
                continue;
            }

            let breakeven = match kind {
                CreditSpreadKind::BullPut => short_strike - (net_credit_per_spread / 100.0),
                CreditSpreadKind::BearCall => short_strike + (net_credit_per_spread / 100.0),
            };

            candidates.push(CreditSpreadCandidateCore {
                short_option_symbol: short_row.option_symbol.clone(),
                long_option_symbol: long_row.option_symbol.clone(),
                short_strike_price: short_row.strike_price.clone(),
                long_strike_price: long_row.strike_price.clone(),
                short_option_price: short_row.option_price.clone(),
                long_option_price: long_row.option_price.clone(),
                short_provider_reported_iv: short_row.provider_reported_iv.clone(),
                days_to_expiry,
                short_delta: short_row.local_greeks.delta,
                short_otm_percent: short_row.diagnostics.otm_percent,
                short_fair_value,
                short_mispricing,
                short_mispricing_percent,
                short_solved_iv_from_market_price: short_solved_iv,
                short_iv_diff,
                short_iv_diff_percent,
                width_per_spread,
                net_credit_per_spread,
                max_profit_per_spread,
                max_loss_per_spread,
                breakeven,
                return_on_risk,
                annualized_return_on_risk,
                short_diagnostics: short_row.diagnostics.clone(),
                long_diagnostics: long_row.diagnostics.clone(),
            });
        }
    }

    candidates.sort_by(|a, b| {
        b.annualized_return_on_risk
            .total_cmp(&a.annualized_return_on_risk)
            .then_with(|| b.net_credit_per_spread.total_cmp(&a.net_credit_per_spread))
            .then_with(|| a.short_option_symbol.cmp(&b.short_option_symbol))
    });

    Ok(candidates)
}

fn build_debit_spread_candidates(
    mut rows: Vec<ChainAnalysisRow>,
    kind: DebitSpreadKind,
    days_to_expiry: i64,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    max_net_debit_per_spread: Option<f64>,
    min_width_per_spread: Option<f64>,
    max_width_per_spread: Option<f64>,
    min_annualized_return_on_risk: Option<f64>,
    max_annualized_return_on_risk: Option<f64>,
) -> Result<Vec<DebitSpreadCandidateCore>> {
    for row in &rows {
        parse_strategy_decimal(&row.strike_price, "spread strike price")?;
    }
    rows.sort_by(|a, b| {
        let a_strike = parse_strategy_decimal(&a.strike_price, "spread strike price")
            .expect("spread strike price validated before sorting");
        let b_strike = parse_strategy_decimal(&b.strike_price, "spread strike price")
            .expect("spread strike price validated before sorting");
        match kind {
            DebitSpreadKind::BearPut => b_strike.total_cmp(&a_strike),
            DebitSpreadKind::BullCall => a_strike.total_cmp(&b_strike),
        }
    });

    let mut candidates = Vec::new();
    for (long_index, long_row) in rows.iter().enumerate() {
        if !matches_min_count(long_row.open_interest, min_open_interest)
            || !matches_min_count(long_row.volume, min_volume)
        {
            continue;
        }

        let long_strike = parse_strategy_decimal(&long_row.strike_price, "long strike price")?;
        let long_price = parse_strategy_decimal(&long_row.option_price, "long option price")?;

        for short_row in rows.iter().skip(long_index + 1) {
            if !matches_min_count(short_row.open_interest, min_open_interest)
                || !matches_min_count(short_row.volume, min_volume)
            {
                continue;
            }

            let short_strike =
                parse_strategy_decimal(&short_row.strike_price, "short strike price")?;
            let short_price =
                parse_strategy_decimal(&short_row.option_price, "short option price")?;

            let width_per_spread = match kind {
                DebitSpreadKind::BearPut => (long_strike - short_strike) * 100.0,
                DebitSpreadKind::BullCall => (short_strike - long_strike) * 100.0,
            };
            if width_per_spread <= 0.0 {
                continue;
            }
            if !matches_min_threshold(width_per_spread, min_width_per_spread)
                || !matches_max_threshold(width_per_spread, max_width_per_spread)
            {
                continue;
            }

            let net_debit_per_spread = (long_price - short_price) * 100.0;
            if net_debit_per_spread <= 0.0
                || !matches_max_threshold(net_debit_per_spread, max_net_debit_per_spread)
            {
                continue;
            }

            let max_loss_per_spread = net_debit_per_spread;
            let max_profit_per_spread = width_per_spread - net_debit_per_spread;
            if max_profit_per_spread <= 0.0 {
                continue;
            }

            let return_on_risk = max_profit_per_spread / max_loss_per_spread;
            let annualized_return_on_risk = if days_to_expiry > 0 {
                return_on_risk * (365.0 / days_to_expiry as f64)
            } else {
                0.0
            };

            if !matches_return_bounds(
                annualized_return_on_risk,
                min_annualized_return_on_risk,
                max_annualized_return_on_risk,
            ) {
                continue;
            }

            let breakeven = match kind {
                DebitSpreadKind::BearPut => long_strike - (net_debit_per_spread / 100.0),
                DebitSpreadKind::BullCall => long_strike + (net_debit_per_spread / 100.0),
            };

            candidates.push(DebitSpreadCandidateCore {
                long_option_symbol: long_row.option_symbol.clone(),
                short_option_symbol: short_row.option_symbol.clone(),
                long_strike_price: long_row.strike_price.clone(),
                short_strike_price: short_row.strike_price.clone(),
                long_option_price: long_row.option_price.clone(),
                short_option_price: short_row.option_price.clone(),
                days_to_expiry,
                long_delta: long_row.local_greeks.delta,
                long_otm_percent: long_row.diagnostics.otm_percent,
                width_per_spread,
                net_debit_per_spread,
                max_profit_per_spread,
                max_loss_per_spread,
                breakeven,
                return_on_risk,
                annualized_return_on_risk,
                long_diagnostics: long_row.diagnostics.clone(),
                short_diagnostics: short_row.diagnostics.clone(),
            });
        }
    }

    candidates.sort_by(|a, b| {
        b.annualized_return_on_risk
            .total_cmp(&a.annualized_return_on_risk)
            .then_with(|| a.net_debit_per_spread.total_cmp(&b.net_debit_per_spread))
            .then_with(|| a.long_option_symbol.cmp(&b.long_option_symbol))
    });

    Ok(candidates)
}

fn build_calendar_spread_candidates(
    near: ChainAnalysisView,
    far: ChainAnalysisView,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    max_net_debit_per_spread: Option<f64>,
    min_net_theta_carry_per_day: Option<f64>,
    min_days_gap: Option<i64>,
    max_days_gap: Option<i64>,
    min_strike_gap: Option<f64>,
    max_strike_gap: Option<f64>,
    pairing: DualExpiryPairing,
) -> Result<(
    ChainAnalysisView,
    ChainAnalysisView,
    Vec<CalendarSpreadCandidateCore>,
)> {
    let far_rows = far.rows;
    let days_gap = far.days_to_expiry - near.days_to_expiry;

    let mut candidates = Vec::new();
    let near_meta = ChainAnalysisView {
        underlying_symbol: near.underlying_symbol.clone(),
        underlying_price: near.underlying_price.clone(),
        expiry: near.expiry.clone(),
        days_to_expiry: near.days_to_expiry,
        rate: near.rate,
        rate_source: near.rate_source.clone(),
        rows: Vec::new(),
    };
    let far_meta = ChainAnalysisView {
        underlying_symbol: far.underlying_symbol.clone(),
        underlying_price: far.underlying_price.clone(),
        expiry: far.expiry.clone(),
        days_to_expiry: far.days_to_expiry,
        rate: far.rate,
        rate_source: far.rate_source.clone(),
        rows: Vec::new(),
    };

    if !matches_min_count(days_gap, min_days_gap) || max_days_gap.is_some_and(|max| days_gap > max)
    {
        return Ok((near_meta, far_meta, candidates));
    }

    for near_row in near.rows {
        if !matches_min_count(near_row.open_interest, min_open_interest)
            || !matches_min_count(near_row.volume, min_volume)
        {
            continue;
        }

        let near_strike = parse_strategy_decimal(&near_row.strike_price, "near strike price")?;

        for far_row in &far_rows {
            if !matches_min_count(far_row.open_interest, min_open_interest)
                || !matches_min_count(far_row.volume, min_volume)
            {
                continue;
            }

            let far_strike = parse_strategy_decimal(&far_row.strike_price, "far strike price")?;

            let strike_matches = match pairing {
                DualExpiryPairing::CalendarSameStrike => (far_strike - near_strike).abs() < 1.0e-9,
                DualExpiryPairing::DiagonalCall => far_strike < near_strike,
                DualExpiryPairing::DiagonalPut => far_strike > near_strike,
            };
            if !strike_matches {
                continue;
            }
            let strike_gap = (far_strike - near_strike).abs();
            if !matches_min_threshold(strike_gap, min_strike_gap)
                || !matches_max_threshold(strike_gap, max_strike_gap)
            {
                continue;
            }

            let near_price = parse_strategy_decimal(&near_row.option_price, "near option price")?;
            let far_price = parse_strategy_decimal(&far_row.option_price, "far option price")?;
            let net_debit_per_spread = (far_price - near_price) * 100.0;
            if net_debit_per_spread <= 0.0
                || !matches_max_threshold(net_debit_per_spread, max_net_debit_per_spread)
            {
                continue;
            }

            let net_theta_carry_per_day =
                far_row.local_greeks.theta_per_day - near_row.local_greeks.theta_per_day;
            if !matches_min_threshold(net_theta_carry_per_day, min_net_theta_carry_per_day) {
                continue;
            }

            let net_vega = far_row.local_greeks.vega - near_row.local_greeks.vega;
            let theta_carry_return_on_debit_per_day = if net_debit_per_spread.abs() > 1.0e-12 {
                (net_theta_carry_per_day * 100.0) / net_debit_per_spread
            } else {
                0.0
            };
            let annualized_theta_carry_return_on_debit =
                theta_carry_return_on_debit_per_day * 365.0;
            let vega_to_debit_ratio = if net_debit_per_spread.abs() > 1.0e-12 {
                (net_vega * 100.0) / net_debit_per_spread
            } else {
                0.0
            };

            candidates.push(CalendarSpreadCandidateCore {
                near_option_symbol: near_row.option_symbol.clone(),
                far_option_symbol: far_row.option_symbol.clone(),
                near_strike_price: near_row.strike_price.clone(),
                far_strike_price: far_row.strike_price.clone(),
                near_expiry: near_meta.expiry.clone(),
                far_expiry: far_meta.expiry.clone(),
                days_gap,
                strike_gap,
                near_option_price: near_row.option_price.clone(),
                far_option_price: far_row.option_price.clone(),
                net_debit_per_spread,
                net_theta_carry_per_day,
                theta_carry_return_on_debit_per_day,
                annualized_theta_carry_return_on_debit,
                net_vega,
                vega_to_debit_ratio,
                max_loss_per_spread: net_debit_per_spread,
                near_diagnostics: near_row.diagnostics.clone(),
                far_diagnostics: far_row.diagnostics.clone(),
            });
        }
    }

    candidates.sort_by(|a, b| {
        b.annualized_theta_carry_return_on_debit
            .total_cmp(&a.annualized_theta_carry_return_on_debit)
            .then_with(|| {
                b.net_theta_carry_per_day
                    .total_cmp(&a.net_theta_carry_per_day)
            })
            .then_with(|| a.net_debit_per_spread.total_cmp(&b.net_debit_per_spread))
            .then_with(|| a.near_strike_price.cmp(&b.near_strike_price))
            .then_with(|| a.far_strike_price.cmp(&b.far_strike_price))
    });

    Ok((near_meta, far_meta, candidates))
}

fn build_covered_call_view(
    analysis: ChainAnalysisView,
    dividend: f64,
    direction: Option<MispricingDirection>,
    iv_direction: Option<IvDiffDirection>,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    min_premium_per_contract: Option<f64>,
    min_annualized_premium_yield: Option<f64>,
    max_annualized_premium_yield: Option<f64>,
    min_abs_mispricing_percent: Option<f64>,
    min_abs_iv_diff_percent: Option<f64>,
    limit: Option<usize>,
) -> Result<CoveredCallView> {
    let underlying_price_f64 =
        parse_strategy_decimal(&analysis.underlying_price, "underlying price")?;

    let mut candidates = Vec::with_capacity(analysis.rows.len());
    for row in analysis.rows {
        let strike_price_f64 = parse_strategy_decimal(&row.strike_price, "strike price")?;
        let option_price_f64 = parse_strategy_decimal(&row.option_price, "option price")?;
        let provider_iv = parse_strategy_decimal(&row.provider_reported_iv, "provider iv")?;

        let solved_iv = implied_volatility_from_price(
            underlying_price_f64,
            strike_price_f64,
            analysis.rate,
            analysis.days_to_expiry as f64,
            dividend,
            row.option_type,
            option_price_f64,
        )?;
        let iv_diff = solved_iv - provider_iv;
        let iv_diff_percent = if provider_iv.abs() > 1.0e-12 {
            iv_diff / provider_iv
        } else {
            0.0
        };
        let fair_value = row.local_greeks.fair_value;
        let mispricing = option_price_f64 - fair_value;
        let mispricing_percent = if fair_value.abs() > 1.0e-12 {
            mispricing / fair_value
        } else {
            0.0
        };

        let premium_per_contract = option_price_f64 * 100.0;
        let premium_yield_on_underlying = if underlying_price_f64 > 0.0 {
            option_price_f64 / underlying_price_f64
        } else {
            0.0
        };
        let annualized_premium_yield = if analysis.days_to_expiry > 0 {
            premium_yield_on_underlying * (365.0 / analysis.days_to_expiry as f64)
        } else {
            0.0
        };

        if !matches_min_count(row.open_interest, min_open_interest)
            || !matches_min_count(row.volume, min_volume)
            || !matches_min_threshold(premium_per_contract, min_premium_per_contract)
            || !matches_mispricing_direction(mispricing_percent, direction)
            || !matches_iv_diff_direction(iv_diff, iv_direction)
            || !matches_min_abs_threshold(mispricing_percent, min_abs_mispricing_percent)
            || !matches_min_abs_threshold(iv_diff_percent, min_abs_iv_diff_percent)
        {
            continue;
        }

        if !matches_return_bounds(
            annualized_premium_yield,
            min_annualized_premium_yield,
            max_annualized_premium_yield,
        ) {
            continue;
        }

        let max_sale_value_per_contract = strike_price_f64 * 100.0;
        let covered_call_breakeven = underlying_price_f64 - option_price_f64;
        let max_profit_per_contract =
            ((strike_price_f64 - underlying_price_f64).max(0.0) * 100.0) + premium_per_contract;
        let max_loss_per_contract = (underlying_price_f64 * 100.0 - premium_per_contract).max(0.0);

        candidates.push(CoveredCallCandidate {
            option_symbol: row.option_symbol,
            strike_price: row.strike_price,
            option_price: row.option_price,
            implied_volatility: row.implied_volatility,
            provider_reported_iv: row.provider_reported_iv,
            days_to_expiry: analysis.days_to_expiry,
            delta: row.local_greeks.delta,
            theta_per_day: row.local_greeks.theta_per_day,
            otm_percent: row.diagnostics.otm_percent,
            breakeven: covered_call_breakeven,
            premium_per_contract,
            premium_yield_on_underlying,
            annualized_premium_yield,
            max_sale_value_per_contract,
            max_profit_per_contract,
            max_loss_per_contract,
            fair_value,
            mispricing,
            mispricing_percent,
            solved_iv_from_market_price: solved_iv,
            iv_diff,
            iv_diff_percent,
            diagnostics: row.diagnostics,
        });
    }

    candidates.sort_by(|a, b| {
        b.annualized_premium_yield
            .total_cmp(&a.annualized_premium_yield)
            .then_with(|| b.theta_per_day.total_cmp(&a.theta_per_day))
            .then_with(|| a.option_symbol.cmp(&b.option_symbol))
    });

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(CoveredCallView {
        underlying_symbol: analysis.underlying_symbol,
        underlying_price: analysis.underlying_price,
        expiry: analysis.expiry,
        days_to_expiry: analysis.days_to_expiry,
        rate: analysis.rate,
        rate_source: analysis.rate_source,
        candidates,
    })
}

fn build_bull_put_spread_view(
    analysis: ChainAnalysisView,
    dividend: f64,
    direction: Option<MispricingDirection>,
    iv_direction: Option<IvDiffDirection>,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    min_net_credit_per_spread: Option<f64>,
    min_width_per_spread: Option<f64>,
    max_width_per_spread: Option<f64>,
    min_annualized_return_on_risk: Option<f64>,
    max_annualized_return_on_risk: Option<f64>,
    min_abs_mispricing_percent: Option<f64>,
    min_abs_iv_diff_percent: Option<f64>,
    limit: Option<usize>,
) -> Result<BullPutSpreadView> {
    let spot = parse_strategy_decimal(&analysis.underlying_price, "underlying price")?;
    let mut candidates: Vec<BullPutSpreadCandidate> = build_credit_spread_candidates(
        analysis.rows,
        CreditSpreadKind::BullPut,
        spot,
        analysis.rate,
        dividend,
        analysis.days_to_expiry,
        direction,
        iv_direction,
        min_open_interest,
        min_volume,
        min_net_credit_per_spread,
        min_width_per_spread,
        max_width_per_spread,
        min_annualized_return_on_risk,
        max_annualized_return_on_risk,
        min_abs_mispricing_percent,
        min_abs_iv_diff_percent,
    )?
    .into_iter()
    .map(|candidate| BullPutSpreadCandidate {
        short_option_symbol: candidate.short_option_symbol,
        long_option_symbol: candidate.long_option_symbol,
        short_strike_price: candidate.short_strike_price,
        long_strike_price: candidate.long_strike_price,
        short_option_price: candidate.short_option_price,
        long_option_price: candidate.long_option_price,
        short_provider_reported_iv: candidate.short_provider_reported_iv,
        days_to_expiry: candidate.days_to_expiry,
        short_delta: candidate.short_delta,
        short_otm_percent: candidate.short_otm_percent,
        short_fair_value: candidate.short_fair_value,
        short_mispricing: candidate.short_mispricing,
        short_mispricing_percent: candidate.short_mispricing_percent,
        short_solved_iv_from_market_price: candidate.short_solved_iv_from_market_price,
        short_iv_diff: candidate.short_iv_diff,
        short_iv_diff_percent: candidate.short_iv_diff_percent,
        width_per_spread: candidate.width_per_spread,
        net_credit_per_spread: candidate.net_credit_per_spread,
        max_profit_per_spread: candidate.max_profit_per_spread,
        max_loss_per_spread: candidate.max_loss_per_spread,
        breakeven: candidate.breakeven,
        return_on_risk: candidate.return_on_risk,
        annualized_return_on_risk: candidate.annualized_return_on_risk,
        short_diagnostics: candidate.short_diagnostics,
        long_diagnostics: candidate.long_diagnostics,
    })
    .collect();

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(BullPutSpreadView {
        underlying_symbol: analysis.underlying_symbol,
        underlying_price: analysis.underlying_price,
        expiry: analysis.expiry,
        days_to_expiry: analysis.days_to_expiry,
        rate: analysis.rate,
        rate_source: analysis.rate_source,
        candidates,
    })
}

fn build_bear_call_spread_view(
    analysis: ChainAnalysisView,
    dividend: f64,
    direction: Option<MispricingDirection>,
    iv_direction: Option<IvDiffDirection>,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    min_net_credit_per_spread: Option<f64>,
    min_width_per_spread: Option<f64>,
    max_width_per_spread: Option<f64>,
    min_annualized_return_on_risk: Option<f64>,
    max_annualized_return_on_risk: Option<f64>,
    min_abs_mispricing_percent: Option<f64>,
    min_abs_iv_diff_percent: Option<f64>,
    limit: Option<usize>,
) -> Result<BearCallSpreadView> {
    let spot = parse_strategy_decimal(&analysis.underlying_price, "underlying price")?;
    let mut candidates: Vec<BearCallSpreadCandidate> = build_credit_spread_candidates(
        analysis.rows,
        CreditSpreadKind::BearCall,
        spot,
        analysis.rate,
        dividend,
        analysis.days_to_expiry,
        direction,
        iv_direction,
        min_open_interest,
        min_volume,
        min_net_credit_per_spread,
        min_width_per_spread,
        max_width_per_spread,
        min_annualized_return_on_risk,
        max_annualized_return_on_risk,
        min_abs_mispricing_percent,
        min_abs_iv_diff_percent,
    )?
    .into_iter()
    .map(|candidate| BearCallSpreadCandidate {
        short_option_symbol: candidate.short_option_symbol,
        long_option_symbol: candidate.long_option_symbol,
        short_strike_price: candidate.short_strike_price,
        long_strike_price: candidate.long_strike_price,
        short_option_price: candidate.short_option_price,
        long_option_price: candidate.long_option_price,
        short_provider_reported_iv: candidate.short_provider_reported_iv,
        days_to_expiry: candidate.days_to_expiry,
        short_delta: candidate.short_delta,
        short_otm_percent: candidate.short_otm_percent,
        short_fair_value: candidate.short_fair_value,
        short_mispricing: candidate.short_mispricing,
        short_mispricing_percent: candidate.short_mispricing_percent,
        short_solved_iv_from_market_price: candidate.short_solved_iv_from_market_price,
        short_iv_diff: candidate.short_iv_diff,
        short_iv_diff_percent: candidate.short_iv_diff_percent,
        width_per_spread: candidate.width_per_spread,
        net_credit_per_spread: candidate.net_credit_per_spread,
        max_profit_per_spread: candidate.max_profit_per_spread,
        max_loss_per_spread: candidate.max_loss_per_spread,
        breakeven: candidate.breakeven,
        return_on_risk: candidate.return_on_risk,
        annualized_return_on_risk: candidate.annualized_return_on_risk,
        short_diagnostics: candidate.short_diagnostics,
        long_diagnostics: candidate.long_diagnostics,
    })
    .collect();

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(BearCallSpreadView {
        underlying_symbol: analysis.underlying_symbol,
        underlying_price: analysis.underlying_price,
        expiry: analysis.expiry,
        days_to_expiry: analysis.days_to_expiry,
        rate: analysis.rate,
        rate_source: analysis.rate_source,
        candidates,
    })
}

fn build_bear_put_spread_view(
    analysis: ChainAnalysisView,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    max_net_debit_per_spread: Option<f64>,
    min_width_per_spread: Option<f64>,
    max_width_per_spread: Option<f64>,
    min_annualized_return_on_risk: Option<f64>,
    max_annualized_return_on_risk: Option<f64>,
    limit: Option<usize>,
) -> Result<BearPutSpreadView> {
    let mut candidates: Vec<BearPutSpreadCandidate> = build_debit_spread_candidates(
        analysis.rows,
        DebitSpreadKind::BearPut,
        analysis.days_to_expiry,
        min_open_interest,
        min_volume,
        max_net_debit_per_spread,
        min_width_per_spread,
        max_width_per_spread,
        min_annualized_return_on_risk,
        max_annualized_return_on_risk,
    )?
    .into_iter()
    .map(|candidate| BearPutSpreadCandidate {
        long_option_symbol: candidate.long_option_symbol,
        short_option_symbol: candidate.short_option_symbol,
        long_strike_price: candidate.long_strike_price,
        short_strike_price: candidate.short_strike_price,
        long_option_price: candidate.long_option_price,
        short_option_price: candidate.short_option_price,
        days_to_expiry: candidate.days_to_expiry,
        long_delta: candidate.long_delta,
        long_otm_percent: candidate.long_otm_percent,
        width_per_spread: candidate.width_per_spread,
        net_debit_per_spread: candidate.net_debit_per_spread,
        max_profit_per_spread: candidate.max_profit_per_spread,
        max_loss_per_spread: candidate.max_loss_per_spread,
        breakeven: candidate.breakeven,
        return_on_risk: candidate.return_on_risk,
        annualized_return_on_risk: candidate.annualized_return_on_risk,
        long_diagnostics: candidate.long_diagnostics,
        short_diagnostics: candidate.short_diagnostics,
    })
    .collect();

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(BearPutSpreadView {
        underlying_symbol: analysis.underlying_symbol,
        underlying_price: analysis.underlying_price,
        expiry: analysis.expiry,
        days_to_expiry: analysis.days_to_expiry,
        rate: analysis.rate,
        rate_source: analysis.rate_source,
        candidates,
    })
}

fn build_bull_call_spread_view(
    analysis: ChainAnalysisView,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    max_net_debit_per_spread: Option<f64>,
    min_width_per_spread: Option<f64>,
    max_width_per_spread: Option<f64>,
    min_annualized_return_on_risk: Option<f64>,
    max_annualized_return_on_risk: Option<f64>,
    limit: Option<usize>,
) -> Result<BullCallSpreadView> {
    let mut candidates: Vec<BullCallSpreadCandidate> = build_debit_spread_candidates(
        analysis.rows,
        DebitSpreadKind::BullCall,
        analysis.days_to_expiry,
        min_open_interest,
        min_volume,
        max_net_debit_per_spread,
        min_width_per_spread,
        max_width_per_spread,
        min_annualized_return_on_risk,
        max_annualized_return_on_risk,
    )?
    .into_iter()
    .map(|candidate| BullCallSpreadCandidate {
        long_option_symbol: candidate.long_option_symbol,
        short_option_symbol: candidate.short_option_symbol,
        long_strike_price: candidate.long_strike_price,
        short_strike_price: candidate.short_strike_price,
        long_option_price: candidate.long_option_price,
        short_option_price: candidate.short_option_price,
        days_to_expiry: candidate.days_to_expiry,
        long_delta: candidate.long_delta,
        long_otm_percent: candidate.long_otm_percent,
        width_per_spread: candidate.width_per_spread,
        net_debit_per_spread: candidate.net_debit_per_spread,
        max_profit_per_spread: candidate.max_profit_per_spread,
        max_loss_per_spread: candidate.max_loss_per_spread,
        breakeven: candidate.breakeven,
        return_on_risk: candidate.return_on_risk,
        annualized_return_on_risk: candidate.annualized_return_on_risk,
        long_diagnostics: candidate.long_diagnostics,
        short_diagnostics: candidate.short_diagnostics,
    })
    .collect();

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(BullCallSpreadView {
        underlying_symbol: analysis.underlying_symbol,
        underlying_price: analysis.underlying_price,
        expiry: analysis.expiry,
        days_to_expiry: analysis.days_to_expiry,
        rate: analysis.rate,
        rate_source: analysis.rate_source,
        candidates,
    })
}

fn build_calendar_call_spread_view(
    near: ChainAnalysisView,
    far: ChainAnalysisView,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    max_net_debit_per_spread: Option<f64>,
    min_net_theta_carry_per_day: Option<f64>,
    min_days_gap: Option<i64>,
    max_days_gap: Option<i64>,
    min_strike_gap: Option<f64>,
    max_strike_gap: Option<f64>,
    limit: Option<usize>,
) -> Result<CalendarCallSpreadView> {
    let (near_meta, far_meta, mut candidates): (
        ChainAnalysisView,
        ChainAnalysisView,
        Vec<CalendarCallSpreadCandidate>,
    ) = {
        let (near_meta, far_meta, candidates) = build_calendar_spread_candidates(
            near,
            far,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_net_theta_carry_per_day,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            DualExpiryPairing::CalendarSameStrike,
        )?;
        let mapped = candidates
            .into_iter()
            .map(|candidate| CalendarCallSpreadCandidate {
                near_option_symbol: candidate.near_option_symbol,
                far_option_symbol: candidate.far_option_symbol,
                strike_price: candidate.near_strike_price,
                near_expiry: candidate.near_expiry,
                far_expiry: candidate.far_expiry,
                days_gap: candidate.days_gap,
                strike_gap: candidate.strike_gap,
                near_option_price: candidate.near_option_price,
                far_option_price: candidate.far_option_price,
                net_debit_per_spread: candidate.net_debit_per_spread,
                net_theta_carry_per_day: candidate.net_theta_carry_per_day,
                theta_carry_return_on_debit_per_day: candidate.theta_carry_return_on_debit_per_day,
                annualized_theta_carry_return_on_debit: candidate
                    .annualized_theta_carry_return_on_debit,
                net_vega: candidate.net_vega,
                vega_to_debit_ratio: candidate.vega_to_debit_ratio,
                max_loss_per_spread: candidate.max_loss_per_spread,
                near_diagnostics: candidate.near_diagnostics,
                far_diagnostics: candidate.far_diagnostics,
            })
            .collect();
        (near_meta, far_meta, mapped)
    };

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(CalendarCallSpreadView {
        underlying_symbol: near_meta.underlying_symbol,
        underlying_price: near_meta.underlying_price,
        near_expiry: near_meta.expiry,
        far_expiry: far_meta.expiry,
        rate: near_meta.rate,
        rate_source: near_meta.rate_source,
        candidates,
    })
}

fn build_calendar_put_spread_view(
    near: ChainAnalysisView,
    far: ChainAnalysisView,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    max_net_debit_per_spread: Option<f64>,
    min_net_theta_carry_per_day: Option<f64>,
    min_days_gap: Option<i64>,
    max_days_gap: Option<i64>,
    min_strike_gap: Option<f64>,
    max_strike_gap: Option<f64>,
    limit: Option<usize>,
) -> Result<CalendarPutSpreadView> {
    let (near_meta, far_meta, mut candidates): (
        ChainAnalysisView,
        ChainAnalysisView,
        Vec<CalendarPutSpreadCandidate>,
    ) = {
        let (near_meta, far_meta, candidates) = build_calendar_spread_candidates(
            near,
            far,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_net_theta_carry_per_day,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            DualExpiryPairing::CalendarSameStrike,
        )?;
        let mapped = candidates
            .into_iter()
            .map(|candidate| CalendarPutSpreadCandidate {
                near_option_symbol: candidate.near_option_symbol,
                far_option_symbol: candidate.far_option_symbol,
                strike_price: candidate.near_strike_price,
                near_expiry: candidate.near_expiry,
                far_expiry: candidate.far_expiry,
                days_gap: candidate.days_gap,
                strike_gap: candidate.strike_gap,
                near_option_price: candidate.near_option_price,
                far_option_price: candidate.far_option_price,
                net_debit_per_spread: candidate.net_debit_per_spread,
                net_theta_carry_per_day: candidate.net_theta_carry_per_day,
                theta_carry_return_on_debit_per_day: candidate.theta_carry_return_on_debit_per_day,
                annualized_theta_carry_return_on_debit: candidate
                    .annualized_theta_carry_return_on_debit,
                net_vega: candidate.net_vega,
                vega_to_debit_ratio: candidate.vega_to_debit_ratio,
                max_loss_per_spread: candidate.max_loss_per_spread,
                near_diagnostics: candidate.near_diagnostics,
                far_diagnostics: candidate.far_diagnostics,
            })
            .collect();
        (near_meta, far_meta, mapped)
    };

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(CalendarPutSpreadView {
        underlying_symbol: near_meta.underlying_symbol,
        underlying_price: near_meta.underlying_price,
        near_expiry: near_meta.expiry,
        far_expiry: far_meta.expiry,
        rate: near_meta.rate,
        rate_source: near_meta.rate_source,
        candidates,
    })
}

fn build_diagonal_call_spread_view(
    near: ChainAnalysisView,
    far: ChainAnalysisView,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    max_net_debit_per_spread: Option<f64>,
    min_net_theta_carry_per_day: Option<f64>,
    min_days_gap: Option<i64>,
    max_days_gap: Option<i64>,
    min_strike_gap: Option<f64>,
    max_strike_gap: Option<f64>,
    limit: Option<usize>,
) -> Result<DiagonalCallSpreadView> {
    let (near_meta, far_meta, mut candidates): (
        ChainAnalysisView,
        ChainAnalysisView,
        Vec<DiagonalCallSpreadCandidate>,
    ) = {
        let (near_meta, far_meta, candidates) = build_calendar_spread_candidates(
            near,
            far,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_net_theta_carry_per_day,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            DualExpiryPairing::DiagonalCall,
        )?;
        let mapped = candidates
            .into_iter()
            .map(|candidate| DiagonalCallSpreadCandidate {
                near_option_symbol: candidate.near_option_symbol,
                far_option_symbol: candidate.far_option_symbol,
                near_strike_price: candidate.near_strike_price,
                far_strike_price: candidate.far_strike_price,
                near_expiry: candidate.near_expiry,
                far_expiry: candidate.far_expiry,
                days_gap: candidate.days_gap,
                strike_gap: candidate.strike_gap,
                near_option_price: candidate.near_option_price,
                far_option_price: candidate.far_option_price,
                net_debit_per_spread: candidate.net_debit_per_spread,
                net_theta_carry_per_day: candidate.net_theta_carry_per_day,
                theta_carry_return_on_debit_per_day: candidate.theta_carry_return_on_debit_per_day,
                annualized_theta_carry_return_on_debit: candidate
                    .annualized_theta_carry_return_on_debit,
                net_vega: candidate.net_vega,
                vega_to_debit_ratio: candidate.vega_to_debit_ratio,
                max_loss_per_spread: candidate.max_loss_per_spread,
                near_diagnostics: candidate.near_diagnostics,
                far_diagnostics: candidate.far_diagnostics,
            })
            .collect();
        (near_meta, far_meta, mapped)
    };

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(DiagonalCallSpreadView {
        underlying_symbol: near_meta.underlying_symbol,
        underlying_price: near_meta.underlying_price,
        near_expiry: near_meta.expiry,
        far_expiry: far_meta.expiry,
        rate: near_meta.rate,
        rate_source: near_meta.rate_source,
        candidates,
    })
}

fn build_diagonal_put_spread_view(
    near: ChainAnalysisView,
    far: ChainAnalysisView,
    min_open_interest: Option<i64>,
    min_volume: Option<i64>,
    max_net_debit_per_spread: Option<f64>,
    min_net_theta_carry_per_day: Option<f64>,
    min_days_gap: Option<i64>,
    max_days_gap: Option<i64>,
    min_strike_gap: Option<f64>,
    max_strike_gap: Option<f64>,
    limit: Option<usize>,
) -> Result<DiagonalPutSpreadView> {
    let (near_meta, far_meta, mut candidates): (
        ChainAnalysisView,
        ChainAnalysisView,
        Vec<DiagonalPutSpreadCandidate>,
    ) = {
        let (near_meta, far_meta, candidates) = build_calendar_spread_candidates(
            near,
            far,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_net_theta_carry_per_day,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            DualExpiryPairing::DiagonalPut,
        )?;
        let mapped = candidates
            .into_iter()
            .map(|candidate| DiagonalPutSpreadCandidate {
                near_option_symbol: candidate.near_option_symbol,
                far_option_symbol: candidate.far_option_symbol,
                near_strike_price: candidate.near_strike_price,
                far_strike_price: candidate.far_strike_price,
                near_expiry: candidate.near_expiry,
                far_expiry: candidate.far_expiry,
                days_gap: candidate.days_gap,
                strike_gap: candidate.strike_gap,
                near_option_price: candidate.near_option_price,
                far_option_price: candidate.far_option_price,
                net_debit_per_spread: candidate.net_debit_per_spread,
                net_theta_carry_per_day: candidate.net_theta_carry_per_day,
                theta_carry_return_on_debit_per_day: candidate.theta_carry_return_on_debit_per_day,
                annualized_theta_carry_return_on_debit: candidate
                    .annualized_theta_carry_return_on_debit,
                net_vega: candidate.net_vega,
                vega_to_debit_ratio: candidate.vega_to_debit_ratio,
                max_loss_per_spread: candidate.max_loss_per_spread,
                near_diagnostics: candidate.near_diagnostics,
                far_diagnostics: candidate.far_diagnostics,
            })
            .collect();
        (near_meta, far_meta, mapped)
    };

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    Ok(DiagonalPutSpreadView {
        underlying_symbol: near_meta.underlying_symbol,
        underlying_price: near_meta.underlying_price,
        near_expiry: near_meta.expiry,
        far_expiry: far_meta.expiry,
        rate: near_meta.rate,
        rate_source: near_meta.rate_source,
        candidates,
    })
}

fn merge_sell_opportunities(
    cash_secured_puts: CashSecuredPutView,
    covered_calls: CoveredCallView,
    bull_put_spreads: BullPutSpreadView,
    bear_call_spreads: BearCallSpreadView,
    calendar_call_spreads: Option<CalendarCallSpreadView>,
    calendar_put_spreads: Option<CalendarPutSpreadView>,
    diagonal_call_spreads: Option<DiagonalCallSpreadView>,
    diagonal_put_spreads: Option<DiagonalPutSpreadView>,
    strategy_filter: &[SellOpportunityStrategy],
    exclude_strategy_filter: &[SellOpportunityStrategy],
    return_basis_filter: &[SellOpportunityReturnBasis],
    exclude_return_basis_filter: &[SellOpportunityReturnBasis],
    min_premium_or_credit: Option<f64>,
    max_risk: Option<f64>,
    min_abs_mispricing_percent: Option<f64>,
    min_abs_iv_diff_percent: Option<f64>,
    min_annualized_return: Option<f64>,
    max_annualized_return: Option<f64>,
    limit_per_strategy: Option<usize>,
    sort_by: Option<SellOpportunitySortField>,
    limit: Option<usize>,
) -> SellOpportunitiesView {
    let mut candidates = Vec::new();

    for row in cash_secured_puts.candidates {
        candidates.push(SellOpportunityCandidate {
            strategy: "cash_secured_put".to_string(),
            primary_symbol: row.option_symbol,
            secondary_symbol: None,
            annualized_return: row.annualized_return_on_cash,
            return_basis: SellOpportunityReturnBasis::CollateralReturn,
            annualized_return_note: None,
            premium_or_credit: row.premium_per_contract,
            max_risk: (row.cash_required_per_contract - row.premium_per_contract).max(0.0),
            breakeven: row.breakeven,
            breakeven_note: None,
            mispricing_percent: row.mispricing_percent,
            iv_diff_percent: row.iv_diff_percent,
        });
    }

    for row in covered_calls.candidates {
        candidates.push(SellOpportunityCandidate {
            strategy: "covered_call".to_string(),
            primary_symbol: row.option_symbol,
            secondary_symbol: None,
            annualized_return: row.annualized_premium_yield,
            return_basis: SellOpportunityReturnBasis::PremiumYield,
            annualized_return_note: None,
            premium_or_credit: row.premium_per_contract,
            max_risk: row.max_loss_per_contract,
            breakeven: row.breakeven,
            breakeven_note: None,
            mispricing_percent: row.mispricing_percent,
            iv_diff_percent: row.iv_diff_percent,
        });
    }

    for row in bull_put_spreads.candidates {
        candidates.push(SellOpportunityCandidate {
            strategy: "bull_put_spread".to_string(),
            primary_symbol: row.short_option_symbol,
            secondary_symbol: Some(row.long_option_symbol),
            annualized_return: row.annualized_return_on_risk,
            return_basis: SellOpportunityReturnBasis::MaxRiskReturn,
            annualized_return_note: None,
            premium_or_credit: row.net_credit_per_spread,
            max_risk: row.max_loss_per_spread,
            breakeven: row.breakeven,
            breakeven_note: None,
            mispricing_percent: row.short_mispricing_percent,
            iv_diff_percent: row.short_iv_diff_percent,
        });
    }

    for row in bear_call_spreads.candidates {
        candidates.push(SellOpportunityCandidate {
            strategy: "bear_call_spread".to_string(),
            primary_symbol: row.short_option_symbol,
            secondary_symbol: Some(row.long_option_symbol),
            annualized_return: row.annualized_return_on_risk,
            return_basis: SellOpportunityReturnBasis::MaxRiskReturn,
            annualized_return_note: None,
            premium_or_credit: row.net_credit_per_spread,
            max_risk: row.max_loss_per_spread,
            breakeven: row.breakeven,
            breakeven_note: None,
            mispricing_percent: row.short_mispricing_percent,
            iv_diff_percent: row.short_iv_diff_percent,
        });
    }

    if let Some(view) = calendar_call_spreads {
        for row in view.candidates {
            candidates.push(SellOpportunityCandidate {
                strategy: "calendar_call_spread".to_string(),
                primary_symbol: row.near_option_symbol,
                secondary_symbol: Some(row.far_option_symbol),
                annualized_return: row.annualized_theta_carry_return_on_debit,
                return_basis: SellOpportunityReturnBasis::ThetaCarryRunRate,
                annualized_return_note: Some(
                    "current theta carry run-rate; not a realized hold-period return".to_string(),
                ),
                premium_or_credit: row.net_debit_per_spread,
                max_risk: row.max_loss_per_spread,
                breakeven: 0.0,
                breakeven_note: Some("not modeled for cross-expiry structures".to_string()),
                mispricing_percent: 0.0,
                iv_diff_percent: 0.0,
            });
        }
    }

    if let Some(view) = calendar_put_spreads {
        for row in view.candidates {
            candidates.push(SellOpportunityCandidate {
                strategy: "calendar_put_spread".to_string(),
                primary_symbol: row.near_option_symbol,
                secondary_symbol: Some(row.far_option_symbol),
                annualized_return: row.annualized_theta_carry_return_on_debit,
                return_basis: SellOpportunityReturnBasis::ThetaCarryRunRate,
                annualized_return_note: Some(
                    "current theta carry run-rate; not a realized hold-period return".to_string(),
                ),
                premium_or_credit: row.net_debit_per_spread,
                max_risk: row.max_loss_per_spread,
                breakeven: 0.0,
                breakeven_note: Some("not modeled for cross-expiry structures".to_string()),
                mispricing_percent: 0.0,
                iv_diff_percent: 0.0,
            });
        }
    }

    if let Some(view) = diagonal_call_spreads {
        for row in view.candidates {
            candidates.push(SellOpportunityCandidate {
                strategy: "diagonal_call_spread".to_string(),
                primary_symbol: row.near_option_symbol,
                secondary_symbol: Some(row.far_option_symbol),
                annualized_return: row.annualized_theta_carry_return_on_debit,
                return_basis: SellOpportunityReturnBasis::ThetaCarryRunRate,
                annualized_return_note: Some(
                    "current theta carry run-rate; not a realized hold-period return".to_string(),
                ),
                premium_or_credit: row.net_debit_per_spread,
                max_risk: row.max_loss_per_spread,
                breakeven: 0.0,
                breakeven_note: Some("not modeled for cross-expiry structures".to_string()),
                mispricing_percent: 0.0,
                iv_diff_percent: 0.0,
            });
        }
    }

    if let Some(view) = diagonal_put_spreads {
        for row in view.candidates {
            candidates.push(SellOpportunityCandidate {
                strategy: "diagonal_put_spread".to_string(),
                primary_symbol: row.near_option_symbol,
                secondary_symbol: Some(row.far_option_symbol),
                annualized_return: row.annualized_theta_carry_return_on_debit,
                return_basis: SellOpportunityReturnBasis::ThetaCarryRunRate,
                annualized_return_note: Some(
                    "current theta carry run-rate; not a realized hold-period return".to_string(),
                ),
                premium_or_credit: row.net_debit_per_spread,
                max_risk: row.max_loss_per_spread,
                breakeven: 0.0,
                breakeven_note: Some("not modeled for cross-expiry structures".to_string()),
                mispricing_percent: 0.0,
                iv_diff_percent: 0.0,
            });
        }
    }

    candidates.retain(|candidate| {
        should_include_sell_opportunity_strategy(
            &candidate.strategy,
            strategy_filter,
            exclude_strategy_filter,
        ) && should_include_sell_opportunity_return_basis(
            candidate.return_basis,
            return_basis_filter,
            exclude_return_basis_filter,
        ) && min_premium_or_credit
            .map(|min| candidate.premium_or_credit >= min)
            .unwrap_or(true)
            && max_risk
                .map(|max| candidate.max_risk <= max)
                .unwrap_or(true)
            && matches_min_abs_threshold(candidate.mispricing_percent, min_abs_mispricing_percent)
            && matches_min_abs_threshold(candidate.iv_diff_percent, min_abs_iv_diff_percent)
            && matches_range(
                candidate.annualized_return,
                min_annualized_return,
                max_annualized_return,
            )
    });

    sort_sell_opportunities(&mut candidates, sort_by);
    candidates = apply_limit_per_strategy(candidates, limit_per_strategy);

    if let Some(limit) = limit {
        candidates.truncate(limit);
    }

    SellOpportunitiesView {
        underlying_symbol: cash_secured_puts.underlying_symbol,
        underlying_price: cash_secured_puts.underlying_price,
        expiry: cash_secured_puts.expiry,
        days_to_expiry: cash_secured_puts.days_to_expiry,
        rate: cash_secured_puts.rate,
        rate_source: cash_secured_puts.rate_source,
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::{ContractSide, OptionMetrics};
    use crate::domain::{
        BearCallSpreadCandidate, BearCallSpreadView, BullPutSpreadCandidate, BullPutSpreadView,
        CashSecuredPutCandidate, CashSecuredPutView, ChainAnalysisRow, ContractDiagnostics,
        CoveredCallCandidate, CoveredCallView,
    };

    #[test]
    fn sorts_candidates_by_annualized_return() {
        let view = test_cash_secured_put_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row("LOW", "1.0", "380"),
                    sample_row("HIGH", "2.0", "380"),
                ],
            },
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates[0].option_symbol, "HIGH");
        assert_eq!(view.candidates[1].option_symbol, "LOW");
    }

    #[test]
    fn filters_cash_secured_puts_by_mispricing_direction() {
        let view = build_cash_secured_put_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row("LOW", "0.8", "380"),
                    sample_row("HIGH", "2.0", "380"),
                ],
            },
            0.0,
            Some(MispricingDirection::Overpriced),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].option_symbol, "HIGH");
    }

    #[test]
    fn cash_secured_put_view_rejects_non_finite_underlying_price() {
        let err = build_cash_secured_put_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "NaN".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_row("PUT", "2.0", "380")],
            },
            0.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("failed to parse underlying price"));
    }

    #[test]
    fn sorts_covered_calls_by_annualized_premium_yield() {
        let view = build_covered_call_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_call_row("LOW", "1.0", "420"),
                    sample_call_row("HIGH", "2.0", "420"),
                ],
            },
            0.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates[0].option_symbol, "HIGH");
        assert_eq!(view.candidates[1].option_symbol, "LOW");
        assert!((view.candidates[0].breakeven - 398.0).abs() < 0.0001);
        assert!((view.candidates[0].max_loss_per_contract - 39_800.0).abs() < 0.0001);
    }

    #[test]
    fn covered_call_view_rejects_non_finite_option_price() {
        let err = build_covered_call_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_call_row("BROKEN", "NaN", "420")],
            },
            0.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();

        assert!(err.to_string().contains("failed to parse option price"));
    }

    #[test]
    fn filters_covered_calls_by_mispricing_direction() {
        let view = build_covered_call_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_call_row("LOW", "0.8", "420"),
                    sample_call_row("HIGH", "2.0", "420"),
                ],
            },
            0.0,
            Some(MispricingDirection::Overpriced),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].option_symbol, "HIGH");
    }

    #[test]
    fn builds_bull_put_spreads_from_put_pairs() {
        let view = build_bull_put_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row("SHORT", "2.5", "390"),
                    sample_row("LONG", "1.0", "380"),
                ],
            },
            0.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].short_option_symbol, "SHORT");
        assert_eq!(view.candidates[0].long_option_symbol, "LONG");
    }

    #[test]
    fn bull_put_spread_view_rejects_non_finite_strike_price() {
        let err = build_bull_put_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row("SHORT", "2.5", "390"),
                    sample_row("BROKEN", "1.0", "NaN"),
                ],
            },
            0.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("failed to parse spread strike price")
        );
    }

    #[test]
    fn builds_bear_call_spreads_from_call_pairs() {
        let view = build_bear_call_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_call_row("SHORT", "2.5", "410"),
                    sample_call_row("LONG", "1.0", "420"),
                ],
            },
            0.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].short_option_symbol, "SHORT");
        assert_eq!(view.candidates[0].long_option_symbol, "LONG");
    }

    #[test]
    fn builds_bear_put_spreads_from_put_pairs() {
        let view = build_bear_put_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row("LONG", "2.5", "390"),
                    sample_row("SHORT", "1.0", "380"),
                ],
            },
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].long_option_symbol, "LONG");
        assert_eq!(view.candidates[0].short_option_symbol, "SHORT");
    }

    #[test]
    fn builds_bull_call_spreads_from_call_pairs() {
        let view = build_bull_call_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_call_row("LONG", "2.5", "410"),
                    sample_call_row("SHORT", "1.0", "420"),
                ],
            },
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].long_option_symbol, "LONG");
        assert_eq!(view.candidates[0].short_option_symbol, "SHORT");
    }

    #[test]
    fn bull_call_spread_view_rejects_non_finite_strike_price() {
        let err = build_bull_call_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_call_row("LONG", "2.5", "410"),
                    sample_call_row("BROKEN", "1.0", "NaN"),
                ],
            },
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("failed to parse spread strike price")
        );
    }

    #[test]
    fn builds_calendar_call_spreads_from_matching_strikes() {
        let view = build_calendar_call_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_call_row("NEAR", "1.0", "420")],
            },
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-04-17".to_string(),
                days_to_expiry: 58,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_call_row("FAR", "2.0", "420")],
            },
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].near_option_symbol, "NEAR");
        assert_eq!(view.candidates[0].far_option_symbol, "FAR");
    }

    #[test]
    fn builds_calendar_put_spreads_from_matching_strikes() {
        let view = build_calendar_put_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_row("NEAR", "1.0", "380")],
            },
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-04-17".to_string(),
                days_to_expiry: 58,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_row("FAR", "2.0", "380")],
            },
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].near_option_symbol, "NEAR");
        assert_eq!(view.candidates[0].far_option_symbol, "FAR");
    }

    #[test]
    fn sorts_calendar_spreads_by_annualized_theta_carry_on_debit() {
        let mut near_a = sample_call_row("NEAR_A", "1.0", "420");
        near_a.local_greeks.theta_per_day = -0.01;
        let mut far_a = sample_call_row("FAR_A", "3.0", "420");
        far_a.local_greeks.theta_per_day = -0.002;

        let mut near_b = sample_call_row("NEAR_B", "1.0", "430");
        near_b.local_greeks.theta_per_day = -0.01;
        let mut far_b = sample_call_row("FAR_B", "1.5", "430");
        far_b.local_greeks.theta_per_day = -0.007;

        let view = build_calendar_call_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![near_a, near_b],
            },
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-04-17".to_string(),
                days_to_expiry: 58,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![far_a, far_b],
            },
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 2);
        assert_eq!(view.candidates[0].near_option_symbol, "NEAR_B");
        assert!(
            view.candidates[0].annualized_theta_carry_return_on_debit
                > view.candidates[1].annualized_theta_carry_return_on_debit
        );
    }

    #[test]
    fn sorts_calendar_spreads_by_debit_when_requested() {
        let mut candidates = vec![
            crate::domain::CalendarCallSpreadCandidate {
                near_option_symbol: "A".to_string(),
                far_option_symbol: "FA".to_string(),
                strike_price: "420".to_string(),
                near_expiry: "2026-03-20".to_string(),
                far_expiry: "2026-04-17".to_string(),
                days_gap: 28,
                strike_gap: 0.0,
                near_option_price: "1.0".to_string(),
                far_option_price: "3.0".to_string(),
                net_debit_per_spread: 200.0,
                net_theta_carry_per_day: 0.0100,
                theta_carry_return_on_debit_per_day: 0.0050,
                annualized_theta_carry_return_on_debit: 1.825,
                net_vega: 0.2000,
                vega_to_debit_ratio: 0.1000,
                max_loss_per_spread: 200.0,
                near_diagnostics: ContractDiagnostics::default(),
                far_diagnostics: ContractDiagnostics::default(),
            },
            crate::domain::CalendarCallSpreadCandidate {
                near_option_symbol: "B".to_string(),
                far_option_symbol: "FB".to_string(),
                strike_price: "430".to_string(),
                near_expiry: "2026-03-20".to_string(),
                far_expiry: "2026-04-17".to_string(),
                days_gap: 28,
                strike_gap: 0.0,
                near_option_price: "1.0".to_string(),
                far_option_price: "2.0".to_string(),
                net_debit_per_spread: 100.0,
                net_theta_carry_per_day: 0.0020,
                theta_carry_return_on_debit_per_day: 0.0020,
                annualized_theta_carry_return_on_debit: 0.730,
                net_vega: 0.0500,
                vega_to_debit_ratio: 0.0500,
                max_loss_per_spread: 100.0,
                near_diagnostics: ContractDiagnostics::default(),
                far_diagnostics: ContractDiagnostics::default(),
            },
        ];

        sort_calendar_call_candidates(&mut candidates, Some(CrossExpirySortField::Debit));

        assert_eq!(candidates[0].near_option_symbol, "B");
        assert_eq!(candidates[1].near_option_symbol, "A");
    }

    #[test]
    fn builds_diagonal_call_spreads_from_directional_strikes() {
        let view = build_diagonal_call_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_call_row("NEAR", "1.0", "420")],
            },
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-04-17".to_string(),
                days_to_expiry: 58,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_call_row("FAR", "2.0", "410")],
            },
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].near_option_symbol, "NEAR");
        assert_eq!(view.candidates[0].far_option_symbol, "FAR");
    }

    #[test]
    fn builds_diagonal_put_spreads_from_directional_strikes() {
        let view = build_diagonal_put_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_row("NEAR", "1.0", "380")],
            },
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-04-17".to_string(),
                days_to_expiry: 58,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_row("FAR", "2.0", "390")],
            },
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].near_option_symbol, "NEAR");
        assert_eq!(view.candidates[0].far_option_symbol, "FAR");
    }

    #[test]
    fn filters_diagonal_call_spreads_by_max_strike_gap() {
        let view = build_diagonal_call_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_call_row("NEAR", "1.0", "420")],
            },
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-04-17".to_string(),
                days_to_expiry: 58,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![sample_call_row("FAR", "2.0", "410")],
            },
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(5.0),
            None,
        )
        .expect("view builds");

        assert!(view.candidates.is_empty());
    }

    #[test]
    fn filters_bull_put_spreads_by_short_leg_mispricing_direction() {
        let view = build_bull_put_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row("SHORT_LOW", "0.8", "390"),
                    sample_row("SHORT_HIGH", "2.0", "390"),
                    sample_row("LONG", "0.5", "380"),
                ],
            },
            0.0,
            Some(MispricingDirection::Overpriced),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].short_option_symbol, "SHORT_HIGH");
    }

    #[test]
    fn filters_bear_call_spreads_by_short_leg_mispricing_direction() {
        let view = build_bear_call_spread_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_call_row("SHORT_LOW", "0.8", "410"),
                    sample_call_row("SHORT_HIGH", "2.0", "410"),
                    sample_call_row("LONG", "0.5", "420"),
                ],
            },
            0.0,
            Some(MispricingDirection::Overpriced),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].short_option_symbol, "SHORT_HIGH");
    }

    #[test]
    fn merges_sell_opportunities_by_annualized_return() {
        let view = merge_sell_opportunities(
            CashSecuredPutView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![CashSecuredPutCandidate {
                    option_symbol: "PUT1".to_string(),
                    strike_price: "380".to_string(),
                    option_price: "2.0".to_string(),
                    implied_volatility: "0.3".to_string(),
                    provider_reported_iv: "0.3".to_string(),
                    days_to_expiry: 30,
                    delta: -0.2,
                    theta_per_day: -0.01,
                    otm_percent: 0.05,
                    breakeven: 378.0,
                    premium_per_contract: 200.0,
                    cash_required_per_contract: 38000.0,
                    return_on_cash: 0.005,
                    annualized_return_on_cash: 0.10,
                    fair_value: 1.0,
                    mispricing: 1.0,
                    mispricing_percent: 1.0,
                    solved_iv_from_market_price: 0.35,
                    iv_diff: 0.05,
                    iv_diff_percent: 0.1667,
                    diagnostics: ContractDiagnostics::default(),
                }],
            },
            CoveredCallView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![CoveredCallCandidate {
                    option_symbol: "CALL1".to_string(),
                    strike_price: "420".to_string(),
                    option_price: "1.5".to_string(),
                    implied_volatility: "0.3".to_string(),
                    provider_reported_iv: "0.3".to_string(),
                    days_to_expiry: 30,
                    delta: 0.2,
                    theta_per_day: -0.01,
                    otm_percent: 0.05,
                    breakeven: 398.5,
                    premium_per_contract: 150.0,
                    premium_yield_on_underlying: 0.00375,
                    annualized_premium_yield: 0.08,
                    max_sale_value_per_contract: 42000.0,
                    max_profit_per_contract: 2150.0,
                    max_loss_per_contract: 39850.0,
                    fair_value: 1.0,
                    mispricing: 0.5,
                    mispricing_percent: 0.5,
                    solved_iv_from_market_price: 0.33,
                    iv_diff: 0.03,
                    iv_diff_percent: 0.10,
                    diagnostics: ContractDiagnostics::default(),
                }],
            },
            BullPutSpreadView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![BullPutSpreadCandidate {
                    short_option_symbol: "BPS_S".to_string(),
                    long_option_symbol: "BPS_L".to_string(),
                    short_strike_price: "390".to_string(),
                    long_strike_price: "380".to_string(),
                    short_option_price: "2.5".to_string(),
                    long_option_price: "1.0".to_string(),
                    short_provider_reported_iv: "0.3".to_string(),
                    days_to_expiry: 30,
                    short_delta: -0.2,
                    short_otm_percent: 0.025,
                    short_fair_value: 1.0,
                    short_mispricing: 1.5,
                    short_mispricing_percent: 1.5,
                    short_solved_iv_from_market_price: 0.36,
                    short_iv_diff: 0.06,
                    short_iv_diff_percent: 0.20,
                    width_per_spread: 1000.0,
                    net_credit_per_spread: 150.0,
                    max_profit_per_spread: 150.0,
                    max_loss_per_spread: 850.0,
                    breakeven: 388.5,
                    return_on_risk: 0.176,
                    annualized_return_on_risk: 0.20,
                    short_diagnostics: ContractDiagnostics::default(),
                    long_diagnostics: ContractDiagnostics::default(),
                }],
            },
            BearCallSpreadView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![BearCallSpreadCandidate {
                    short_option_symbol: "BCS_S".to_string(),
                    long_option_symbol: "BCS_L".to_string(),
                    short_strike_price: "410".to_string(),
                    long_strike_price: "420".to_string(),
                    short_option_price: "2.5".to_string(),
                    long_option_price: "1.0".to_string(),
                    short_provider_reported_iv: "0.3".to_string(),
                    days_to_expiry: 30,
                    short_delta: 0.2,
                    short_otm_percent: 0.025,
                    short_fair_value: 1.0,
                    short_mispricing: 1.5,
                    short_mispricing_percent: 1.5,
                    short_solved_iv_from_market_price: 0.36,
                    short_iv_diff: 0.06,
                    short_iv_diff_percent: 0.20,
                    width_per_spread: 1000.0,
                    net_credit_per_spread: 150.0,
                    max_profit_per_spread: 150.0,
                    max_loss_per_spread: 850.0,
                    breakeven: 411.5,
                    return_on_risk: 0.176,
                    annualized_return_on_risk: 0.15,
                    short_diagnostics: ContractDiagnostics::default(),
                    long_diagnostics: ContractDiagnostics::default(),
                }],
            },
            None,
            None,
            None,
            None,
            &[],
            &[],
            &[],
            &[],
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(2),
        );

        assert_eq!(view.candidates.len(), 2);
        assert_eq!(view.candidates[0].strategy, "bull_put_spread");
        assert_eq!(view.candidates[1].strategy, "bear_call_spread");
    }

    #[test]
    fn merge_sell_opportunities_uses_true_max_loss_for_cash_secured_put() {
        let view = merge_sell_opportunities(
            CashSecuredPutView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![CashSecuredPutCandidate {
                    option_symbol: "PUT1".to_string(),
                    strike_price: "380".to_string(),
                    option_price: "2.0".to_string(),
                    implied_volatility: "0.3".to_string(),
                    provider_reported_iv: "0.3".to_string(),
                    days_to_expiry: 30,
                    delta: -0.2,
                    theta_per_day: -0.01,
                    otm_percent: 0.05,
                    breakeven: 378.0,
                    premium_per_contract: 200.0,
                    cash_required_per_contract: 38000.0,
                    return_on_cash: 0.005,
                    annualized_return_on_cash: 0.10,
                    fair_value: 1.0,
                    mispricing: 1.0,
                    mispricing_percent: 1.0,
                    solved_iv_from_market_price: 0.35,
                    iv_diff: 0.05,
                    iv_diff_percent: 0.1667,
                    diagnostics: ContractDiagnostics::default(),
                }],
            },
            CoveredCallView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![],
            },
            BullPutSpreadView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![],
            },
            BearCallSpreadView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![],
            },
            None,
            None,
            None,
            None,
            &[],
            &[],
            &[],
            &[],
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].strategy, "cash_secured_put");
        assert!((view.candidates[0].max_risk - 37_800.0).abs() < 0.0001);
    }

    #[test]
    fn merge_sell_opportunities_marks_cross_expiry_breakeven_as_unmodeled() {
        let view = merge_sell_opportunities(
            CashSecuredPutView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![],
            },
            CoveredCallView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![],
            },
            BullPutSpreadView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![],
            },
            BearCallSpreadView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![],
            },
            Some(CalendarCallSpreadView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                near_expiry: "2026-03-20".to_string(),
                far_expiry: "2026-04-17".to_string(),
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                candidates: vec![CalendarCallSpreadCandidate {
                    near_option_symbol: "TSLA250320C00400000".to_string(),
                    far_option_symbol: "TSLA250417C00400000".to_string(),
                    strike_price: "400".to_string(),
                    near_expiry: "2026-03-20".to_string(),
                    far_expiry: "2026-04-17".to_string(),
                    days_gap: 28,
                    strike_gap: 0.0,
                    near_option_price: "8.0".to_string(),
                    far_option_price: "12.0".to_string(),
                    net_debit_per_spread: 400.0,
                    net_theta_carry_per_day: 5.0,
                    theta_carry_return_on_debit_per_day: 0.0125,
                    annualized_theta_carry_return_on_debit: 0.30,
                    net_vega: 0.20,
                    vega_to_debit_ratio: 0.0005,
                    max_loss_per_spread: 400.0,
                    near_diagnostics: ContractDiagnostics::default(),
                    far_diagnostics: ContractDiagnostics::default(),
                }],
            }),
            None,
            None,
            None,
            &[],
            &[],
            &[],
            &[],
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].strategy, "calendar_call_spread");
        assert_eq!(
            view.candidates[0].annualized_return_note.as_deref(),
            Some("current theta carry run-rate; not a realized hold-period return")
        );
        assert_eq!(view.candidates[0].breakeven, 0.0);
        assert_eq!(
            view.candidates[0].breakeven_note.as_deref(),
            Some("not modeled for cross-expiry structures")
        );
    }

    #[test]
    fn sorts_sell_opportunities_by_absolute_mispricing() {
        let mut candidates = vec![
            SellOpportunityCandidate {
                strategy: "covered_call".to_string(),
                primary_symbol: "LOW".to_string(),
                secondary_symbol: None,
                annualized_return: 0.30,
                return_basis: SellOpportunityReturnBasis::PremiumYield,
                annualized_return_note: None,
                premium_or_credit: 100.0,
                max_risk: 39900.0,
                breakeven: 401.0,
                breakeven_note: None,
                mispricing_percent: 0.10,
                iv_diff_percent: 0.05,
            },
            SellOpportunityCandidate {
                strategy: "cash_secured_put".to_string(),
                primary_symbol: "HIGH".to_string(),
                secondary_symbol: None,
                annualized_return: 0.10,
                return_basis: SellOpportunityReturnBasis::CollateralReturn,
                annualized_return_note: None,
                premium_or_credit: 100.0,
                max_risk: 10000.0,
                breakeven: 390.0,
                breakeven_note: None,
                mispricing_percent: 0.40,
                iv_diff_percent: 0.02,
            },
        ];

        sort_sell_opportunities(&mut candidates, Some(SellOpportunitySortField::Mispricing));

        assert_eq!(candidates[0].primary_symbol, "HIGH");
    }

    #[test]
    fn annualized_sort_prioritizes_realized_return_over_run_rate_notes() {
        let mut candidates = vec![
            SellOpportunityCandidate {
                strategy: "calendar_call_spread".to_string(),
                primary_symbol: "RUNRATE".to_string(),
                secondary_symbol: Some("FAR".to_string()),
                annualized_return: 0.80,
                return_basis: SellOpportunityReturnBasis::ThetaCarryRunRate,
                annualized_return_note: Some(
                    "current theta carry run-rate; not a realized hold-period return".to_string(),
                ),
                premium_or_credit: 200.0,
                max_risk: 400.0,
                breakeven: 0.0,
                breakeven_note: Some("not modeled for cross-expiry structures".to_string()),
                mispricing_percent: 0.0,
                iv_diff_percent: 0.0,
            },
            SellOpportunityCandidate {
                strategy: "covered_call".to_string(),
                primary_symbol: "REALIZED".to_string(),
                secondary_symbol: None,
                annualized_return: 0.20,
                return_basis: SellOpportunityReturnBasis::PremiumYield,
                annualized_return_note: None,
                premium_or_credit: 100.0,
                max_risk: 39900.0,
                breakeven: 401.0,
                breakeven_note: None,
                mispricing_percent: 0.10,
                iv_diff_percent: 0.05,
            },
        ];

        sort_sell_opportunities(
            &mut candidates,
            Some(SellOpportunitySortField::AnnualizedReturn),
        );

        assert_eq!(candidates[0].primary_symbol, "REALIZED");
        assert_eq!(candidates[1].primary_symbol, "RUNRATE");
    }

    #[test]
    fn matches_annualized_return_range() {
        assert!(matches_range(0.20, Some(0.10), Some(0.30)));
        assert!(!matches_range(0.05, Some(0.10), Some(0.30)));
        assert!(!matches_range(0.35, Some(0.10), Some(0.30)));
        assert!(matches_range(0.20, None, Some(0.30)));
        assert!(matches_range(0.20, Some(0.10), None));
    }

    #[test]
    fn filters_sell_opportunities_by_capital_constraints() {
        let mut candidates = vec![
            SellOpportunityCandidate {
                strategy: "covered_call".to_string(),
                primary_symbol: "LOW_PREMIUM".to_string(),
                secondary_symbol: None,
                annualized_return: 0.20,
                return_basis: SellOpportunityReturnBasis::PremiumYield,
                annualized_return_note: None,
                premium_or_credit: 50.0,
                max_risk: 39950.0,
                breakeven: 399.5,
                breakeven_note: None,
                mispricing_percent: 0.10,
                iv_diff_percent: 0.05,
            },
            SellOpportunityCandidate {
                strategy: "cash_secured_put".to_string(),
                primary_symbol: "HIGH_RISK".to_string(),
                secondary_symbol: None,
                annualized_return: 0.20,
                return_basis: SellOpportunityReturnBasis::CollateralReturn,
                annualized_return_note: None,
                premium_or_credit: 200.0,
                max_risk: 20000.0,
                breakeven: 390.0,
                breakeven_note: None,
                mispricing_percent: 0.10,
                iv_diff_percent: 0.05,
            },
            SellOpportunityCandidate {
                strategy: "bull_put_spread".to_string(),
                primary_symbol: "PASS".to_string(),
                secondary_symbol: None,
                annualized_return: 0.20,
                return_basis: SellOpportunityReturnBasis::MaxRiskReturn,
                annualized_return_note: None,
                premium_or_credit: 150.0,
                max_risk: 5000.0,
                breakeven: 390.0,
                breakeven_note: None,
                mispricing_percent: 0.10,
                iv_diff_percent: 0.05,
            },
        ];

        candidates.retain(|candidate| {
            candidate.premium_or_credit >= 100.0 && candidate.max_risk <= 10000.0
        });

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].primary_symbol, "PASS");
    }

    #[test]
    fn filters_sell_opportunities_by_abs_signal_strength() {
        let mut candidates = vec![
            SellOpportunityCandidate {
                strategy: "covered_call".to_string(),
                primary_symbol: "LOW".to_string(),
                secondary_symbol: None,
                annualized_return: 0.20,
                return_basis: SellOpportunityReturnBasis::PremiumYield,
                annualized_return_note: None,
                premium_or_credit: 100.0,
                max_risk: 39900.0,
                breakeven: 401.0,
                breakeven_note: None,
                mispricing_percent: 0.05,
                iv_diff_percent: 0.05,
            },
            SellOpportunityCandidate {
                strategy: "cash_secured_put".to_string(),
                primary_symbol: "PASS".to_string(),
                secondary_symbol: None,
                annualized_return: 0.20,
                return_basis: SellOpportunityReturnBasis::CollateralReturn,
                annualized_return_note: None,
                premium_or_credit: 100.0,
                max_risk: 10000.0,
                breakeven: 390.0,
                breakeven_note: None,
                mispricing_percent: 0.15,
                iv_diff_percent: 0.20,
            },
        ];

        candidates.retain(|candidate| {
            matches_min_abs_threshold(candidate.mispricing_percent, Some(0.10))
                && matches_min_abs_threshold(candidate.iv_diff_percent, Some(0.10))
        });

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].primary_symbol, "PASS");
    }

    #[test]
    fn limits_sell_opportunities_per_strategy() {
        let filtered = apply_limit_per_strategy(
            vec![
                SellOpportunityCandidate {
                    strategy: "covered_call".to_string(),
                    primary_symbol: "CALL1".to_string(),
                    secondary_symbol: None,
                    annualized_return: 0.30,
                    return_basis: SellOpportunityReturnBasis::PremiumYield,
                    annualized_return_note: None,
                    premium_or_credit: 100.0,
                    max_risk: 39900.0,
                    breakeven: 401.0,
                    breakeven_note: None,
                    mispricing_percent: 0.10,
                    iv_diff_percent: 0.05,
                },
                SellOpportunityCandidate {
                    strategy: "covered_call".to_string(),
                    primary_symbol: "CALL2".to_string(),
                    secondary_symbol: None,
                    annualized_return: 0.20,
                    return_basis: SellOpportunityReturnBasis::PremiumYield,
                    annualized_return_note: None,
                    premium_or_credit: 90.0,
                    max_risk: 39910.0,
                    breakeven: 402.0,
                    breakeven_note: None,
                    mispricing_percent: 0.08,
                    iv_diff_percent: 0.04,
                },
                SellOpportunityCandidate {
                    strategy: "cash_secured_put".to_string(),
                    primary_symbol: "PUT1".to_string(),
                    secondary_symbol: None,
                    annualized_return: 0.10,
                    return_basis: SellOpportunityReturnBasis::CollateralReturn,
                    annualized_return_note: None,
                    premium_or_credit: 120.0,
                    max_risk: 10000.0,
                    breakeven: 390.0,
                    breakeven_note: None,
                    mispricing_percent: 0.12,
                    iv_diff_percent: 0.06,
                },
            ],
            Some(1),
        );

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].primary_symbol, "CALL1");
        assert_eq!(filtered[1].primary_symbol, "PUT1");
    }

    #[test]
    fn filters_sell_opportunities_by_strategy() {
        assert!(matches_sell_opportunity_strategy(
            "cash_secured_put",
            &[SellOpportunityStrategy::CashSecuredPut],
        ));
        assert!(!matches_sell_opportunity_strategy(
            "covered_call",
            &[SellOpportunityStrategy::CashSecuredPut],
        ));
        assert!(matches_sell_opportunity_strategy("covered_call", &[],));
    }

    #[test]
    fn excludes_sell_opportunities_by_strategy() {
        assert!(!should_include_sell_opportunity_strategy(
            "covered_call",
            &[],
            &[SellOpportunityStrategy::CoveredCall],
        ));
        assert!(should_include_sell_opportunity_strategy(
            "cash_secured_put",
            &[],
            &[SellOpportunityStrategy::CoveredCall],
        ));
        assert!(!should_include_sell_opportunity_strategy(
            "covered_call",
            &[SellOpportunityStrategy::CoveredCall],
            &[SellOpportunityStrategy::CoveredCall],
        ));
    }

    #[test]
    fn filters_sell_opportunities_by_return_basis() {
        assert!(should_include_sell_opportunity_return_basis(
            SellOpportunityReturnBasis::PremiumYield,
            &[SellOpportunityReturnBasis::PremiumYield],
            &[],
        ));
        assert!(!should_include_sell_opportunity_return_basis(
            SellOpportunityReturnBasis::ThetaCarryRunRate,
            &[SellOpportunityReturnBasis::PremiumYield],
            &[],
        ));
        assert!(!should_include_sell_opportunity_return_basis(
            SellOpportunityReturnBasis::ThetaCarryRunRate,
            &[],
            &[SellOpportunityReturnBasis::ThetaCarryRunRate],
        ));
    }

    #[test]
    fn sorts_mispricing_by_absolute_deviation() {
        let view = build_mispricing_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row("LOW", "0.8", "380"),
                    sample_row("HIGH", "2.0", "380"),
                ],
            },
            0.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates[0].option_symbol, "HIGH");
    }

    #[test]
    fn filters_mispricing_by_direction() {
        let view = build_mispricing_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row("LOW", "0.8", "380"),
                    sample_row("HIGH", "2.0", "380"),
                ],
            },
            0.0,
            Some(MispricingDirection::Overpriced),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("view builds");

        assert_eq!(view.candidates.len(), 1);
        assert_eq!(view.candidates[0].option_symbol, "HIGH");
    }

    #[test]
    fn filters_mispricing_by_iv_direction() {
        assert!(matches_iv_diff_direction(
            0.1,
            Some(IvDiffDirection::Higher)
        ));
        assert!(!matches_iv_diff_direction(
            -0.1,
            Some(IvDiffDirection::Higher)
        ));
        assert!(matches_iv_diff_direction(
            -0.1,
            Some(IvDiffDirection::Lower)
        ));
        assert!(!matches_iv_diff_direction(
            0.1,
            Some(IvDiffDirection::Lower)
        ));
    }

    #[test]
    fn sorts_mispricing_by_absolute_iv_diff() {
        let mut candidates = vec![
            MispricingCandidate {
                option_symbol: "LOW_IV".to_string(),
                option_type: ContractSide::Put,
                strike_price: "380".to_string(),
                option_price: "1.0".to_string(),
                provider_reported_iv: "0.3".to_string(),
                solved_iv_from_market_price: 0.32,
                iv_diff: 0.02,
                iv_diff_percent: 0.05,
                fair_value: 1.0,
                mispricing: 0.20,
                mispricing_percent: 0.20,
                diagnostics: ContractDiagnostics::default(),
                local_greeks: sample_row("TMP", "1.0", "380").local_greeks,
            },
            MispricingCandidate {
                option_symbol: "HIGH_IV".to_string(),
                option_type: ContractSide::Put,
                strike_price: "380".to_string(),
                option_price: "1.0".to_string(),
                provider_reported_iv: "0.3".to_string(),
                solved_iv_from_market_price: 0.42,
                iv_diff: 0.12,
                iv_diff_percent: 0.40,
                fair_value: 1.0,
                mispricing: 0.10,
                mispricing_percent: 0.10,
                diagnostics: ContractDiagnostics::default(),
                local_greeks: sample_row("TMP", "1.0", "380").local_greeks,
            },
        ];

        sort_mispricing_candidates(&mut candidates, Some(MispricingSortField::IvDiff));

        assert_eq!(candidates[0].option_symbol, "HIGH_IV");
    }

    #[test]
    fn resolves_cross_expiry_inclusion_flags() {
        assert_eq!(resolve_cross_expiry_inclusion(None, None), (true, true));
        assert_eq!(
            resolve_cross_expiry_inclusion(Some(true), None),
            (true, false)
        );
        assert_eq!(
            resolve_cross_expiry_inclusion(None, Some(true)),
            (false, true)
        );
        assert_eq!(
            resolve_cross_expiry_inclusion(Some(true), Some(true)),
            (true, true)
        );
    }

    fn test_cash_secured_put_view(
        analysis: ChainAnalysisView,
        direction: Option<MispricingDirection>,
    ) -> Result<CashSecuredPutView> {
        build_cash_secured_put_view(
            analysis, 0.0, direction, None, None, None, None, None, None, None, None, None,
        )
    }

    fn build_cash_secured_put_view(
        analysis: ChainAnalysisView,
        dividend: f64,
        direction: Option<MispricingDirection>,
        iv_direction: Option<IvDiffDirection>,
        min_open_interest: Option<i64>,
        min_volume: Option<i64>,
        min_premium_per_contract: Option<f64>,
        max_cash_required_per_contract: Option<f64>,
        min_annualized_return_on_cash: Option<f64>,
        max_annualized_return_on_cash: Option<f64>,
        min_abs_mispricing_percent: Option<f64>,
        min_abs_iv_diff_percent: Option<f64>,
    ) -> Result<CashSecuredPutView> {
        super::build_cash_secured_put_view(
            analysis,
            dividend,
            direction,
            iv_direction,
            min_open_interest,
            min_volume,
            min_premium_per_contract,
            max_cash_required_per_contract,
            min_annualized_return_on_cash,
            max_annualized_return_on_cash,
            min_abs_mispricing_percent,
            min_abs_iv_diff_percent,
            None,
        )
    }

    fn build_covered_call_view(
        analysis: ChainAnalysisView,
        dividend: f64,
        direction: Option<MispricingDirection>,
        iv_direction: Option<IvDiffDirection>,
        min_open_interest: Option<i64>,
        min_volume: Option<i64>,
        min_premium_per_contract: Option<f64>,
        min_annualized_premium_yield: Option<f64>,
        max_annualized_premium_yield: Option<f64>,
        min_abs_mispricing_percent: Option<f64>,
        min_abs_iv_diff_percent: Option<f64>,
        limit: Option<usize>,
        _unused: Option<usize>,
    ) -> Result<CoveredCallView> {
        super::build_covered_call_view(
            analysis,
            dividend,
            direction,
            iv_direction,
            min_open_interest,
            min_volume,
            min_premium_per_contract,
            min_annualized_premium_yield,
            max_annualized_premium_yield,
            min_abs_mispricing_percent,
            min_abs_iv_diff_percent,
            limit,
        )
    }

    fn build_bull_put_spread_view(
        analysis: ChainAnalysisView,
        dividend: f64,
        direction: Option<MispricingDirection>,
        iv_direction: Option<IvDiffDirection>,
        min_open_interest: Option<i64>,
        min_volume: Option<i64>,
        min_net_credit_per_spread: Option<f64>,
        min_width_per_spread: Option<f64>,
        max_width_per_spread: Option<f64>,
        min_annualized_return_on_risk: Option<f64>,
        max_annualized_return_on_risk: Option<f64>,
        min_abs_mispricing_percent: Option<f64>,
    ) -> Result<BullPutSpreadView> {
        super::build_bull_put_spread_view(
            analysis,
            dividend,
            direction,
            iv_direction,
            min_open_interest,
            min_volume,
            min_net_credit_per_spread,
            min_width_per_spread,
            max_width_per_spread,
            min_annualized_return_on_risk,
            max_annualized_return_on_risk,
            min_abs_mispricing_percent,
            None,
            None,
        )
    }

    fn build_bear_call_spread_view(
        analysis: ChainAnalysisView,
        dividend: f64,
        direction: Option<MispricingDirection>,
        iv_direction: Option<IvDiffDirection>,
        min_open_interest: Option<i64>,
        min_volume: Option<i64>,
        min_net_credit_per_spread: Option<f64>,
        min_width_per_spread: Option<f64>,
        max_width_per_spread: Option<f64>,
        min_annualized_return_on_risk: Option<f64>,
        max_annualized_return_on_risk: Option<f64>,
        min_abs_mispricing_percent: Option<f64>,
        min_abs_iv_diff_percent: Option<f64>,
        limit: Option<usize>,
        _unused: Option<usize>,
    ) -> Result<BearCallSpreadView> {
        super::build_bear_call_spread_view(
            analysis,
            dividend,
            direction,
            iv_direction,
            min_open_interest,
            min_volume,
            min_net_credit_per_spread,
            min_width_per_spread,
            max_width_per_spread,
            min_annualized_return_on_risk,
            max_annualized_return_on_risk,
            min_abs_mispricing_percent,
            min_abs_iv_diff_percent,
            limit,
        )
    }

    fn build_bear_put_spread_view(
        analysis: ChainAnalysisView,
        min_open_interest: Option<i64>,
        min_volume: Option<i64>,
        max_net_debit_per_spread: Option<f64>,
        min_width_per_spread: Option<f64>,
        max_width_per_spread: Option<f64>,
        min_annualized_return_on_risk: Option<f64>,
        max_annualized_return_on_risk: Option<f64>,
        limit: Option<usize>,
        _unused: Option<usize>,
    ) -> Result<BearPutSpreadView> {
        super::build_bear_put_spread_view(
            analysis,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_width_per_spread,
            max_width_per_spread,
            min_annualized_return_on_risk,
            max_annualized_return_on_risk,
            limit,
        )
    }

    fn build_bull_call_spread_view(
        analysis: ChainAnalysisView,
        min_open_interest: Option<i64>,
        min_volume: Option<i64>,
        max_net_debit_per_spread: Option<f64>,
        min_width_per_spread: Option<f64>,
        max_width_per_spread: Option<f64>,
        min_annualized_return_on_risk: Option<f64>,
        max_annualized_return_on_risk: Option<f64>,
        limit: Option<usize>,
        _unused: Option<usize>,
    ) -> Result<BullCallSpreadView> {
        super::build_bull_call_spread_view(
            analysis,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_width_per_spread,
            max_width_per_spread,
            min_annualized_return_on_risk,
            max_annualized_return_on_risk,
            limit,
        )
    }

    fn build_calendar_call_spread_view(
        near: ChainAnalysisView,
        far: ChainAnalysisView,
        min_open_interest: Option<i64>,
        min_volume: Option<i64>,
        max_net_debit_per_spread: Option<f64>,
        min_net_theta_carry_per_day: Option<f64>,
        min_days_gap: Option<i64>,
        max_days_gap: Option<i64>,
        min_strike_gap: Option<f64>,
        max_strike_gap: Option<f64>,
        limit: Option<usize>,
        _unused: Option<usize>,
    ) -> Result<CalendarCallSpreadView> {
        super::build_calendar_call_spread_view(
            near,
            far,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_net_theta_carry_per_day,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            limit,
        )
    }

    fn build_calendar_put_spread_view(
        near: ChainAnalysisView,
        far: ChainAnalysisView,
        min_open_interest: Option<i64>,
        min_volume: Option<i64>,
        max_net_debit_per_spread: Option<f64>,
        min_net_theta_carry_per_day: Option<f64>,
        min_days_gap: Option<i64>,
        max_days_gap: Option<i64>,
        min_strike_gap: Option<f64>,
        max_strike_gap: Option<f64>,
        limit: Option<usize>,
        _unused: Option<usize>,
    ) -> Result<CalendarPutSpreadView> {
        super::build_calendar_put_spread_view(
            near,
            far,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_net_theta_carry_per_day,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            limit,
        )
    }

    fn build_mispricing_view(
        analysis: ChainAnalysisView,
        dividend: f64,
        direction: Option<MispricingDirection>,
        iv_direction: Option<IvDiffDirection>,
        min_open_interest: Option<i64>,
        min_volume: Option<i64>,
        min_abs_mispricing_percent: Option<f64>,
        min_abs_iv_diff_percent: Option<f64>,
        sort_by: Option<MispricingSortField>,
    ) -> Result<MispricingView> {
        super::build_mispricing_view(
            analysis,
            dividend,
            direction,
            iv_direction,
            min_open_interest,
            min_volume,
            min_abs_mispricing_percent,
            min_abs_iv_diff_percent,
            sort_by,
            None,
        )
    }

    fn sample_row(symbol: &str, option_price: &str, strike_price: &str) -> ChainAnalysisRow {
        ChainAnalysisRow {
            option_symbol: symbol.to_string(),
            option_type: ContractSide::Put,
            option_price: option_price.to_string(),
            volume: 10,
            open_interest: 20,
            strike_price: strike_price.to_string(),
            implied_volatility: "0.3".to_string(),
            implied_volatility_source: "provider".to_string(),
            provider_reported_iv: "0.3".to_string(),
            diagnostics: ContractDiagnostics {
                is_liquid: true,
                otm_percent: 0.05,
                breakeven: 379.0,
                ..ContractDiagnostics::default()
            },
            local_greeks: OptionMetrics {
                option_type: ContractSide::Put,
                fair_value: 1.0,
                delta: -0.2,
                gamma: 0.1,
                vega: 0.2,
                theta_per_day: -0.01,
                rho: -0.1,
                d1: 0.0,
                d2: 0.0,
            },
        }
    }

    fn sample_call_row(symbol: &str, option_price: &str, strike_price: &str) -> ChainAnalysisRow {
        ChainAnalysisRow {
            option_symbol: symbol.to_string(),
            option_type: ContractSide::Call,
            option_price: option_price.to_string(),
            volume: 10,
            open_interest: 20,
            strike_price: strike_price.to_string(),
            implied_volatility: "0.3".to_string(),
            implied_volatility_source: "provider".to_string(),
            provider_reported_iv: "0.3".to_string(),
            diagnostics: ContractDiagnostics {
                is_liquid: true,
                otm_percent: 0.05,
                breakeven: 421.0,
                ..ContractDiagnostics::default()
            },
            local_greeks: OptionMetrics {
                option_type: ContractSide::Call,
                fair_value: 1.0,
                delta: 0.2,
                gamma: 0.1,
                vega: 0.2,
                theta_per_day: -0.01,
                rho: 0.1,
                d1: 0.0,
                d2: 0.0,
            },
        }
    }
}
