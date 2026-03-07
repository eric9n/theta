use crate::analysis_service::{AnalyzeChainRequest, AnalyzeOptionRequest, ThetaAnalysisService};
use crate::analytics::{ContractSide, OptionMetrics, PricingInput, calculate_metrics};
use crate::diagnostics::{
    NormalizedChainDiagnosticsFilter, apply_normalized_chain_diagnostics_filter,
};
use crate::domain::{
    ChainAnalysisView, NormalizedOptionChainSnapshot, OptionAnalysisView, OptionContractSnapshot,
    SellOpportunityReturnBasis, normalize_option_chain,
};
use crate::market_data::parse_expiry_date;
use crate::screening_service::{ChainScreeningRequest, ChainSortField};
use crate::strategy_service::{
    BearCallSpreadRequest, BearPutSpreadRequest, BullCallSpreadRequest, BullPutSpreadRequest,
    CalendarCallSpreadRequest, CalendarPutSpreadRequest, CashSecuredPutRequest, CoveredCallRequest,
    CrossExpirySortField, DiagonalCallSpreadRequest, DiagonalPutSpreadRequest, IvDiffDirection,
    MispricingDirection, MispricingRequest, MispricingSortField, SellOpportunitiesRequest,
    SellOpportunitySortField, SellOpportunityStrategy, ThetaStrategyService,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Args, Debug)]
