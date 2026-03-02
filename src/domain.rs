use crate::analytics::{ContractSide, OptionMetrics};
use serde::Serialize;
use std::cmp::Ordering;
use time::Date;

#[derive(Debug, Clone, Serialize)]
pub struct UnderlyingSnapshot {
    pub symbol: String,
    pub last_done: String,
    pub last_done_f64: f64,
    pub prev_close: String,
    pub prev_close_f64: f64,
    pub open: String,
    pub open_f64: f64,
    pub high: String,
    pub high_f64: f64,
    pub low: String,
    pub low_f64: f64,
    pub volume: i64,
    pub turnover: String,
    pub turnover_f64: f64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptionContractSnapshot {
    pub symbol: String,
    pub underlying_symbol: String,
    pub option_type: ContractSide,
    pub last_done: String,
    pub last_done_f64: f64,
    pub prev_close: String,
    pub prev_close_f64: f64,
    pub open: String,
    pub open_f64: f64,
    pub high: String,
    pub high_f64: f64,
    pub low: String,
    pub low_f64: f64,
    pub timestamp: String,
    pub volume: i64,
    pub turnover: String,
    pub turnover_f64: f64,
    pub trade_status: String,
    pub strike_price: String,
    pub strike_price_f64: f64,
    pub expiry: Date,
    pub provider_reported_iv: String,
    pub provider_reported_iv_f64: f64,
    pub open_interest: i64,
    pub historical_volatility: String,
    pub historical_volatility_f64: f64,
    pub contract_multiplier: String,
    pub contract_multiplier_f64: f64,
    pub contract_size: String,
    pub contract_size_f64: f64,
    pub contract_style: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptionChainSnapshot {
    pub underlying: UnderlyingSnapshot,
    pub expiry: Date,
    pub days_to_expiry: i64,
    pub contracts: Vec<OptionContractSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OptionChainStrikeRow {
    pub strike_price: String,
    pub strike_price_f64: f64,
    pub call: Option<OptionContractSnapshot>,
    pub call_diagnostics: Option<ContractDiagnostics>,
    pub put: Option<OptionContractSnapshot>,
    pub put_diagnostics: Option<ContractDiagnostics>,
}

#[derive(Debug, Clone, Serialize)]
pub struct NormalizedOptionChainSnapshot {
    pub underlying: UnderlyingSnapshot,
    pub expiry: Date,
    pub days_to_expiry: i64,
    pub rows: Vec<OptionChainStrikeRow>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ContractDiagnostics {
    pub zero_last_done: bool,
    pub zero_volume: bool,
    pub zero_open_interest: bool,
    pub non_positive_iv: bool,
    pub non_standard_contract: bool,
    pub halted_or_abnormal_trade_status: bool,
    pub near_expiry: bool,
    pub is_liquid: bool,
    pub otm_percent: f64,
    pub intrinsic_value: f64,
    pub extrinsic_value: f64,
    pub breakeven: f64,
    pub quality_flags: Vec<String>,
    pub liquidity_flags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderGreeks {
    pub delta: Option<String>,
    pub gamma: Option<String>,
    pub theta: Option<String>,
    pub vega: Option<String>,
    pub rho: Option<String>,
    pub implied_volatility: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct IvComparison {
    pub provider_iv: String,
    pub solved_from_market_price_iv: String,
    pub diff: String,
    pub reference_price: String,
}

#[derive(Debug, Serialize)]
pub struct OptionAnalysisView {
    pub option_symbol: String,
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub option_price: String,
    pub strike_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub implied_volatility: String,
    pub implied_volatility_source: String,
    pub provider_reported_iv: String,
    pub iv_reference_price: Option<String>,
    pub rate: f64,
    pub rate_source: String,
    pub dividend: f64,
    pub option_type: ContractSide,
    pub diagnostics: ContractDiagnostics,
    pub local_greeks: OptionMetrics,
    pub provider_greeks: Option<ProviderGreeks>,
    pub iv_comparison: Option<IvComparison>,
}

#[derive(Debug, Serialize)]
pub struct ChainAnalysisRow {
    pub option_symbol: String,
    pub option_type: ContractSide,
    pub option_price: String,
    pub volume: i64,
    pub open_interest: i64,
    pub strike_price: String,
    pub implied_volatility: String,
    pub implied_volatility_source: String,
    pub provider_reported_iv: String,
    pub diagnostics: ContractDiagnostics,
    pub local_greeks: OptionMetrics,
}

#[derive(Debug, Serialize)]
pub struct ChainAnalysisView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub rows: Vec<ChainAnalysisRow>,
}

#[derive(Debug, Serialize)]
pub struct SkewLegPoint {
    pub option_symbol: String,
    pub strike_price: String,
    pub delta: f64,
    pub otm_percent: f64,
    pub implied_volatility: f64,
}

#[derive(Debug, Serialize)]
pub struct SkewSignalView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub target_delta: f64,
    pub target_otm_percent: f64,
    pub atm_strike_price: String,
    pub atm_iv: f64,
    pub delta_put: Option<SkewLegPoint>,
    pub delta_call: Option<SkewLegPoint>,
    pub delta_skew: Option<f64>,
    pub delta_put_wing_vs_atm: Option<f64>,
    pub delta_call_wing_vs_atm: Option<f64>,
    pub otm_put: Option<SkewLegPoint>,
    pub otm_call: Option<SkewLegPoint>,
    pub otm_skew: Option<f64>,
    pub otm_put_wing_vs_atm: Option<f64>,
    pub otm_call_wing_vs_atm: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct TermStructurePoint {
    pub expiry: String,
    pub days_to_expiry: i64,
    pub atm_strike_price: String,
    pub atm_call_iv: Option<f64>,
    pub atm_put_iv: Option<f64>,
    pub atm_iv: f64,
    pub iv_change_from_prev: Option<f64>,
    pub iv_change_from_front: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct TermStructureView {
    pub underlying_symbol: String,
    pub target_expiries: usize,
    pub points: Vec<TermStructurePoint>,
}

#[derive(Debug, Serialize)]
pub struct SmilePoint {
    pub target_otm_percent: f64,
    pub option_symbol: String,
    pub strike_price: String,
    pub delta: f64,
    pub otm_percent: f64,
    pub implied_volatility: f64,
    pub iv_vs_atm: f64,
}

#[derive(Debug, Serialize)]
pub struct SmileSignalView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub atm_strike_price: String,
    pub atm_iv: f64,
    pub put_points: Vec<SmilePoint>,
    pub call_points: Vec<SmilePoint>,
    pub put_wing_slope: Option<f64>,
    pub call_wing_slope: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct PutCallSideTotals {
    pub contracts: usize,
    pub total_volume: i64,
    pub total_open_interest: i64,
    pub average_iv: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct PutCallBiasView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub min_otm_percent: f64,
    pub all_puts: PutCallSideTotals,
    pub all_calls: PutCallSideTotals,
    pub otm_puts: PutCallSideTotals,
    pub otm_calls: PutCallSideTotals,
    pub volume_bias_ratio: Option<f64>,
    pub open_interest_bias_ratio: Option<f64>,
    pub otm_volume_bias_ratio: Option<f64>,
    pub otm_open_interest_bias_ratio: Option<f64>,
    pub average_iv_bias: Option<f64>,
    pub otm_average_iv_bias: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct MarketToneSummary {
    pub delta_skew: Option<f64>,
    pub otm_skew: Option<f64>,
    pub front_atm_iv: f64,
    pub farthest_atm_iv: Option<f64>,
    pub term_structure_change_from_front: Option<f64>,
    pub put_wing_slope: Option<f64>,
    pub call_wing_slope: Option<f64>,
    pub open_interest_bias_ratio: Option<f64>,
    pub otm_open_interest_bias_ratio: Option<f64>,
    pub average_iv_bias: Option<f64>,
    pub otm_average_iv_bias: Option<f64>,
    pub downside_protection: String,
    pub term_structure_shape: String,
    pub wing_shape: String,
    pub positioning_bias: String,
    pub overall_tone: String,
    pub summary_sentence: String,
}

#[derive(Debug, Serialize)]
pub struct MarketToneView {
    pub underlying_symbol: String,
    pub front_expiry: String,
    pub summary: MarketToneSummary,
    pub skew: SkewSignalView,
    pub smile: SmileSignalView,
    pub put_call_bias: PutCallBiasView,
    pub term_structure: TermStructureView,
}

#[derive(Debug, Serialize)]
pub struct CashSecuredPutCandidate {
    pub option_symbol: String,
    pub strike_price: String,
    pub option_price: String,
    pub implied_volatility: String,
    pub provider_reported_iv: String,
    pub days_to_expiry: i64,
    pub delta: f64,
    pub theta_per_day: f64,
    pub otm_percent: f64,
    pub breakeven: f64,
    pub premium_per_contract: f64,
    pub cash_required_per_contract: f64,
    pub return_on_cash: f64,
    pub annualized_return_on_cash: f64,
    pub fair_value: f64,
    pub mispricing: f64,
    pub mispricing_percent: f64,
    pub solved_iv_from_market_price: f64,
    pub iv_diff: f64,
    pub iv_diff_percent: f64,
    pub diagnostics: ContractDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct CashSecuredPutView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<CashSecuredPutCandidate>,
}

#[derive(Debug, Serialize)]
pub struct CoveredCallCandidate {
    pub option_symbol: String,
    pub strike_price: String,
    pub option_price: String,
    pub implied_volatility: String,
    pub provider_reported_iv: String,
    pub days_to_expiry: i64,
    pub delta: f64,
    pub theta_per_day: f64,
    pub otm_percent: f64,
    pub breakeven: f64,
    pub premium_per_contract: f64,
    pub premium_yield_on_underlying: f64,
    pub annualized_premium_yield: f64,
    pub max_sale_value_per_contract: f64,
    pub max_profit_per_contract: f64,
    pub fair_value: f64,
    pub mispricing: f64,
    pub mispricing_percent: f64,
    pub solved_iv_from_market_price: f64,
    pub iv_diff: f64,
    pub iv_diff_percent: f64,
    pub diagnostics: ContractDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct CoveredCallView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<CoveredCallCandidate>,
}

#[derive(Debug, Serialize)]
pub struct BullPutSpreadCandidate {
    pub short_option_symbol: String,
    pub long_option_symbol: String,
    pub short_strike_price: String,
    pub long_strike_price: String,
    pub short_option_price: String,
    pub long_option_price: String,
    pub short_provider_reported_iv: String,
    pub days_to_expiry: i64,
    pub short_delta: f64,
    pub short_otm_percent: f64,
    pub short_fair_value: f64,
    pub short_mispricing: f64,
    pub short_mispricing_percent: f64,
    pub short_solved_iv_from_market_price: f64,
    pub short_iv_diff: f64,
    pub short_iv_diff_percent: f64,
    pub width_per_spread: f64,
    pub net_credit_per_spread: f64,
    pub max_profit_per_spread: f64,
    pub max_loss_per_spread: f64,
    pub breakeven: f64,
    pub return_on_risk: f64,
    pub annualized_return_on_risk: f64,
    pub short_diagnostics: ContractDiagnostics,
    pub long_diagnostics: ContractDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct BullPutSpreadView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<BullPutSpreadCandidate>,
}

#[derive(Debug, Serialize)]
pub struct BearCallSpreadCandidate {
    pub short_option_symbol: String,
    pub long_option_symbol: String,
    pub short_strike_price: String,
    pub long_strike_price: String,
    pub short_option_price: String,
    pub long_option_price: String,
    pub short_provider_reported_iv: String,
    pub days_to_expiry: i64,
    pub short_delta: f64,
    pub short_otm_percent: f64,
    pub short_fair_value: f64,
    pub short_mispricing: f64,
    pub short_mispricing_percent: f64,
    pub short_solved_iv_from_market_price: f64,
    pub short_iv_diff: f64,
    pub short_iv_diff_percent: f64,
    pub width_per_spread: f64,
    pub net_credit_per_spread: f64,
    pub max_profit_per_spread: f64,
    pub max_loss_per_spread: f64,
    pub breakeven: f64,
    pub return_on_risk: f64,
    pub annualized_return_on_risk: f64,
    pub short_diagnostics: ContractDiagnostics,
    pub long_diagnostics: ContractDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct BearCallSpreadView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<BearCallSpreadCandidate>,
}

#[derive(Debug, Serialize)]
pub struct BearPutSpreadCandidate {
    pub long_option_symbol: String,
    pub short_option_symbol: String,
    pub long_strike_price: String,
    pub short_strike_price: String,
    pub long_option_price: String,
    pub short_option_price: String,
    pub days_to_expiry: i64,
    pub long_delta: f64,
    pub long_otm_percent: f64,
    pub width_per_spread: f64,
    pub net_debit_per_spread: f64,
    pub max_profit_per_spread: f64,
    pub max_loss_per_spread: f64,
    pub breakeven: f64,
    pub return_on_risk: f64,
    pub annualized_return_on_risk: f64,
    pub long_diagnostics: ContractDiagnostics,
    pub short_diagnostics: ContractDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct BearPutSpreadView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<BearPutSpreadCandidate>,
}

#[derive(Debug, Serialize)]
pub struct BullCallSpreadCandidate {
    pub long_option_symbol: String,
    pub short_option_symbol: String,
    pub long_strike_price: String,
    pub short_strike_price: String,
    pub long_option_price: String,
    pub short_option_price: String,
    pub days_to_expiry: i64,
    pub long_delta: f64,
    pub long_otm_percent: f64,
    pub width_per_spread: f64,
    pub net_debit_per_spread: f64,
    pub max_profit_per_spread: f64,
    pub max_loss_per_spread: f64,
    pub breakeven: f64,
    pub return_on_risk: f64,
    pub annualized_return_on_risk: f64,
    pub long_diagnostics: ContractDiagnostics,
    pub short_diagnostics: ContractDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct BullCallSpreadView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<BullCallSpreadCandidate>,
}

#[derive(Debug, Serialize)]
pub struct CalendarCallSpreadCandidate {
    pub near_option_symbol: String,
    pub far_option_symbol: String,
    pub strike_price: String,
    pub near_expiry: String,
    pub far_expiry: String,
    pub days_gap: i64,
    pub strike_gap: f64,
    pub near_option_price: String,
    pub far_option_price: String,
    pub net_debit_per_spread: f64,
    pub net_theta_carry_per_day: f64,
    pub theta_carry_return_on_debit_per_day: f64,
    pub annualized_theta_carry_return_on_debit: f64,
    pub net_vega: f64,
    pub vega_to_debit_ratio: f64,
    pub max_loss_per_spread: f64,
    pub near_diagnostics: ContractDiagnostics,
    pub far_diagnostics: ContractDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct CalendarCallSpreadView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub near_expiry: String,
    pub far_expiry: String,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<CalendarCallSpreadCandidate>,
}

#[derive(Debug, Serialize)]
pub struct CalendarPutSpreadCandidate {
    pub near_option_symbol: String,
    pub far_option_symbol: String,
    pub strike_price: String,
    pub near_expiry: String,
    pub far_expiry: String,
    pub days_gap: i64,
    pub strike_gap: f64,
    pub near_option_price: String,
    pub far_option_price: String,
    pub net_debit_per_spread: f64,
    pub net_theta_carry_per_day: f64,
    pub theta_carry_return_on_debit_per_day: f64,
    pub annualized_theta_carry_return_on_debit: f64,
    pub net_vega: f64,
    pub vega_to_debit_ratio: f64,
    pub max_loss_per_spread: f64,
    pub near_diagnostics: ContractDiagnostics,
    pub far_diagnostics: ContractDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct CalendarPutSpreadView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub near_expiry: String,
    pub far_expiry: String,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<CalendarPutSpreadCandidate>,
}

#[derive(Debug, Serialize)]
pub struct DiagonalCallSpreadCandidate {
    pub near_option_symbol: String,
    pub far_option_symbol: String,
    pub near_strike_price: String,
    pub far_strike_price: String,
    pub near_expiry: String,
    pub far_expiry: String,
    pub days_gap: i64,
    pub strike_gap: f64,
    pub near_option_price: String,
    pub far_option_price: String,
    pub net_debit_per_spread: f64,
    pub net_theta_carry_per_day: f64,
    pub theta_carry_return_on_debit_per_day: f64,
    pub annualized_theta_carry_return_on_debit: f64,
    pub net_vega: f64,
    pub vega_to_debit_ratio: f64,
    pub max_loss_per_spread: f64,
    pub near_diagnostics: ContractDiagnostics,
    pub far_diagnostics: ContractDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct DiagonalCallSpreadView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub near_expiry: String,
    pub far_expiry: String,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<DiagonalCallSpreadCandidate>,
}

#[derive(Debug, Serialize)]
pub struct DiagonalPutSpreadCandidate {
    pub near_option_symbol: String,
    pub far_option_symbol: String,
    pub near_strike_price: String,
    pub far_strike_price: String,
    pub near_expiry: String,
    pub far_expiry: String,
    pub days_gap: i64,
    pub strike_gap: f64,
    pub near_option_price: String,
    pub far_option_price: String,
    pub net_debit_per_spread: f64,
    pub net_theta_carry_per_day: f64,
    pub theta_carry_return_on_debit_per_day: f64,
    pub annualized_theta_carry_return_on_debit: f64,
    pub net_vega: f64,
    pub vega_to_debit_ratio: f64,
    pub max_loss_per_spread: f64,
    pub near_diagnostics: ContractDiagnostics,
    pub far_diagnostics: ContractDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct DiagonalPutSpreadView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub near_expiry: String,
    pub far_expiry: String,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<DiagonalPutSpreadCandidate>,
}

#[derive(Debug, Serialize)]
pub struct SellOpportunityCandidate {
    pub strategy: String,
    pub primary_symbol: String,
    pub secondary_symbol: Option<String>,
    pub annualized_return: f64,
    pub premium_or_credit: f64,
    pub max_risk: f64,
    pub breakeven: f64,
    pub mispricing_percent: f64,
    pub iv_diff_percent: f64,
}

#[derive(Debug, Serialize)]
pub struct SellOpportunitiesView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<SellOpportunityCandidate>,
}

#[derive(Debug, Serialize)]
pub struct MispricingCandidate {
    pub option_symbol: String,
    pub option_type: ContractSide,
    pub strike_price: String,
    pub option_price: String,
    pub provider_reported_iv: String,
    pub solved_iv_from_market_price: f64,
    pub iv_diff: f64,
    pub iv_diff_percent: f64,
    pub fair_value: f64,
    pub mispricing: f64,
    pub mispricing_percent: f64,
    pub diagnostics: ContractDiagnostics,
    pub local_greeks: OptionMetrics,
}

#[derive(Debug, Serialize)]
pub struct MispricingView {
    pub underlying_symbol: String,
    pub underlying_price: String,
    pub expiry: String,
    pub days_to_expiry: i64,
    pub rate: f64,
    pub rate_source: String,
    pub candidates: Vec<MispricingCandidate>,
}

pub fn normalize_option_chain(chain: OptionChainSnapshot) -> NormalizedOptionChainSnapshot {
    let underlying_spot = chain.underlying.last_done_f64;
    let days_to_expiry = chain.days_to_expiry;
    let mut contracts = chain.contracts;
    contracts.sort_by(|left, right| {
        left.strike_price_f64
            .partial_cmp(&right.strike_price_f64)
            .unwrap_or(Ordering::Equal)
            .then_with(|| match (&left.option_type, &right.option_type) {
                (ContractSide::Call, ContractSide::Put) => Ordering::Less,
                (ContractSide::Put, ContractSide::Call) => Ordering::Greater,
                _ => Ordering::Equal,
            })
    });

    let mut rows: Vec<OptionChainStrikeRow> = Vec::new();
    for contract in contracts {
        if let Some(last_row) = rows.last_mut() {
            if last_row.strike_price == contract.strike_price {
                match contract.option_type {
                    ContractSide::Call => {
                        last_row.call_diagnostics = Some(crate::diagnostics::analyze_contract(
                            &contract,
                            underlying_spot,
                            days_to_expiry,
                        ));
                        last_row.call = Some(contract);
                    }
                    ContractSide::Put => {
                        last_row.put_diagnostics = Some(crate::diagnostics::analyze_contract(
                            &contract,
                            underlying_spot,
                            days_to_expiry,
                        ));
                        last_row.put = Some(contract);
                    }
                }
                continue;
            }
        }

        let strike_price = contract.strike_price.clone();
        let strike_price_f64 = contract.strike_price_f64;
        let diagnostics =
            crate::diagnostics::analyze_contract(&contract, underlying_spot, days_to_expiry);
        let (call, call_diagnostics, put, put_diagnostics) = match contract.option_type {
            ContractSide::Call => (Some(contract), Some(diagnostics), None, None),
            ContractSide::Put => (None, None, Some(contract), Some(diagnostics)),
        };
        rows.push(OptionChainStrikeRow {
            strike_price,
            strike_price_f64,
            call,
            call_diagnostics,
            put,
            put_diagnostics,
        });
    }

    NormalizedOptionChainSnapshot {
        underlying: chain.underlying,
        expiry: chain.expiry,
        days_to_expiry: chain.days_to_expiry,
        rows,
    }
}
