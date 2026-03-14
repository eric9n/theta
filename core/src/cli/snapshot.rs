use crate::analysis_service::{AnalyzeChainRequest, AnalyzeOptionRequest, ThetaAnalysisService};
use crate::analytics::{ContractSide, OptionMetrics, PricingInput, calculate_metrics};
use crate::diagnostics::{
    NormalizedChainDiagnosticsFilter, apply_normalized_chain_diagnostics_filter,
};
use crate::domain::{
    ChainAnalysisView, NormalizedOptionChainSnapshot, OptionAnalysisView, OptionContractSnapshot,
    normalize_option_chain,
};
use crate::market_data::parse_expiry_date;
use crate::screening_service::{ChainScreeningRequest, ChainSortField};
use crate::strategy_service::{
    BullCallSpreadRequest, BullPutSpreadRequest, CalendarCallSpreadRequest, CrossExpirySortField,
    DiagonalCallSpreadRequest, IvDiffDirection, MispricingDirection, ThetaStrategyService,
};
use anyhow::Result;
use clap::{Args, Subcommand};
use serde::Serialize;

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
        #[arg(
            long,
            default_value = "TSLA.US",
            help = "Underlying symbol. Default: TSLA.US"
        )]
        symbol: String,
    },
    /// List available option expiries for an underlying
    OptionExpiries {
        #[arg(
            long,
            default_value = "TSLA.US",
            help = "Underlying symbol. Default: TSLA.US"
        )]
        symbol: String,
    },
    /// Fetch an option chain with option quotes for one expiry
    OptionChain {
        #[arg(
            long,
            default_value = "TSLA.US",
            help = "Underlying symbol. Default: TSLA.US"
        )]
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
        #[arg(
            long,
            default_value = "TSLA.US",
            help = "Underlying symbol. Default: TSLA.US"
        )]
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
    /// Screen bull put spread candidates
    BullPutSpread {
        #[arg(
            long,
            default_value = "TSLA.US",
            help = "Underlying symbol. Default: TSLA.US"
        )]
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
    /// Screen bull call spread candidates
    BullCallSpread {
        #[arg(
            long,
            default_value = "TSLA.US",
            help = "Underlying symbol. Default: TSLA.US"
        )]
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
        #[arg(
            long,
            default_value = "TSLA.US",
            help = "Underlying symbol. Default: TSLA.US"
        )]
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
        #[arg(
            long,
            default_value = "TSLA.US",
            help = "Underlying symbol. Default: TSLA.US"
        )]
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

#[cfg(test)]
mod tests {
    use crate::market_data;

    #[test]
    fn parses_expiry_dates() {
        let date = market_data::parse_expiry_date("2026-03-20").expect("valid date");
        assert_eq!(date.to_string(), "2026-03-20");
    }
}