#[command(name = "snapshot")]
#[command(about = "CLI for market snapshots, option analytics, and strategy screening")]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Calculate Black-Scholes metrics from explicit inputs
    Calc {
        #[arg(long)]
        spot: f64,
        #[arg(long)]
        strike: f64,
        #[arg(long, help = "Annualized risk-free rate, e.g. 0.03 for 3%")]
        rate: f64,
        #[arg(long, help = "Annualized implied volatility, e.g. 0.25 for 25%")]
        volatility: f64,
        #[arg(long, help = "Days remaining until expiration")]
        days: f64,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(long, value_enum)]
        option_type: ContractSide,
        #[arg(long)]
        json: bool,
    },
    /// Fetch realtime quote for a stock
    StockQuote {
        #[arg(long)]
        symbol: String,
    },
    /// List available option expiries for an underlying
    OptionExpiries {
        #[arg(long)]
        symbol: String,
    },
    /// Fetch an option chain with option quotes for one expiry
    OptionChain {
        #[arg(long)]
        symbol: String,
        #[arg(long, help = "Expiry date in YYYY-MM-DD")]
        expiry: String,
        #[arg(long, help = "Only keep legs that pass the basic liquidity check")]
        only_liquid: bool,
        #[arg(long, help = "Exclude abnormal trade status or non-standard contracts")]
        exclude_abnormal: bool,
        #[arg(long, help = "Exclude contracts that are near expiry (<= 1 day)")]
        exclude_near_expiry: bool,
        #[arg(long)]
        json: bool,
    },
    /// Fetch a single option quote with full provider fields
    OptionQuote {
        #[arg(long, help = "Option symbol, e.g. TSLA260320C00400000.US")]
        symbol: String,
        #[arg(long)]
        json: bool,
    },
    /// Analyze a single option contract with locally computed Greeks
    AnalyzeOption {
        #[arg(long, help = "Option symbol, e.g. AAPL250117C00200000.US")]
        symbol: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override, e.g. 0.35"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from a manually supplied option price")]
        iv_from_option_price: Option<f64>,
        #[arg(long, help = "Solve IV from the provider option last_done price")]
        iv_from_market_price: bool,
        #[arg(long, help = "Show provider IV vs market-price-solved IV comparison")]
        show_iv_diff: bool,
        #[arg(long, help = "Also fetch provider Greeks from calc_indexes")]
        use_provider_greeks: bool,
        #[arg(long)]
        json: bool,
    },
    /// Analyze an entire option chain for one expiry with locally computed Greeks
    AnalyzeChain {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Expiry date in YYYY-MM-DD")]
        expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(long, value_enum, help = "Filter to only calls or puts")]
        side: Option<ContractSide>,
        #[arg(long, help = "Minimum strike price filter")]
        min_strike: Option<f64>,
        #[arg(long, help = "Maximum strike price filter")]
        max_strike: Option<f64>,
        #[arg(long, help = "Minimum delta filter")]
        min_delta: Option<f64>,
        #[arg(long, help = "Maximum delta filter")]
        max_delta: Option<f64>,
        #[arg(long, help = "Minimum theta/day filter")]
        min_theta: Option<f64>,
        #[arg(long, help = "Maximum theta/day filter")]
        max_theta: Option<f64>,
        #[arg(long, help = "Minimum vega filter")]
        min_vega: Option<f64>,
        #[arg(long, help = "Maximum vega filter")]
        max_vega: Option<f64>,
        #[arg(long, help = "Minimum IV filter")]
        min_iv: Option<f64>,
        #[arg(long, help = "Maximum IV filter")]
        max_iv: Option<f64>,
        #[arg(long, help = "Minimum option price filter")]
        min_option_price: Option<f64>,
        #[arg(long, help = "Maximum option price filter")]
        max_option_price: Option<f64>,
        #[arg(
            long,
            help = "Minimum out-of-the-money percentage filter, e.g. 0.05 = 5%"
        )]
        min_otm_percent: Option<f64>,
        #[arg(
            long,
            help = "Maximum out-of-the-money percentage filter, e.g. 0.10 = 10%"
        )]
        max_otm_percent: Option<f64>,
        #[arg(long, help = "Only keep contracts that pass the basic liquidity check")]
        only_liquid: bool,
        #[arg(long, help = "Exclude abnormal trade status or non-standard contracts")]
        exclude_abnormal: bool,
        #[arg(long, help = "Exclude contracts that are near expiry (<= 1 day)")]
        exclude_near_expiry: bool,
        #[arg(long, value_enum, help = "Sort output rows")]
        sort_by: Option<ChainSortField>,
        #[arg(long, help = "Maximum number of rows to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen cash-secured put candidates
    CashSecuredPut {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Expiry date in YYYY-MM-DD")]
        expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(
            long,
            value_enum,
            help = "Only keep puts that are overpriced or underpriced versus fair value"
        )]
        direction: Option<MispricingDirection>,
        #[arg(
            long,
            value_enum,
            help = "Only keep puts where solved IV is higher or lower than provider IV"
        )]
        iv_direction: Option<IvDiffDirection>,
        #[arg(long, help = "Minimum delta filter, e.g. -0.30")]
        min_delta: Option<f64>,
        #[arg(long, help = "Maximum delta filter, e.g. -0.10")]
        max_delta: Option<f64>,
        #[arg(
            long,
            help = "Minimum out-of-the-money percentage filter, e.g. 0.05 = 5%"
        )]
        min_otm_percent: Option<f64>,
        #[arg(
            long,
            help = "Maximum out-of-the-money percentage filter, e.g. 0.12 = 12%"
        )]
        max_otm_percent: Option<f64>,
        #[arg(long, help = "Minimum option premium filter")]
        min_option_price: Option<f64>,
        #[arg(long, help = "Maximum option premium filter")]
        max_option_price: Option<f64>,
        #[arg(long, help = "Minimum open interest")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume")]
        min_volume: Option<i64>,
        #[arg(long, help = "Minimum premium per contract, in dollars")]
        min_premium_per_contract: Option<f64>,
        #[arg(long, help = "Maximum cash required per contract, in dollars")]
        max_cash_required_per_contract: Option<f64>,
        #[arg(long, help = "Minimum annualized return on cash, e.g. 0.12 = 12%")]
        min_annualized_return_on_cash: Option<f64>,
        #[arg(long, help = "Maximum annualized return on cash")]
        max_annualized_return_on_cash: Option<f64>,
        #[arg(long, help = "Minimum absolute mispricing percent, e.g. 0.10 = 10%")]
        min_abs_mispricing_percent: Option<f64>,
        #[arg(long, help = "Minimum absolute IV diff percent, e.g. 0.10 = 10%")]
        min_abs_iv_diff_percent: Option<f64>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen covered call candidates
    CoveredCall {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Expiry date in YYYY-MM-DD")]
        expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(
            long,
            value_enum,
            help = "Only keep calls that are overpriced or underpriced versus fair value"
        )]
        direction: Option<MispricingDirection>,
        #[arg(
            long,
            value_enum,
            help = "Only keep calls where solved IV is higher or lower than provider IV"
        )]
        iv_direction: Option<IvDiffDirection>,
        #[arg(long, help = "Minimum delta filter, e.g. 0.10")]
        min_delta: Option<f64>,
        #[arg(long, help = "Maximum delta filter, e.g. 0.35")]
        max_delta: Option<f64>,
        #[arg(
            long,
            help = "Minimum out-of-the-money percentage filter, e.g. 0.03 = 3%"
        )]
        min_otm_percent: Option<f64>,
        #[arg(
            long,
            help = "Maximum out-of-the-money percentage filter, e.g. 0.10 = 10%"
        )]
        max_otm_percent: Option<f64>,
        #[arg(long, help = "Minimum option premium filter")]
        min_option_price: Option<f64>,
        #[arg(long, help = "Maximum option premium filter")]
        max_option_price: Option<f64>,
        #[arg(long, help = "Minimum open interest")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume")]
        min_volume: Option<i64>,
        #[arg(long, help = "Minimum premium per contract, in dollars")]
        min_premium_per_contract: Option<f64>,
        #[arg(long, help = "Minimum annualized premium yield, e.g. 0.08 = 8%")]
        min_annualized_premium_yield: Option<f64>,
        #[arg(long, help = "Maximum annualized premium yield")]
        max_annualized_premium_yield: Option<f64>,
        #[arg(long, help = "Minimum absolute mispricing percent, e.g. 0.10 = 10%")]
        min_abs_mispricing_percent: Option<f64>,
        #[arg(long, help = "Minimum absolute IV diff percent, e.g. 0.10 = 10%")]
        min_abs_iv_diff_percent: Option<f64>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen bull put spread candidates
    BullPutSpread {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Expiry date in YYYY-MM-DD")]
        expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(
            long,
            value_enum,
            help = "Only keep spreads whose short put is overpriced or underpriced versus fair value"
        )]
        direction: Option<MispricingDirection>,
        #[arg(
            long,
            value_enum,
            help = "Only keep spreads whose short put has solved IV higher or lower than provider IV"
        )]
        iv_direction: Option<IvDiffDirection>,
        #[arg(long, help = "Minimum short put delta, e.g. -0.35")]
        min_short_delta: Option<f64>,
        #[arg(long, help = "Maximum short put delta, e.g. -0.10")]
        max_short_delta: Option<f64>,
        #[arg(long, help = "Minimum short leg OTM percentage")]
        min_short_otm_percent: Option<f64>,
        #[arg(long, help = "Maximum short leg OTM percentage")]
        max_short_otm_percent: Option<f64>,
        #[arg(long, help = "Minimum open interest per leg")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume per leg")]
        min_volume: Option<i64>,
        #[arg(long, help = "Minimum net credit per spread, in dollars")]
        min_net_credit_per_spread: Option<f64>,
        #[arg(long, help = "Minimum spread width, in dollars")]
        min_width_per_spread: Option<f64>,
        #[arg(long, help = "Maximum spread width, in dollars")]
        max_width_per_spread: Option<f64>,
        #[arg(long, help = "Minimum annualized return on risk")]
        min_annualized_return_on_risk: Option<f64>,
        #[arg(long, help = "Maximum annualized return on risk")]
        max_annualized_return_on_risk: Option<f64>,
        #[arg(
            long,
            help = "Minimum absolute short-leg mispricing percent, e.g. 0.10 = 10%"
        )]
        min_abs_mispricing_percent: Option<f64>,
        #[arg(
            long,
            help = "Minimum absolute short-leg IV diff percent, e.g. 0.10 = 10%"
        )]
        min_abs_iv_diff_percent: Option<f64>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen bear call spread candidates
    BearCallSpread {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Expiry date in YYYY-MM-DD")]
        expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(
            long,
            value_enum,
            help = "Only keep spreads whose short call is overpriced or underpriced versus fair value"
        )]
        direction: Option<MispricingDirection>,
        #[arg(
            long,
            value_enum,
            help = "Only keep spreads whose short call has solved IV higher or lower than provider IV"
        )]
        iv_direction: Option<IvDiffDirection>,
        #[arg(long, help = "Minimum short call delta, e.g. 0.10")]
        min_short_delta: Option<f64>,
        #[arg(long, help = "Maximum short call delta, e.g. 0.35")]
        max_short_delta: Option<f64>,
        #[arg(long, help = "Minimum short leg OTM percentage")]
        min_short_otm_percent: Option<f64>,
        #[arg(long, help = "Maximum short leg OTM percentage")]
        max_short_otm_percent: Option<f64>,
        #[arg(long, help = "Minimum open interest per leg")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume per leg")]
        min_volume: Option<i64>,
        #[arg(long, help = "Minimum net credit per spread, in dollars")]
        min_net_credit_per_spread: Option<f64>,
        #[arg(long, help = "Minimum spread width, in dollars")]
        min_width_per_spread: Option<f64>,
        #[arg(long, help = "Maximum spread width, in dollars")]
        max_width_per_spread: Option<f64>,
        #[arg(long, help = "Minimum annualized return on risk")]
        min_annualized_return_on_risk: Option<f64>,
        #[arg(long, help = "Maximum annualized return on risk")]
        max_annualized_return_on_risk: Option<f64>,
        #[arg(
            long,
            help = "Minimum absolute short-leg mispricing percent, e.g. 0.10 = 10%"
        )]
        min_abs_mispricing_percent: Option<f64>,
        #[arg(
            long,
            help = "Minimum absolute short-leg IV diff percent, e.g. 0.10 = 10%"
        )]
        min_abs_iv_diff_percent: Option<f64>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen bear put spread candidates
    BearPutSpread {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Expiry date in YYYY-MM-DD")]
        expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(long, help = "Minimum open interest per leg")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume per leg")]
        min_volume: Option<i64>,
        #[arg(long, help = "Maximum net debit per spread, in dollars")]
        max_net_debit_per_spread: Option<f64>,
        #[arg(long, help = "Minimum spread width, in dollars")]
        min_width_per_spread: Option<f64>,
        #[arg(long, help = "Maximum spread width, in dollars")]
        max_width_per_spread: Option<f64>,
        #[arg(long, help = "Minimum annualized return on risk")]
        min_annualized_return_on_risk: Option<f64>,
        #[arg(long, help = "Maximum annualized return on risk")]
        max_annualized_return_on_risk: Option<f64>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen bull call spread candidates
    BullCallSpread {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Expiry date in YYYY-MM-DD")]
        expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(long, help = "Minimum open interest per leg")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume per leg")]
        min_volume: Option<i64>,
        #[arg(long, help = "Maximum net debit per spread, in dollars")]
        max_net_debit_per_spread: Option<f64>,
        #[arg(long, help = "Minimum spread width, in dollars")]
        min_width_per_spread: Option<f64>,
        #[arg(long, help = "Maximum spread width, in dollars")]
        max_width_per_spread: Option<f64>,
        #[arg(long, help = "Minimum annualized return on risk")]
        min_annualized_return_on_risk: Option<f64>,
        #[arg(long, help = "Maximum annualized return on risk")]
        max_annualized_return_on_risk: Option<f64>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen calendar call spread candidates (short near expiry, long far expiry)
    CalendarCallSpread {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Near expiry date in YYYY-MM-DD")]
        near_expiry: String,
        #[arg(long, help = "Far expiry date in YYYY-MM-DD")]
        far_expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(long, help = "Minimum open interest per leg")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume per leg")]
        min_volume: Option<i64>,
        #[arg(long, help = "Maximum net debit per spread, in dollars")]
        max_net_debit_per_spread: Option<f64>,
        #[arg(long, help = "Minimum net theta carry per day")]
        min_net_theta_carry_per_day: Option<f64>,
        #[arg(long, help = "Minimum days gap between far and near expiry")]
        min_days_gap: Option<i64>,
        #[arg(long, help = "Maximum days gap between far and near expiry")]
        max_days_gap: Option<i64>,
        #[arg(long, help = "Minimum strike gap, in dollars")]
        min_strike_gap: Option<f64>,
        #[arg(long, help = "Maximum strike gap, in dollars")]
        max_strike_gap: Option<f64>,
        #[arg(long, help = "Sort candidates by carry|theta|vega-to-debit|debit")]
        sort_by: Option<CrossExpirySortField>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen calendar put spread candidates (short near expiry, long far expiry)
    CalendarPutSpread {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Near expiry date in YYYY-MM-DD")]
        near_expiry: String,
        #[arg(long, help = "Far expiry date in YYYY-MM-DD")]
        far_expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(long, help = "Minimum open interest per leg")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume per leg")]
        min_volume: Option<i64>,
        #[arg(long, help = "Maximum net debit per spread, in dollars")]
        max_net_debit_per_spread: Option<f64>,
        #[arg(long, help = "Minimum net theta carry per day")]
        min_net_theta_carry_per_day: Option<f64>,
        #[arg(long, help = "Minimum days gap between far and near expiry")]
        min_days_gap: Option<i64>,
        #[arg(long, help = "Maximum days gap between far and near expiry")]
        max_days_gap: Option<i64>,
        #[arg(long, help = "Minimum strike gap, in dollars")]
        min_strike_gap: Option<f64>,
        #[arg(long, help = "Maximum strike gap, in dollars")]
        max_strike_gap: Option<f64>,
        #[arg(long, help = "Sort candidates by carry|theta|vega-to-debit|debit")]
        sort_by: Option<CrossExpirySortField>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen diagonal call spread candidates (short near expiry call, long farther expiry lower strike call)
    DiagonalCallSpread {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Near expiry date in YYYY-MM-DD")]
        near_expiry: String,
        #[arg(long, help = "Far expiry date in YYYY-MM-DD")]
        far_expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(long, help = "Minimum open interest per leg")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume per leg")]
        min_volume: Option<i64>,
        #[arg(long, help = "Maximum net debit per spread, in dollars")]
        max_net_debit_per_spread: Option<f64>,
        #[arg(long, help = "Minimum net theta carry per day")]
        min_net_theta_carry_per_day: Option<f64>,
        #[arg(long, help = "Minimum days gap between far and near expiry")]
        min_days_gap: Option<i64>,
        #[arg(long, help = "Maximum days gap between far and near expiry")]
        max_days_gap: Option<i64>,
        #[arg(long, help = "Minimum strike gap, in dollars")]
        min_strike_gap: Option<f64>,
        #[arg(long, help = "Maximum strike gap, in dollars")]
        max_strike_gap: Option<f64>,
        #[arg(long, help = "Sort candidates by carry|theta|vega-to-debit|debit")]
        sort_by: Option<CrossExpirySortField>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen diagonal put spread candidates (short near expiry put, long farther expiry higher strike put)
    DiagonalPutSpread {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Near expiry date in YYYY-MM-DD")]
        near_expiry: String,
        #[arg(long, help = "Far expiry date in YYYY-MM-DD")]
        far_expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(long, help = "Minimum open interest per leg")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume per leg")]
        min_volume: Option<i64>,
        #[arg(long, help = "Maximum net debit per spread, in dollars")]
        max_net_debit_per_spread: Option<f64>,
        #[arg(long, help = "Minimum net theta carry per day")]
        min_net_theta_carry_per_day: Option<f64>,
        #[arg(long, help = "Minimum days gap between far and near expiry")]
        min_days_gap: Option<i64>,
        #[arg(long, help = "Maximum days gap between far and near expiry")]
        max_days_gap: Option<i64>,
        #[arg(long, help = "Minimum strike gap, in dollars")]
        min_strike_gap: Option<f64>,
        #[arg(long, help = "Maximum strike gap, in dollars")]
        max_strike_gap: Option<f64>,
        #[arg(long, help = "Sort candidates by carry|theta|vega-to-debit|debit")]
        sort_by: Option<CrossExpirySortField>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Screen option chain mispricing and IV divergence
    Mispricing {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Expiry date in YYYY-MM-DD")]
        expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(long, value_enum, help = "Filter to only calls or puts")]
        side: Option<ContractSide>,
        #[arg(
            long,
            value_enum,
            help = "Only show overpriced or underpriced contracts"
        )]
        direction: Option<MispricingDirection>,
        #[arg(
            long,
            value_enum,
            help = "Only show rows where solved IV is higher or lower than provider IV"
        )]
        iv_direction: Option<IvDiffDirection>,
        #[arg(long, help = "Minimum open interest")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume")]
        min_volume: Option<i64>,
        #[arg(long, help = "Minimum absolute mispricing percent, e.g. 0.10 = 10%")]
        min_abs_mispricing_percent: Option<f64>,
        #[arg(long, help = "Minimum absolute IV diff percent, e.g. 0.10 = 10%")]
        min_abs_iv_diff_percent: Option<f64>,
        #[arg(
            long,
            value_enum,
            help = "Sort by absolute mispricing or absolute iv_diff"
        )]
        sort_by: Option<MispricingSortField>,
        #[arg(
            long,
            help = "Render only summary metrics without listing individual candidates"
        )]
        summary_only: bool,
        #[arg(long, help = "Show side-level summary buckets for calls and puts")]
        group_by_side: bool,
        #[arg(long, help = "Show at most N rows per side in text output")]
        top_per_side: Option<usize>,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    /// Rank sell-side opportunities across built-in strategies
    SellOpportunities {
        #[arg(long, help = "Underlying symbol, e.g. TSLA.US")]
        symbol: String,
        #[arg(long, help = "Expiry date in YYYY-MM-DD")]
        expiry: String,
        #[arg(
            long,
            help = "Annualized risk-free rate; if omitted, uses default tenor-based mapping"
        )]
        rate: Option<f64>,
        #[arg(long, default_value_t = 0.0, help = "Annualized dividend yield")]
        dividend: f64,
        #[arg(
            long,
            help = "Manual annualized implied volatility override for all contracts"
        )]
        iv: Option<f64>,
        #[arg(long, help = "Solve IV from each option's provider last_done price")]
        iv_from_market_price: bool,
        #[arg(
            long,
            value_enum,
            help = "Only keep candidates that are overpriced or underpriced versus fair value"
        )]
        direction: Option<MispricingDirection>,
        #[arg(
            long,
            value_enum,
            help = "Only keep candidates where solved IV is higher or lower than provider IV"
        )]
        iv_direction: Option<IvDiffDirection>,
        #[arg(long, action = clap::ArgAction::SetTrue, help = "Only include calendar spreads among cross-expiry strategies")]
        include_calendars: Option<bool>,
        #[arg(long, action = clap::ArgAction::SetTrue, help = "Only include diagonal spreads among cross-expiry strategies")]
        include_diagonals: Option<bool>,
        #[arg(long, help = "Minimum open interest")]
        min_open_interest: Option<i64>,
        #[arg(long, help = "Minimum volume")]
        min_volume: Option<i64>,
        #[arg(long, help = "Minimum absolute mispricing percent, e.g. 0.10 = 10%")]
        min_abs_mispricing_percent: Option<f64>,
        #[arg(long, help = "Minimum absolute IV diff percent, e.g. 0.10 = 10%")]
        min_abs_iv_diff_percent: Option<f64>,
        #[arg(
            long,
            help = "Minimum days gap for auto-included calendar/diagonal strategies"
        )]
        min_days_gap: Option<i64>,
        #[arg(
            long,
            help = "Maximum days gap for auto-included calendar/diagonal strategies"
        )]
        max_days_gap: Option<i64>,
        #[arg(
            long,
            help = "Minimum strike gap for auto-included calendar/diagonal strategies"
        )]
        min_strike_gap: Option<f64>,
        #[arg(
            long,
            help = "Maximum strike gap for auto-included calendar/diagonal strategies"
        )]
        max_strike_gap: Option<f64>,
        #[arg(
            long,
            value_enum,
            help = "Only include specific strategies; repeat to select multiple"
        )]
        strategy: Vec<SellOpportunityStrategy>,
        #[arg(
            long,
            value_enum,
            help = "Exclude specific strategies; repeat to exclude multiple"
        )]
        exclude_strategy: Vec<SellOpportunityStrategy>,
        #[arg(
            long,
            value_enum,
            help = "Only include specific return bases; repeat to select multiple"
        )]
        return_basis: Vec<SellOpportunityReturnBasis>,
        #[arg(
            long,
            value_enum,
            help = "Exclude specific return bases; repeat to exclude multiple"
        )]
        exclude_return_basis: Vec<SellOpportunityReturnBasis>,
        #[arg(
            long,
            help = "Minimum premium or net credit/debit across merged opportunities"
        )]
        min_premium_or_credit: Option<f64>,
        #[arg(long, help = "Maximum risk across merged opportunities")]
        max_risk: Option<f64>,
        #[arg(long, help = "Minimum annualized return across merged opportunities")]
        min_annualized_return: Option<f64>,
        #[arg(long, help = "Maximum annualized return across merged opportunities")]
        max_annualized_return: Option<f64>,
        #[arg(
            long,
            help = "Maximum number of candidates to keep per strategy before global limit"
        )]
        limit_per_strategy: Option<usize>,
        #[arg(
            long,
            value_enum,
            help = "Sort merged opportunities by annualized_return, mispricing, or iv_diff"
        )]
        sort_by: Option<SellOpportunitySortField>,
        #[arg(long, help = "Render grouped by strategy instead of one merged list")]
        group_by_strategy: bool,
        #[arg(
            long,
            conflicts_with = "group_by_strategy",
            help = "Render grouped by return basis instead of strategy"
        )]
        group_by_return_basis: bool,
        #[arg(
            long,
            help = "Render only summary sections without listing individual candidates"
        )]
        summary_only: bool,
        #[arg(long, help = "Maximum number of candidates to return")]
        limit: Option<usize>,
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Serialize)]
struct StockQuoteView {
    symbol: String,
    last_done: String,
    prev_close: String,
    open: String,
    high: String,
    low: String,
    volume: i64,
    turnover: String,
    timestamp: String,
}

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Calc {
            spot,
            strike,
            rate,
            volatility,
            days,
            dividend,
            option_type,
            json,
        } => {
            let input =
                PricingInput::new(spot, strike, rate, volatility, days, dividend, option_type)?;
            let metrics = calculate_metrics(&input);
            render_metrics(&metrics, json)?;
        }
        Command::StockQuote { symbol } => {
            let service = ThetaAnalysisService::from_env().await?;
            let snapshot = service.market().fetch_underlying(&symbol).await?;
            let quote = StockQuoteView {
                symbol: snapshot.symbol,
                last_done: snapshot.last_done,
                prev_close: snapshot.prev_close,
                open: snapshot.open,
                high: snapshot.high,
                low: snapshot.low,
                volume: snapshot.volume,
                turnover: snapshot.turnover,
                timestamp: snapshot.timestamp,
            };
            println!("{}", serde_json::to_string_pretty(&quote)?);
        }
        Command::OptionExpiries { symbol } => {
            let service = ThetaAnalysisService::from_env().await?;
            let expiries = service.market().fetch_option_expiries(&symbol).await?;
            println!("{}", serde_json::to_string_pretty(&expiries)?);
        }
        Command::OptionChain {
            symbol,
            expiry,
            only_liquid,
            exclude_abnormal,
            exclude_near_expiry,
            json,
        } => {
            let service = ThetaAnalysisService::from_env().await?;
            let chain = service
                .market()
                .fetch_option_chain(&symbol, parse_expiry_date(&expiry)?)
                .await?;
            let mut normalized = normalize_option_chain(chain);
            apply_normalized_chain_diagnostics_filter(
                &mut normalized,
                NormalizedChainDiagnosticsFilter {
                    only_liquid,
                    exclude_abnormal,
                    exclude_near_expiry,
                },
            );
            if json {
                println!("{}", serde_json::to_string_pretty(&normalized)?);
                return Ok(());
            }
            render_option_chain(&normalized, json)?;
        }
        Command::OptionQuote { symbol, json } => {
            let service = ThetaAnalysisService::from_env().await?;
            let quote = service.market().fetch_option_contract(&symbol).await?;
            render_option_quote(&quote, json)?;
        }
        Command::AnalyzeOption {
            symbol,
            rate,
            dividend,
            iv,
            iv_from_option_price,
            iv_from_market_price,
            show_iv_diff,
            use_provider_greeks,
            json,
        } => {
            let service = ThetaAnalysisService::from_env().await?;
            let analysis = service
                .analyze_option(AnalyzeOptionRequest {
                    symbol,
                    rate,
                    dividend,
                    iv,
                    iv_from_option_price,
                    iv_from_market_price,
                    show_iv_diff,
                    use_provider_greeks,
                })
                .await?;
            render_option_analysis(&analysis, json)?;
        }
        Command::AnalyzeChain {
            symbol,
            expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            side,
            min_strike,
            max_strike,
            min_delta,
            max_delta,
            min_theta,
            max_theta,
            min_vega,
            max_vega,
            min_iv,
            max_iv,
            min_option_price,
            max_option_price,
            min_otm_percent,
            max_otm_percent,
            only_liquid,
            exclude_abnormal,
            exclude_near_expiry,
            sort_by,
            limit,
            json,
        } => {
            let service = ThetaAnalysisService::from_env().await?;
            let analysis = service
                .analyze_chain(
                    parse_expiry_date(&expiry)?,
                    AnalyzeChainRequest {
                        symbol,
                        rate,
                        dividend,
                        iv,
                        iv_from_market_price,
                        screening: ChainScreeningRequest {
                            side,
                            min_strike,
                            max_strike,
                            min_delta,
                            max_delta,
                            min_theta,
                            max_theta,
                            min_vega,
                            max_vega,
                            min_iv,
                            max_iv,
                            min_option_price,
                            max_option_price,
                            min_otm_percent,
                            max_otm_percent,
                            only_liquid,
                            exclude_abnormal,
                            exclude_near_expiry,
                            sort_by,
                            limit,
                        },
                    },
                )
                .await?;
            render_chain_analysis(&analysis, json)?;
        }
        Command::CashSecuredPut {
            symbol,
            expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            direction,
            iv_direction,
            min_delta,
            max_delta,
            min_otm_percent,
            max_otm_percent,
            min_option_price,
            max_option_price,
            min_open_interest,
            min_volume,
            min_premium_per_contract,
            max_cash_required_per_contract,
            min_annualized_return_on_cash,
            max_annualized_return_on_cash,
            min_abs_mispricing_percent,
            min_abs_iv_diff_percent,
            limit,
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_cash_secured_puts(CashSecuredPutRequest {
                    symbol,
                    expiry: parse_expiry_date(&expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    direction,
                    iv_direction,
                    min_delta,
                    max_delta,
                    min_otm_percent,
                    max_otm_percent,
                    min_option_price,
                    max_option_price,
                    min_open_interest,
                    min_volume,
                    min_premium_per_contract,
                    max_cash_required_per_contract,
                    min_annualized_return_on_cash,
                    max_annualized_return_on_cash,
                    min_abs_mispricing_percent,
                    min_abs_iv_diff_percent,
                    limit,
                })
                .await?;
            render_cash_secured_puts(&view, json)?;
        }
        Command::CoveredCall {
            symbol,
            expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            direction,
            iv_direction,
            min_delta,
            max_delta,
            min_otm_percent,
            max_otm_percent,
            min_option_price,
            max_option_price,
            min_open_interest,
            min_volume,
            min_premium_per_contract,
            min_annualized_premium_yield,
            max_annualized_premium_yield,
            min_abs_mispricing_percent,
            min_abs_iv_diff_percent,
            limit,
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_covered_calls(CoveredCallRequest {
                    symbol,
                    expiry: parse_expiry_date(&expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    direction,
                    iv_direction,
                    min_delta,
                    max_delta,
                    min_otm_percent,
                    max_otm_percent,
                    min_option_price,
                    max_option_price,
                    min_open_interest,
                    min_volume,
                    min_premium_per_contract,
                    min_annualized_premium_yield,
                    max_annualized_premium_yield,
                    min_abs_mispricing_percent,
                    min_abs_iv_diff_percent,
                    limit,
                })
                .await?;
            render_covered_calls(&view, json)?;
        }
        Command::BullPutSpread {
            symbol,
            expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            direction,
            iv_direction,
            min_short_delta,
            max_short_delta,
            min_short_otm_percent,
            max_short_otm_percent,
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
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_bull_put_spreads(BullPutSpreadRequest {
                    symbol,
                    expiry: parse_expiry_date(&expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    direction,
                    iv_direction,
                    min_short_delta,
                    max_short_delta,
                    min_short_otm_percent,
                    max_short_otm_percent,
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
                })
                .await?;
            render_bull_put_spreads(&view, json)?;
        }
        Command::BearCallSpread {
            symbol,
            expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            direction,
            iv_direction,
            min_short_delta,
            max_short_delta,
            min_short_otm_percent,
            max_short_otm_percent,
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
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_bear_call_spreads(BearCallSpreadRequest {
                    symbol,
                    expiry: parse_expiry_date(&expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    direction,
                    iv_direction,
                    min_short_delta,
                    max_short_delta,
                    min_short_otm_percent,
                    max_short_otm_percent,
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
                })
                .await?;
            render_bear_call_spreads(&view, json)?;
        }
        Command::BearPutSpread {
            symbol,
            expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_width_per_spread,
            max_width_per_spread,
            min_annualized_return_on_risk,
            max_annualized_return_on_risk,
            limit,
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_bear_put_spreads(BearPutSpreadRequest {
                    symbol,
                    expiry: parse_expiry_date(&expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    min_open_interest,
                    min_volume,
                    max_net_debit_per_spread,
                    min_width_per_spread,
                    max_width_per_spread,
                    min_annualized_return_on_risk,
                    max_annualized_return_on_risk,
                    limit,
                })
                .await?;
            render_bear_put_spreads(&view, json)?;
        }
        Command::BullCallSpread {
            symbol,
            expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_width_per_spread,
            max_width_per_spread,
            min_annualized_return_on_risk,
            max_annualized_return_on_risk,
            limit,
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_bull_call_spreads(BullCallSpreadRequest {
                    symbol,
                    expiry: parse_expiry_date(&expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    min_open_interest,
                    min_volume,
                    max_net_debit_per_spread,
                    min_width_per_spread,
                    max_width_per_spread,
                    min_annualized_return_on_risk,
                    max_annualized_return_on_risk,
                    limit,
                })
                .await?;
            render_bull_call_spreads(&view, json)?;
        }
        Command::CalendarCallSpread {
            symbol,
            near_expiry,
            far_expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_net_theta_carry_per_day,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            sort_by,
            limit,
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_calendar_call_spreads(CalendarCallSpreadRequest {
                    symbol,
                    near_expiry: parse_expiry_date(&near_expiry)?,
                    far_expiry: parse_expiry_date(&far_expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    min_open_interest,
                    min_volume,
                    max_net_debit_per_spread,
                    min_net_theta_carry_per_day,
                    min_days_gap,
                    max_days_gap,
                    min_strike_gap,
                    max_strike_gap,
                    sort_by,
                    limit,
                })
                .await?;
            render_calendar_call_spreads(&view, json)?;
        }
        Command::CalendarPutSpread {
            symbol,
            near_expiry,
            far_expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_net_theta_carry_per_day,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            sort_by,
            limit,
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_calendar_put_spreads(CalendarPutSpreadRequest {
                    symbol,
                    near_expiry: parse_expiry_date(&near_expiry)?,
                    far_expiry: parse_expiry_date(&far_expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    min_open_interest,
                    min_volume,
                    max_net_debit_per_spread,
                    min_net_theta_carry_per_day,
                    min_days_gap,
                    max_days_gap,
                    min_strike_gap,
                    max_strike_gap,
                    sort_by,
                    limit,
                })
                .await?;
            render_calendar_put_spreads(&view, json)?;
        }
        Command::DiagonalCallSpread {
            symbol,
            near_expiry,
            far_expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_net_theta_carry_per_day,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            sort_by,
            limit,
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_diagonal_call_spreads(DiagonalCallSpreadRequest {
                    symbol,
                    near_expiry: parse_expiry_date(&near_expiry)?,
                    far_expiry: parse_expiry_date(&far_expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    min_open_interest,
                    min_volume,
                    max_net_debit_per_spread,
                    min_net_theta_carry_per_day,
                    min_days_gap,
                    max_days_gap,
                    min_strike_gap,
                    max_strike_gap,
                    sort_by,
                    limit,
                })
                .await?;
            render_diagonal_call_spreads(&view, json)?;
        }
        Command::DiagonalPutSpread {
            symbol,
            near_expiry,
            far_expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            min_open_interest,
            min_volume,
            max_net_debit_per_spread,
            min_net_theta_carry_per_day,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            sort_by,
            limit,
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_diagonal_put_spreads(DiagonalPutSpreadRequest {
                    symbol,
                    near_expiry: parse_expiry_date(&near_expiry)?,
                    far_expiry: parse_expiry_date(&far_expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    min_open_interest,
                    min_volume,
                    max_net_debit_per_spread,
                    min_net_theta_carry_per_day,
                    min_days_gap,
                    max_days_gap,
                    min_strike_gap,
                    max_strike_gap,
                    sort_by,
                    limit,
                })
                .await?;
            render_diagonal_put_spreads(&view, json)?;
        }
        Command::Mispricing {
            symbol,
            expiry,
            rate,
            dividend,
            side,
            direction,
            iv_direction,
            min_open_interest,
            min_volume,
            min_abs_mispricing_percent,
            min_abs_iv_diff_percent,
            sort_by,
            summary_only,
            group_by_side,
            top_per_side,
            limit,
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_mispricing(MispricingRequest {
                    symbol,
                    expiry: parse_expiry_date(&expiry)?,
                    rate,
                    dividend,
                    side,
                    direction,
                    iv_direction,
                    min_open_interest,
                    min_volume,
                    min_abs_mispricing_percent,
                    min_abs_iv_diff_percent,
                    sort_by,
                    limit,
                })
                .await?;
            render_mispricing(&view, json, summary_only, group_by_side, top_per_side)?;
        }
        Command::SellOpportunities {
            symbol,
            expiry,
            rate,
            dividend,
            iv,
            iv_from_market_price,
            direction,
            iv_direction,
            include_calendars,
            include_diagonals,
            min_open_interest,
            min_volume,
            min_abs_mispricing_percent,
            min_abs_iv_diff_percent,
            min_days_gap,
            max_days_gap,
            min_strike_gap,
            max_strike_gap,
            strategy,
            exclude_strategy,
            return_basis,
            exclude_return_basis,
            min_premium_or_credit,
            max_risk,
            min_annualized_return,
            max_annualized_return,
            limit_per_strategy,
            sort_by,
            group_by_strategy,
            group_by_return_basis,
            summary_only,
            limit,
            json,
        } => {
            let service = ThetaStrategyService::from_env().await?;
            let view = service
                .screen_sell_opportunities(SellOpportunitiesRequest {
                    symbol,
                    expiry: parse_expiry_date(&expiry)?,
                    rate,
                    dividend,
                    iv,
                    iv_from_market_price,
                    direction,
                    iv_direction,
                    include_calendars,
                    include_diagonals,
                    min_open_interest,
                    min_volume,
                    min_abs_mispricing_percent,
                    min_abs_iv_diff_percent,
                    min_days_gap,
                    max_days_gap,
                    min_strike_gap,
                    max_strike_gap,
                    strategy_filter: strategy,
                    exclude_strategy_filter: exclude_strategy,
                    return_basis_filter: return_basis,
                    exclude_return_basis_filter: exclude_return_basis,
                    min_premium_or_credit,
                    max_risk,
                    min_annualized_return,
                    max_annualized_return,
                    limit_per_strategy,
                    sort_by,
                    limit,
                })
                .await?;
            render_sell_opportunities(
                &view,
                json,
                group_by_strategy,
                group_by_return_basis,
                summary_only,
            )?;
        }
    }

    Ok(())
}

fn render_metrics(metrics: &OptionMetrics, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(metrics)?);
        return Ok(());
    }

    println!("type           : {:?}", metrics.option_type);
    println!("fair_value     : {:.6}", metrics.fair_value);
    println!("delta          : {:.6}", metrics.delta);
    println!("gamma          : {:.6}", metrics.gamma);
    println!("vega           : {:.6}", metrics.vega);
    println!("theta_per_day  : {:.6}", metrics.theta_per_day);
    println!("rho            : {:.6}", metrics.rho);
    println!("d1             : {:.6}", metrics.d1);
    println!("d2             : {:.6}", metrics.d2);

    Ok(())
}

fn render_option_chain(chain: &NormalizedOptionChainSnapshot, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(chain)?);
        return Ok(());
    }

    println!("underlying      : {}", chain.underlying.symbol);
    println!("spot            : {}", chain.underlying.last_done);
    println!("expiry          : {}", chain.expiry);
    println!("days_to_expiry  : {}", chain.days_to_expiry);
    println!("rows            : {}", chain.rows.len());

    for row in &chain.rows {
        let call_oi = row
            .call
            .as_ref()
            .map(|leg| leg.open_interest.to_string())
            .unwrap_or_else(|| "-".to_string());
        let put_oi = row
            .put
            .as_ref()
            .map(|leg| leg.open_interest.to_string())
            .unwrap_or_else(|| "-".to_string());
        let call_symbol = row
            .call
            .as_ref()
            .map(|leg| leg.symbol.as_str())
            .unwrap_or("-");
        let call_last_done = row
            .call
            .as_ref()
            .map(|leg| leg.last_done.as_str())
            .unwrap_or("-");
        let call_iv = row
            .call
            .as_ref()
            .map(|leg| leg.provider_reported_iv.as_str())
            .unwrap_or("-");
        let put_symbol = row
            .put
            .as_ref()
            .map(|leg| leg.symbol.as_str())
            .unwrap_or("-");
        let put_last_done = row
            .put
            .as_ref()
            .map(|leg| leg.last_done.as_str())
            .unwrap_or("-");
        let put_iv = row
            .put
            .as_ref()
            .map(|leg| leg.provider_reported_iv.as_str())
            .unwrap_or("-");

        println!(
            "strike {:>8} | call {:<20} {:>10} iv {:>8} oi {:>8} | put {:<20} {:>10} iv {:>8} oi {:>8}",
            row.strike_price,
            call_symbol,
            call_last_done,
            call_iv,
            call_oi,
            put_symbol,
            put_last_done,
            put_iv,
            put_oi,
        );
    }

    Ok(())
}

fn render_option_analysis(view: &OptionAnalysisView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!("option          : {}", view.option_symbol);
    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!("option_price    : {}", view.option_price);
    println!("strike          : {}", view.strike_price);
    println!(
        "expiry          : {} ({} days)",
        view.expiry, view.days_to_expiry
    );
    println!(
        "iv              : {} ({})",
        view.implied_volatility, view.implied_volatility_source
    );
    if let Some(price) = &view.iv_reference_price {
        println!("iv.ref_price    : {}", price);
    }
    println!("provider.iv     : {}", view.provider_reported_iv);
    println!(
        "rate/dividend   : {:.6} ({}) / {:.6}",
        view.rate, view.rate_source, view.dividend
    );
    println!("type            : {:?}", view.option_type);
    println!(
        "diag.liquid     : {} | otm {:>8.4}% | extrinsic {:>10.6} | breakeven {:>10.6}",
        view.diagnostics.is_liquid,
        view.diagnostics.otm_percent * 100.0,
        view.diagnostics.extrinsic_value,
        view.diagnostics.breakeven,
    );
    if !view.diagnostics.quality_flags.is_empty() {
        println!(
            "diag.quality    : {}",
            view.diagnostics.quality_flags.join(", ")
        );
    }
    if !view.diagnostics.liquidity_flags.is_empty() {
        println!(
            "diag.liquidity  : {}",
            view.diagnostics.liquidity_flags.join(", ")
        );
    }
    println!("local.fair_value: {:.6}", view.local_greeks.fair_value);
    println!("local.delta     : {:.6}", view.local_greeks.delta);
    println!("local.gamma     : {:.6}", view.local_greeks.gamma);
    println!("local.theta/day : {:.6}", view.local_greeks.theta_per_day);
    println!("local.vega      : {:.6}", view.local_greeks.vega);
    println!("local.rho       : {:.6}", view.local_greeks.rho);

    if let Some(compare) = &view.iv_comparison {
        println!("iv.diff.ref     : {}", compare.reference_price);
        println!("iv.diff.provider: {}", compare.provider_iv);
        println!("iv.diff.solved  : {}", compare.solved_from_market_price_iv);
        println!("iv.diff.delta   : {}", compare.diff);
    }

    if let Some(provider) = &view.provider_greeks {
        println!(
            "provider.delta  : {}",
            provider.delta.as_deref().unwrap_or("-")
        );
        println!(
            "provider.gamma  : {}",
            provider.gamma.as_deref().unwrap_or("-")
        );
        println!(
            "provider.theta  : {}",
            provider.theta.as_deref().unwrap_or("-")
        );
        println!(
            "provider.vega   : {}",
            provider.vega.as_deref().unwrap_or("-")
        );
        println!(
            "provider.rho    : {}",
            provider.rho.as_deref().unwrap_or("-")
        );
    }

    Ok(())
}

fn render_option_quote(quote: &OptionContractSnapshot, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(quote)?);
        return Ok(());
    }

    println!("symbol          : {}", quote.symbol);
    println!("underlying      : {}", quote.underlying_symbol);
    println!("type            : {:?}", quote.option_type);
    println!("last_done       : {}", quote.last_done);
    println!("prev_close      : {}", quote.prev_close);
    println!("open            : {}", quote.open);
    println!("high            : {}", quote.high);
    println!("low             : {}", quote.low);
    println!("volume          : {}", quote.volume);
    println!("turnover        : {}", quote.turnover);
    println!("timestamp       : {}", quote.timestamp);
    println!("trade_status    : {}", quote.trade_status);
    println!("strike          : {}", quote.strike_price);
    println!("expiry          : {}", quote.expiry);
    println!("provider.iv     : {}", quote.provider_reported_iv);
    println!("historical_vol  : {}", quote.historical_volatility);
    println!("open_interest   : {}", quote.open_interest);
    println!("multiplier      : {}", quote.contract_multiplier);
    println!("contract_size   : {}", quote.contract_size);
    println!("contract_style  : {}", quote.contract_style);

    Ok(())
}

fn render_chain_analysis(view: &ChainAnalysisView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "expiry          : {} ({} days)",
        view.expiry, view.days_to_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("rows            : {}", view.rows.len());

    for row in &view.rows {
        println!(
            "{} {:<4} px {:>10} k {:>10} iv {:>10} ({:<22}) delta {:>9.6} theta/day {:>9.6} liquid {:<5} otm {:>7.3}%",
            row.option_symbol,
            format!("{:?}", row.option_type),
            row.option_price,
            row.strike_price,
            row.implied_volatility,
            row.implied_volatility_source,
            row.local_greeks.delta,
            row.local_greeks.theta_per_day,
            row.diagnostics.is_liquid,
            row.diagnostics.otm_percent * 100.0,
        );
    }

    Ok(())
}

fn render_cash_secured_puts(view: &crate::domain::CashSecuredPutView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "expiry          : {} ({} days)",
        view.expiry, view.days_to_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());

    for row in &view.candidates {
        println!(
            "{} px {:>8} k {:>8} delta {:>8.4} otm {:>7.3}% roc {:>7.3}% ann {:>7.3}% mis {:>7.3}% iv_diff {:>7.3}% breakeven {:>9.3}",
            row.option_symbol,
            row.option_price,
            row.strike_price,
            row.delta,
            row.otm_percent * 100.0,
            row.return_on_cash * 100.0,
            row.annualized_return_on_cash * 100.0,
            row.mispricing_percent * 100.0,
            row.iv_diff_percent * 100.0,
            row.breakeven,
        );
    }

    Ok(())
}

fn render_covered_calls(view: &crate::domain::CoveredCallView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "expiry          : {} ({} days)",
        view.expiry, view.days_to_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());

    for row in &view.candidates {
        println!(
            "{} px {:>8} k {:>8} delta {:>8.4} otm {:>7.3}% prem {:>7.3}% ann {:>7.3}% mis {:>7.3}% iv_diff {:>7.3}% max_profit {:>10.3}",
            row.option_symbol,
            row.option_price,
            row.strike_price,
            row.delta,
            row.otm_percent * 100.0,
            row.premium_yield_on_underlying * 100.0,
            row.annualized_premium_yield * 100.0,
            row.mispricing_percent * 100.0,
            row.iv_diff_percent * 100.0,
            row.max_profit_per_contract,
        );
    }

    Ok(())
}

fn render_bull_put_spreads(view: &crate::domain::BullPutSpreadView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "expiry          : {} ({} days)",
        view.expiry, view.days_to_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());

    for row in &view.candidates {
        println!(
            "short {} @ {} | long {} @ {} | credit {:>8.2} width {:>8.2} max_loss {:>8.2} ann {:>7.3}% mis {:>7.3}% iv_diff {:>7.3}% breakeven {:>9.3}",
            row.short_option_symbol,
            row.short_strike_price,
            row.long_option_symbol,
            row.long_strike_price,
            row.net_credit_per_spread,
            row.width_per_spread,
            row.max_loss_per_spread,
            row.annualized_return_on_risk * 100.0,
            row.short_mispricing_percent * 100.0,
            row.short_iv_diff_percent * 100.0,
            row.breakeven,
        );
    }

    Ok(())
}

fn render_bear_call_spreads(view: &crate::domain::BearCallSpreadView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "expiry          : {} ({} days)",
        view.expiry, view.days_to_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());

    for row in &view.candidates {
        println!(
            "short {} @ {} | long {} @ {} | credit {:>8.2} width {:>8.2} max_loss {:>8.2} ann {:>7.3}% mis {:>7.3}% iv_diff {:>7.3}% breakeven {:>9.3}",
            row.short_option_symbol,
            row.short_strike_price,
            row.long_option_symbol,
            row.long_strike_price,
            row.net_credit_per_spread,
            row.width_per_spread,
            row.max_loss_per_spread,
            row.annualized_return_on_risk * 100.0,
            row.short_mispricing_percent * 100.0,
            row.short_iv_diff_percent * 100.0,
            row.breakeven,
        );
    }

    Ok(())
}

fn render_bear_put_spreads(view: &crate::domain::BearPutSpreadView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "expiry          : {} ({} days)",
        view.expiry, view.days_to_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());

    for row in &view.candidates {
        println!(
            "long {} @ {} | short {} @ {} | debit {:>8.2} width {:>8.2} max_profit {:>8.2} ann {:>7.3}% breakeven {:>9.3}",
            row.long_option_symbol,
            row.long_strike_price,
            row.short_option_symbol,
            row.short_strike_price,
            row.net_debit_per_spread,
            row.width_per_spread,
            row.max_profit_per_spread,
            row.annualized_return_on_risk * 100.0,
            row.breakeven,
        );
    }

    Ok(())
}

fn render_bull_call_spreads(view: &crate::domain::BullCallSpreadView, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "expiry          : {} ({} days)",
        view.expiry, view.days_to_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());

    for row in &view.candidates {
        println!(
            "long {} @ {} | short {} @ {} | debit {:>8.2} width {:>8.2} max_profit {:>8.2} ann {:>7.3}% breakeven {:>9.3}",
            row.long_option_symbol,
            row.long_strike_price,
            row.short_option_symbol,
            row.short_strike_price,
            row.net_debit_per_spread,
            row.width_per_spread,
            row.max_profit_per_spread,
            row.annualized_return_on_risk * 100.0,
            row.breakeven,
        );
    }

    Ok(())
}

fn render_calendar_call_spreads(
    view: &crate::domain::CalendarCallSpreadView,
    json: bool,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "near/far expiry : {} -> {}",
        view.near_expiry, view.far_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());

    for row in &view.candidates {
        println!(
            "near {} | far {} | k {:>8} d_gap {:>3} debit {:>8.2} theta {:>8.4} carry(run-rate) {:>7.2}% vega/debit {:>7.3} max_loss {:>8.2}",
            row.near_option_symbol,
            row.far_option_symbol,
            row.strike_price,
            row.days_gap,
            row.net_debit_per_spread,
            row.net_theta_carry_per_day,
            row.annualized_theta_carry_return_on_debit * 100.0,
            row.vega_to_debit_ratio,
            row.max_loss_per_spread,
        );
    }

    Ok(())
}

fn render_calendar_put_spreads(
    view: &crate::domain::CalendarPutSpreadView,
    json: bool,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "near/far expiry : {} -> {}",
        view.near_expiry, view.far_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());

    for row in &view.candidates {
        println!(
            "near {} | far {} | k {:>8} d_gap {:>3} debit {:>8.2} theta {:>8.4} carry(run-rate) {:>7.2}% vega/debit {:>7.3} max_loss {:>8.2}",
            row.near_option_symbol,
            row.far_option_symbol,
            row.strike_price,
            row.days_gap,
            row.net_debit_per_spread,
            row.net_theta_carry_per_day,
            row.annualized_theta_carry_return_on_debit * 100.0,
            row.vega_to_debit_ratio,
            row.max_loss_per_spread,
        );
    }

    Ok(())
}

fn render_diagonal_call_spreads(
    view: &crate::domain::DiagonalCallSpreadView,
    json: bool,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "near/far expiry : {} -> {}",
        view.near_expiry, view.far_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());

    for row in &view.candidates {
        println!(
            "near {} @ {} | far {} @ {} | d_gap {:>3} k_gap {:>6.2} debit {:>8.2} theta {:>8.4} carry(run-rate) {:>7.2}% vega/debit {:>7.3} max_loss {:>8.2}",
            row.near_option_symbol,
            row.near_strike_price,
            row.far_option_symbol,
            row.far_strike_price,
            row.days_gap,
            row.strike_gap,
            row.net_debit_per_spread,
            row.net_theta_carry_per_day,
            row.annualized_theta_carry_return_on_debit * 100.0,
            row.vega_to_debit_ratio,
            row.max_loss_per_spread,
        );
    }

    Ok(())
}

fn render_diagonal_put_spreads(
    view: &crate::domain::DiagonalPutSpreadView,
    json: bool,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "near/far expiry : {} -> {}",
        view.near_expiry, view.far_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());

    for row in &view.candidates {
        println!(
            "near {} @ {} | far {} @ {} | d_gap {:>3} k_gap {:>6.2} debit {:>8.2} theta {:>8.4} carry(run-rate) {:>7.2}% vega/debit {:>7.3} max_loss {:>8.2}",
            row.near_option_symbol,
            row.near_strike_price,
            row.far_option_symbol,
            row.far_strike_price,
            row.days_gap,
            row.strike_gap,
            row.net_debit_per_spread,
            row.net_theta_carry_per_day,
            row.annualized_theta_carry_return_on_debit * 100.0,
            row.vega_to_debit_ratio,
            row.max_loss_per_spread,
        );
    }

    Ok(())
}

fn render_mispricing(
    view: &crate::domain::MispricingView,
    json: bool,
    summary_only: bool,
    group_by_side: bool,
    top_per_side: Option<usize>,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "expiry          : {} ({} days)",
        view.expiry, view.days_to_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());
    let call_count = view
        .candidates
        .iter()
        .filter(|row| row.option_type == ContractSide::Call)
        .count();
    let put_count = view.candidates.len().saturating_sub(call_count);
    let best_abs_mispricing_percent = view
        .candidates
        .iter()
        .map(|row| row.mispricing_percent.abs())
        .reduce(f64::max)
        .unwrap_or(0.0);
    let best_abs_iv_diff_percent = view
        .candidates
        .iter()
        .map(|row| row.iv_diff_percent.abs())
        .reduce(f64::max)
        .unwrap_or(0.0);
    println!("call / put      : {} / {}", call_count, put_count);
    println!(
        "best abs mis    : {:>7.3}%",
        best_abs_mispricing_percent * 100.0
    );
    println!(
        "best abs iv     : {:>7.3}%",
        best_abs_iv_diff_percent * 100.0
    );

    if group_by_side {
        for side in [ContractSide::Call, ContractSide::Put] {
            let rows: Vec<_> = view
                .candidates
                .iter()
                .filter(|row| row.option_type == side)
                .collect();
            if rows.is_empty() {
                continue;
            }
            let best_side_abs_mis = rows
                .iter()
                .map(|row| row.mispricing_percent.abs())
                .reduce(f64::max)
                .unwrap_or(0.0);
            let best_side_abs_iv = rows
                .iter()
                .map(|row| row.iv_diff_percent.abs())
                .reduce(f64::max)
                .unwrap_or(0.0);
            println!(
                "{:<15}: {} rows, best mis {:>7.3}%, best iv {:>7.3}%",
                format!("{:?}", side),
                rows.len(),
                best_side_abs_mis * 100.0,
                best_side_abs_iv * 100.0,
            );
        }
    }

    if summary_only {
        return Ok(());
    }

    let display_rows = collect_mispricing_rows_for_display(&view.candidates, top_per_side);

    if group_by_side {
        for side in [ContractSide::Call, ContractSide::Put] {
            let rows: Vec<_> = display_rows
                .iter()
                .copied()
                .filter(|row| row.option_type == side)
                .collect();
            if rows.is_empty() {
                continue;
            }
            println!();
            println!("{:?} rows ({})", side, rows.len());
            for row in rows {
                println!(
                    "{} {:<4} px {:>8} fair {:>8.4} mis {:>8.4} ({:>7.3}%) iv_diff {:>8.4} ({:>7.3}%)",
                    row.option_symbol,
                    format!("{:?}", row.option_type),
                    row.option_price,
                    row.fair_value,
                    row.mispricing,
                    row.mispricing_percent * 100.0,
                    row.iv_diff,
                    row.iv_diff_percent * 100.0,
                );
            }
        }
    } else {
        for row in display_rows {
            println!(
                "{} {:<4} px {:>8} fair {:>8.4} mis {:>8.4} ({:>7.3}%) iv_diff {:>8.4} ({:>7.3}%)",
                row.option_symbol,
                format!("{:?}", row.option_type),
                row.option_price,
                row.fair_value,
                row.mispricing,
                row.mispricing_percent * 100.0,
                row.iv_diff,
                row.iv_diff_percent * 100.0,
            );
        }
    }

    Ok(())
}

fn collect_mispricing_rows_for_display<'a>(
    rows: &'a [crate::domain::MispricingCandidate],
    top_per_side: Option<usize>,
) -> Vec<&'a crate::domain::MispricingCandidate> {
    let Some(top_per_side) = top_per_side else {
        return rows.iter().collect();
    };
    if top_per_side == 0 {
        return Vec::new();
    }

    let mut call_count = 0usize;
    let mut put_count = 0usize;
    let mut filtered = Vec::with_capacity(rows.len());

    for row in rows {
        match row.option_type {
            ContractSide::Call if call_count < top_per_side => {
                call_count += 1;
                filtered.push(row);
            }
            ContractSide::Put if put_count < top_per_side => {
                put_count += 1;
                filtered.push(row);
            }
            _ => {}
        }
    }

    filtered
}

fn render_sell_opportunities(
    view: &crate::domain::SellOpportunitiesView,
    json: bool,
    group_by_strategy: bool,
    group_by_return_basis: bool,
    summary_only: bool,
) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(view)?);
        return Ok(());
    }

    println!(
        "underlying      : {} @ {}",
        view.underlying_symbol, view.underlying_price
    );
    println!(
        "expiry          : {} ({} days)",
        view.expiry, view.days_to_expiry
    );
    println!("rate            : {:.6} ({})", view.rate, view.rate_source);
    println!("candidates      : {}", view.candidates.len());
    let strategy_count = view
        .candidates
        .iter()
        .map(|row| row.strategy.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    println!("strategies      : {}", strategy_count);
    match best_annualized_return(&view.candidates) {
        Some(value) => println!("best annualized : {:>7.3}%", value * 100.0),
        None => println!("best annualized : n/a"),
    }
    if let Some(value) = best_carry_run_rate(&view.candidates) {
        println!("best carry run  : {:>7.3}%", value * 100.0);
    }

    if group_by_return_basis {
        let mut groups: BTreeMap<String, Vec<&crate::domain::SellOpportunityCandidate>> =
            BTreeMap::new();
        for row in &view.candidates {
            groups
                .entry(row.return_basis.to_string())
                .or_default()
                .push(row);
        }

        for (basis, rows) in groups {
            let share = if view.candidates.is_empty() {
                0.0
            } else {
                rows.len() as f64 / view.candidates.len() as f64
            };
            let best = rows
                .iter()
                .map(|row| row.annualized_return)
                .fold(f64::NEG_INFINITY, f64::max);
            let avg = rows.iter().map(|row| row.annualized_return).sum::<f64>() / rows.len() as f64;
            println!();
            println!(
                "{} ({}, {:>5.1}% of total, best {:>7.3}%, avg {:>7.3}%)",
                basis,
                rows.len(),
                share * 100.0,
                best * 100.0,
                avg * 100.0
            );
            if summary_only {
                continue;
            }
            for row in rows {
                let leg = match &row.secondary_symbol {
                    Some(secondary) => format!("{} / {}", row.primary_symbol, secondary),
                    None => row.primary_symbol.clone(),
                };
                let annualized = row
                    .annualized_return_note
                    .as_deref()
                    .map(|note| format!("{:>7.3}% [{}]", row.annualized_return * 100.0, note))
                    .unwrap_or_else(|| format!("{:>7.3}%", row.annualized_return * 100.0));
                let breakeven = row
                    .breakeven_note
                    .as_deref()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{:>9.3}", row.breakeven));
                println!(
                    "  {:<17} {:<40} ann {} credit {:>8.2} risk {:>8.2} mis {:>7.3}% iv_diff {:>7.3}% breakeven {}",
                    row.strategy,
                    leg,
                    annualized,
                    row.premium_or_credit,
                    row.max_risk,
                    row.mispricing_percent * 100.0,
                    row.iv_diff_percent * 100.0,
                    breakeven,
                );
            }
        }
    } else if group_by_strategy {
        let mut groups: BTreeMap<&str, Vec<&crate::domain::SellOpportunityCandidate>> =
            BTreeMap::new();
        for row in &view.candidates {
            groups.entry(&row.strategy).or_default().push(row);
        }

        for (strategy, rows) in groups {
            let share = if view.candidates.is_empty() {
                0.0
            } else {
                rows.len() as f64 / view.candidates.len() as f64
            };
            let realized_rows: Vec<&crate::domain::SellOpportunityCandidate> = rows
                .iter()
                .copied()
                .filter(|row| {
                    row.return_basis != crate::domain::SellOpportunityReturnBasis::ThetaCarryRunRate
                })
                .collect();
            let run_rate_rows: Vec<&crate::domain::SellOpportunityCandidate> = rows
                .iter()
                .copied()
                .filter(|row| {
                    row.return_basis == crate::domain::SellOpportunityReturnBasis::ThetaCarryRunRate
                })
                .collect();
            println!();
            if !realized_rows.is_empty() {
                let best_annualized_return = realized_rows
                    .iter()
                    .map(|row| row.annualized_return)
                    .fold(f64::NEG_INFINITY, f64::max);
                let avg_annualized_return = realized_rows
                    .iter()
                    .map(|row| row.annualized_return)
                    .sum::<f64>()
                    / realized_rows.len() as f64;
                println!(
                    "{} ({}, {:>5.1}% of total, best {:>7.3}%, avg {:>7.3}%)",
                    strategy,
                    rows.len(),
                    share * 100.0,
                    best_annualized_return * 100.0,
                    avg_annualized_return * 100.0
                );
            } else {
                let best_run_rate = run_rate_rows
                    .iter()
                    .map(|row| row.annualized_return)
                    .fold(f64::NEG_INFINITY, f64::max);
                let avg_run_rate = run_rate_rows
                    .iter()
                    .map(|row| row.annualized_return)
                    .sum::<f64>()
                    / run_rate_rows.len() as f64;
                println!(
                    "{} ({}, {:>5.1}% of total, best carry {:>7.3}%, avg carry {:>7.3}%)",
                    strategy,
                    rows.len(),
                    share * 100.0,
                    best_run_rate * 100.0,
                    avg_run_rate * 100.0
                );
            }
            if summary_only {
                continue;
            }
            for row in rows {
                let leg = match &row.secondary_symbol {
                    Some(secondary) => format!("{} / {}", row.primary_symbol, secondary),
                    None => row.primary_symbol.clone(),
                };
                let annualized = row
                    .annualized_return_note
                    .as_deref()
                    .map(|note| format!("{:>7.3}% [{}]", row.annualized_return * 100.0, note))
                    .unwrap_or_else(|| format!("{:>7.3}%", row.annualized_return * 100.0));
                let breakeven = row
                    .breakeven_note
                    .as_deref()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{:>9.3}", row.breakeven));
                println!(
                    "  {:<40} ann {} basis {} credit {:>8.2} risk {:>8.2} mis {:>7.3}% iv_diff {:>7.3}% breakeven {}",
                    leg,
                    annualized,
                    row.return_basis,
                    row.premium_or_credit,
                    row.max_risk,
                    row.mispricing_percent * 100.0,
                    row.iv_diff_percent * 100.0,
                    breakeven,
                );
            }
        }
    } else {
        if summary_only {
            return Ok(());
        }
        for row in &view.candidates {
            let leg = match &row.secondary_symbol {
                Some(secondary) => format!("{} / {}", row.primary_symbol, secondary),
                None => row.primary_symbol.clone(),
            };
            let annualized = row
                .annualized_return_note
                .as_deref()
                .map(|note| format!("{:>7.3}% [{}]", row.annualized_return * 100.0, note))
                .unwrap_or_else(|| format!("{:>7.3}%", row.annualized_return * 100.0));
            let breakeven = row
                .breakeven_note
                .as_deref()
                .map(str::to_string)
                .unwrap_or_else(|| format!("{:>9.3}", row.breakeven));
            println!(
                "{:<17} {:<40} ann {} basis {} credit {:>8.2} risk {:>8.2} mis {:>7.3}% iv_diff {:>7.3}% breakeven {}",
                row.strategy,
                leg,
                annualized,
                row.return_basis,
                row.premium_or_credit,
                row.max_risk,
                row.mispricing_percent * 100.0,
                row.iv_diff_percent * 100.0,
                breakeven,
            );
        }
    }

    Ok(())
}

fn best_annualized_return(candidates: &[crate::domain::SellOpportunityCandidate]) -> Option<f64> {
    candidates
        .iter()
        .filter(|row| {
            row.return_basis != crate::domain::SellOpportunityReturnBasis::ThetaCarryRunRate
        })
        .map(|row| row.annualized_return)
        .max_by(f64::total_cmp)
}

fn best_carry_run_rate(candidates: &[crate::domain::SellOpportunityCandidate]) -> Option<f64> {
    candidates
        .iter()
        .filter(|row| {
            row.return_basis == crate::domain::SellOpportunityReturnBasis::ThetaCarryRunRate
        })
        .map(|row| row.annualized_return)
        .max_by(f64::total_cmp)
}

#[cfg(test)]
mod tests {
    use crate::domain::{SellOpportunityCandidate, SellOpportunityReturnBasis};
    use crate::market_data;

    #[test]
    fn parses_expiry_dates() {
        let date = market_data::parse_expiry_date("2026-03-20").expect("valid date");
        assert_eq!(date.to_string(), "2026-03-20");
    }

    #[test]
    fn best_annualized_excludes_run_rate_rows() {
        let candidates = vec![
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

        assert_eq!(super::best_annualized_return(&candidates), Some(0.20));
        assert_eq!(super::best_carry_run_rate(&candidates), Some(0.80));
    }

    #[test]
    fn best_annualized_uses_return_basis_instead_of_notes() {
        let candidates = vec![
            SellOpportunityCandidate {
                strategy: "calendar_call_spread".to_string(),
                primary_symbol: "RUNRATE".to_string(),
                secondary_symbol: Some("FAR".to_string()),
                annualized_return: 0.80,
                return_basis: SellOpportunityReturnBasis::ThetaCarryRunRate,
                annualized_return_note: None,
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
                annualized_return_note: Some("display note only".to_string()),
                premium_or_credit: 100.0,
                max_risk: 39900.0,
                breakeven: 401.0,
                breakeven_note: None,
                mispricing_percent: 0.10,
                iv_diff_percent: 0.05,
            },
        ];

        assert_eq!(super::best_annualized_return(&candidates), Some(0.20));
        assert_eq!(super::best_carry_run_rate(&candidates), Some(0.80));
    }
}
