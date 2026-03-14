use crate::analysis_service::{AnalyzeChainRequest, ThetaAnalysisService};
use crate::analytics::ContractSide;
use crate::daemon_protocol::{
    is_provider_code, is_transient_quote_rate_limit_error as is_transient_quote_limit_error,
};
use crate::domain::{
    ChainAnalysisRow, MarketToneSummary, MarketToneView, PutCallBiasView, PutCallSideTotals,
    SkewLegPoint, SkewSignalView, SmilePoint, SmileSignalView, TermStructurePoint,
    TermStructureView, UnderlyingSnapshot,
};
use crate::market_data::parse_expiry_date;
use crate::screening_service::ChainScreeningRequest;
use anyhow::{Context, Result, bail};

#[derive(Debug, Clone, Copy)]
pub struct ExpirySelection {
    pub min_days_to_expiry: i64,
    pub max_days_to_expiry: i64,
    pub target_days_to_expiry: i64,
}

pub struct SkewSignalRequest {
    pub symbol: String,
    pub expiry: time::Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub target_delta: f64,
    pub target_otm_percent: f64,
}

pub struct TermStructureRequest {
    pub symbol: String,
    pub expiries_limit: usize,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
}

pub struct SmileSignalRequest {
    pub symbol: String,
    pub expiry: time::Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub target_otm_percents: Vec<f64>,
}

pub struct PutCallBiasRequest {
    pub symbol: String,
    pub expiry: time::Date,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub min_otm_percent: f64,
}

pub struct MarketToneRequest {
    pub symbol: String,
    pub expiry: time::Date,
    pub expiries_limit: usize,
    pub rate: Option<f64>,
    pub dividend: f64,
    pub iv: Option<f64>,
    pub iv_from_market_price: bool,
    pub target_delta: f64,
    pub target_otm_percent: f64,
    pub smile_target_otm_percents: Vec<f64>,
    pub bias_min_otm_percent: f64,
}

pub struct ThetaSignalService {
    analysis: ThetaAnalysisService,
}

impl ThetaSignalService {
    pub async fn stock_quote(&self, symbol: &str) -> Result<UnderlyingSnapshot> {
        self.analysis.market().fetch_underlying(symbol).await
    }

    pub async fn from_env() -> Result<Self> {
        Ok(Self {
            analysis: ThetaAnalysisService::from_env().await?,
        })
    }

    /// Returns the underlying analysis service, allowing callers to reuse
    /// the pre-initialized LongPort connection instead of creating a new one.
    pub fn analysis(&self) -> &ThetaAnalysisService {
        &self.analysis
    }

    pub async fn front_expiry_for_symbol(&self, symbol: &str) -> Result<time::Date> {
        let expiries = self.analysis.market().fetch_option_expiries(symbol).await?;
        select_front_expiry(expiries, time::OffsetDateTime::now_utc().date())
            .with_context(|| format!("no usable option expiries returned for {}", symbol))
    }

    pub async fn target_expiry_for_symbol(
        &self,
        symbol: &str,
        selection: ExpirySelection,
    ) -> Result<time::Date> {
        let expiries = self.analysis.market().fetch_option_expiries(symbol).await?;
        select_expiry_by_dte(expiries, time::OffsetDateTime::now_utc().date(), selection)
            .with_context(|| {
                format!(
                    "no usable option expiries returned for {} in {}-{} DTE range",
                    symbol, selection.min_days_to_expiry, selection.max_days_to_expiry
                )
            })
    }

    pub async fn skew(&self, req: SkewSignalRequest) -> Result<SkewSignalView> {
        validate_skew_request(&req)?;

        let analysis = self
            .adaptive_analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol.clone(),
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        min_otm_percent: Some(-0.20),
                        max_otm_percent: Some(0.20),
                        ..Default::default()
                    },
                },
            )
            .await?;

        build_skew_signal_view(analysis, req.target_delta, req.target_otm_percent)
    }

    pub async fn term_structure(&self, req: TermStructureRequest) -> Result<TermStructureView> {
        validate_term_structure_request(&req)?;
        self.build_term_structure_from_front(
            req.symbol,
            time::Date::MIN, // Use MIN to fetch from the very first available expiry
            req.expiries_limit,
            req.rate,
            req.dividend,
            req.iv,
            req.iv_from_market_price,
            None,
        )
        .await
    }

    pub async fn smile(&self, req: SmileSignalRequest) -> Result<SmileSignalView> {
        validate_smile_request(&req)?;

        let analysis = self
            .adaptive_analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol.clone(),
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        min_otm_percent: Some(-0.20),
                        max_otm_percent: Some(0.20),
                        ..Default::default()
                    },
                },
            )
            .await?;

        build_smile_signal_view(analysis, &req.target_otm_percents)
    }

    pub async fn put_call_bias(&self, req: PutCallBiasRequest) -> Result<PutCallBiasView> {
        validate_put_call_bias_request(&req)?;

        let analysis = self
            .adaptive_analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol.clone(),
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        min_otm_percent: Some(-0.20),
                        max_otm_percent: Some(0.20),
                        ..Default::default()
                    },
                },
            )
            .await?;

        build_put_call_bias_view(analysis, req.min_otm_percent)
    }

    pub async fn market_tone(&self, req: MarketToneRequest) -> Result<MarketToneView> {
        validate_term_structure_request(&TermStructureRequest {
            symbol: req.symbol.clone(),
            expiries_limit: req.expiries_limit,
            rate: req.rate,
            dividend: req.dividend,
            iv: req.iv,
            iv_from_market_price: req.iv_from_market_price,
        })?;
        validate_skew_request(&SkewSignalRequest {
            symbol: req.symbol.clone(),
            expiry: req.expiry,
            rate: req.rate,
            dividend: req.dividend,
            iv: req.iv,
            iv_from_market_price: req.iv_from_market_price,
            target_delta: req.target_delta,
            target_otm_percent: req.target_otm_percent,
        })?;
        validate_smile_request(&SmileSignalRequest {
            symbol: req.symbol.clone(),
            expiry: req.expiry,
            rate: req.rate,
            dividend: req.dividend,
            iv: req.iv,
            iv_from_market_price: req.iv_from_market_price,
            target_otm_percents: req.smile_target_otm_percents.clone(),
        })?;
        validate_put_call_bias_request(&PutCallBiasRequest {
            symbol: req.symbol.clone(),
            expiry: req.expiry,
            rate: req.rate,
            dividend: req.dividend,
            iv: req.iv,
            iv_from_market_price: req.iv_from_market_price,
            min_otm_percent: req.bias_min_otm_percent,
        })?;

        // CRITICAL OPTIMIZATION: Fetch the chain analysis for front expiry ONCE
        let front_analysis = self
            .adaptive_analyze_chain(
                req.expiry,
                AnalyzeChainRequest {
                    symbol: req.symbol.clone(),
                    rate: req.rate,
                    dividend: req.dividend,
                    iv: req.iv,
                    iv_from_market_price: req.iv_from_market_price,
                    screening: ChainScreeningRequest {
                        only_liquid: true,
                        exclude_abnormal: true,
                        exclude_near_expiry: true,
                        min_otm_percent: Some(-0.20),
                        max_otm_percent: Some(0.20),
                        ..Default::default()
                    },
                },
            )
            .await?;

        // Reuse pre-computed analysis for all summary components
        let skew = build_skew_signal_view(
            front_analysis.clone(),
            req.target_delta,
            req.target_otm_percent,
        )?;
        let smile =
            build_smile_signal_view(front_analysis.clone(), &req.smile_target_otm_percents)?;
        let put_call_bias =
            build_put_call_bias_view(front_analysis.clone(), req.bias_min_otm_percent)?;

        // Throttling implemented inside build_term_structure_from_front
        let term_structure = self
            .build_term_structure_from_front(
                req.symbol.clone(),
                req.expiry,
                req.expiries_limit,
                req.rate,
                req.dividend,
                req.iv,
                req.iv_from_market_price,
                Some(front_analysis),
            )
            .await?;

        let summary = build_market_tone_summary(&skew, &smile, &put_call_bias, &term_structure);

        Ok(MarketToneView {
            underlying_symbol: req.symbol,
            front_expiry: skew.expiry.clone(),
            summary,
            skew,
            smile,
            put_call_bias,
            term_structure,
        })
    }

    async fn build_term_structure_from_front(
        &self,
        symbol: String,
        front_expiry: time::Date,
        expiries_limit: usize,
        rate: Option<f64>,
        dividend: f64,
        iv: Option<f64>,
        iv_from_market_price: bool,
        prefetched_front: Option<crate::domain::ChainAnalysisView>,
    ) -> Result<TermStructureView> {
        let expiry_strings = self
            .analysis
            .market()
            .fetch_option_expiries(&symbol)
            .await?;
        let expiries: Vec<time::Date> = expiry_strings
            .into_iter()
            .map(|value| parse_expiry_date(&value))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .filter(|expiry| *expiry >= front_expiry)
            .take(expiries_limit)
            .collect();

        let mut points = Vec::with_capacity(expiries.len());

        for (i, expiry) in expiries.into_iter().enumerate() {
            // Reuse prefetched front analysis if applicable
            let analysis = if i == 0
                && let Some(view) = prefetched_front.clone()
                && view.expiry == expiry.to_string()
            {
                view
            } else {
                // Throttling: introduce a small gap to avoid concurrent API rate limit 301607/max concurrent
                if i > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(3000)).await;
                }

                match self
                    .adaptive_analyze_chain(
                        expiry,
                        AnalyzeChainRequest {
                            symbol: symbol.clone(),
                            rate,
                            dividend,
                            iv,
                            iv_from_market_price,
                            screening: ChainScreeningRequest {
                                only_liquid: true,
                                exclude_abnormal: true,
                                exclude_near_expiry: true,
                                min_otm_percent: Some(-0.15), // Term structure can be +/- 15%
                                max_otm_percent: Some(0.15),
                                ..Default::default()
                            },
                        },
                    )
                    .await
                {
                    Ok(view) => view,
                    Err(err)
                        if i > 0 && !points.is_empty() && is_transient_quote_limit_error(&err) =>
                    {
                        tracing::warn!(
                            "Stopping term structure early for {} after {} points due to transient quote limit at expiry {}: {}",
                            symbol,
                            points.len(),
                            expiry,
                            err
                        );
                        break;
                    }
                    Err(err) => return Err(err),
                }
            };

            points.push(build_term_structure_point(&analysis)?);
        }

        apply_term_structure_deltas(&mut points);

        Ok(TermStructureView {
            underlying_symbol: symbol,
            target_expiries: expiries_limit,
            points,
        })
    }

    async fn adaptive_analyze_chain(
        &self,
        expiry: time::Date,
        mut req: AnalyzeChainRequest,
    ) -> Result<crate::domain::ChainAnalysisView> {
        let mut otm_range = req.screening.max_otm_percent.unwrap_or(0.20);
        let min_range = 0.05;

        loop {
            req.screening.min_otm_percent = Some(-otm_range);
            req.screening.max_otm_percent = Some(otm_range);

            match self.analysis.analyze_chain(expiry, req.clone()).await {
                Ok(view) => return Ok(view),
                Err(e) if is_provider_code(&e, 301607) && otm_range > min_range => {
                    tracing::warn!(
                        "Hit 301607 error for {} @ {} with range +/- {:.2}%. Narrowing range and retrying...",
                        req.symbol,
                        expiry,
                        otm_range * 100.0
                    );
                    otm_range *= 0.6; // Narrow range by 40%
                    if otm_range < min_range {
                        otm_range = min_range;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

fn validate_skew_request(req: &SkewSignalRequest) -> Result<()> {
    if !(0.0..1.0).contains(&req.target_delta) {
        bail!("target_delta must be between 0 and 1");
    }
    if req.target_otm_percent < 0.0 {
        bail!("target_otm_percent must be greater than or equal to 0");
    }
    Ok(())
}

fn validate_term_structure_request(req: &TermStructureRequest) -> Result<()> {
    if req.expiries_limit == 0 {
        bail!("expiries_limit must be greater than 0");
    }
    Ok(())
}

fn validate_smile_request(req: &SmileSignalRequest) -> Result<()> {
    if req.target_otm_percents.is_empty() {
        bail!("target_otm_percents must not be empty");
    }
    if req.target_otm_percents.iter().any(|value| *value < 0.0) {
        bail!("target_otm_percents must be greater than or equal to 0");
    }
    Ok(())
}

fn validate_put_call_bias_request(req: &PutCallBiasRequest) -> Result<()> {
    if req.min_otm_percent < 0.0 {
        bail!("min_otm_percent must be greater than or equal to 0");
    }
    Ok(())
}

fn build_skew_signal_view(
    analysis: crate::domain::ChainAnalysisView,
    target_delta: f64,
    target_otm_percent: f64,
) -> Result<SkewSignalView> {
    let atm_strike_price = find_atm_strike_price(&analysis.rows)?;
    let atm_iv = average_iv_for_strike(&analysis.rows, &atm_strike_price)?;

    let delta_put = select_by_delta(&analysis.rows, ContractSide::Put, target_delta)?;
    let delta_call = select_by_delta(&analysis.rows, ContractSide::Call, target_delta)?;
    let otm_put = select_by_otm(&analysis.rows, ContractSide::Put, target_otm_percent)?;
    let otm_call = select_by_otm(&analysis.rows, ContractSide::Call, target_otm_percent)?;

    let delta_skew = pair_diff(delta_put.as_ref(), delta_call.as_ref());
    let delta_put_wing_vs_atm = delta_put
        .as_ref()
        .map(|leg| leg.implied_volatility - atm_iv);
    let delta_call_wing_vs_atm = delta_call
        .as_ref()
        .map(|leg| leg.implied_volatility - atm_iv);
    let otm_skew = pair_diff(otm_put.as_ref(), otm_call.as_ref());
    let otm_put_wing_vs_atm = otm_put.as_ref().map(|leg| leg.implied_volatility - atm_iv);
    let otm_call_wing_vs_atm = otm_call.as_ref().map(|leg| leg.implied_volatility - atm_iv);

    Ok(SkewSignalView {
        underlying_symbol: analysis.underlying_symbol,
        underlying_price: analysis.underlying_price,
        expiry: analysis.expiry,
        days_to_expiry: analysis.days_to_expiry,
        rate: analysis.rate,
        rate_source: analysis.rate_source,
        target_delta,
        target_otm_percent,
        atm_strike_price,
        atm_iv,
        delta_put,
        delta_call,
        delta_skew,
        delta_put_wing_vs_atm,
        delta_call_wing_vs_atm,
        otm_put,
        otm_call,
        otm_skew,
        otm_put_wing_vs_atm,
        otm_call_wing_vs_atm,
    })
}

fn build_term_structure_point(
    analysis: &crate::domain::ChainAnalysisView,
) -> Result<TermStructurePoint> {
    let atm_strike_price = find_atm_strike_price(&analysis.rows)?;
    let atm_call_iv =
        find_leg_iv_for_strike(&analysis.rows, &atm_strike_price, ContractSide::Call)?;
    let atm_put_iv = find_leg_iv_for_strike(&analysis.rows, &atm_strike_price, ContractSide::Put)?;
    let atm_iv = match (atm_call_iv, atm_put_iv) {
        (Some(call), Some(put)) => (call + put) / 2.0,
        (Some(call), None) => call,
        (None, Some(put)) => put,
        (None, None) => bail!("no atm call or put found"),
    };

    Ok(TermStructurePoint {
        expiry: analysis.expiry.clone(),
        days_to_expiry: analysis.days_to_expiry,
        atm_strike_price,
        atm_call_iv,
        atm_put_iv,
        atm_iv,
        iv_change_from_prev: None,
        iv_change_from_front: None,
    })
}

fn build_smile_signal_view(
    analysis: crate::domain::ChainAnalysisView,
    target_otm_percents: &[f64],
) -> Result<SmileSignalView> {
    let atm_strike_price = find_atm_strike_price(&analysis.rows)?;
    let atm_iv = average_iv_for_strike(&analysis.rows, &atm_strike_price)?;

    let put_points = build_smile_points(
        &analysis.rows,
        ContractSide::Put,
        target_otm_percents,
        atm_iv,
    )?;
    let call_points = build_smile_points(
        &analysis.rows,
        ContractSide::Call,
        target_otm_percents,
        atm_iv,
    )?;
    let put_wing_slope = wing_slope(&put_points);
    let call_wing_slope = wing_slope(&call_points);

    Ok(SmileSignalView {
        underlying_symbol: analysis.underlying_symbol,
        underlying_price: analysis.underlying_price,
        expiry: analysis.expiry,
        days_to_expiry: analysis.days_to_expiry,
        rate: analysis.rate,
        rate_source: analysis.rate_source,
        atm_strike_price,
        atm_iv,
        put_points,
        call_points,
        put_wing_slope,
        call_wing_slope,
    })
}

fn build_put_call_bias_view(
    analysis: crate::domain::ChainAnalysisView,
    min_otm_percent: f64,
) -> Result<PutCallBiasView> {
    let all_puts = summarize_side(&analysis.rows, ContractSide::Put, None)?;
    let all_calls = summarize_side(&analysis.rows, ContractSide::Call, None)?;
    let otm_puts = summarize_side(&analysis.rows, ContractSide::Put, Some(min_otm_percent))?;
    let otm_calls = summarize_side(&analysis.rows, ContractSide::Call, Some(min_otm_percent))?;

    Ok(PutCallBiasView {
        underlying_symbol: analysis.underlying_symbol,
        underlying_price: analysis.underlying_price,
        expiry: analysis.expiry,
        days_to_expiry: analysis.days_to_expiry,
        rate: analysis.rate,
        rate_source: analysis.rate_source,
        min_otm_percent,
        volume_bias_ratio: safe_ratio(all_puts.total_volume as f64, all_calls.total_volume as f64),
        open_interest_bias_ratio: safe_ratio(
            all_puts.total_open_interest as f64,
            all_calls.total_open_interest as f64,
        ),
        otm_volume_bias_ratio: safe_ratio(
            otm_puts.total_volume as f64,
            otm_calls.total_volume as f64,
        ),
        otm_open_interest_bias_ratio: safe_ratio(
            otm_puts.total_open_interest as f64,
            otm_calls.total_open_interest as f64,
        ),
        average_iv_bias: option_diff(all_puts.average_iv, all_calls.average_iv),
        otm_average_iv_bias: option_diff(otm_puts.average_iv, otm_calls.average_iv),
        all_puts,
        all_calls,
        otm_puts,
        otm_calls,
    })
}

fn build_market_tone_summary(
    skew: &SkewSignalView,
    smile: &SmileSignalView,
    put_call_bias: &PutCallBiasView,
    term_structure: &TermStructureView,
) -> MarketToneSummary {
    let farthest = term_structure.points.last();
    let downside_protection = classify_downside_protection(
        skew.delta_skew,
        skew.otm_skew,
        put_call_bias.otm_average_iv_bias,
    );
    let term_structure_shape =
        classify_term_structure_shape(farthest.and_then(|point| point.iv_change_from_front));
    let wing_shape = classify_wing_shape(smile.put_wing_slope, smile.call_wing_slope);
    let positioning_bias = classify_positioning_bias(
        put_call_bias.open_interest_bias_ratio,
        put_call_bias.otm_open_interest_bias_ratio,
    );
    let overall_tone = classify_overall_tone(
        &downside_protection,
        &term_structure_shape,
        &wing_shape,
        &positioning_bias,
    );
    let summary_sentence = build_market_tone_sentence(
        &overall_tone,
        &downside_protection,
        &term_structure_shape,
        &wing_shape,
        &positioning_bias,
    );

    MarketToneSummary {
        delta_skew: skew.delta_skew,
        otm_skew: skew.otm_skew,
        front_atm_iv: skew.atm_iv,
        farthest_atm_iv: farthest.map(|point| point.atm_iv),
        term_structure_change_from_front: farthest.and_then(|point| point.iv_change_from_front),
        put_wing_slope: smile.put_wing_slope,
        call_wing_slope: smile.call_wing_slope,
        open_interest_bias_ratio: put_call_bias.open_interest_bias_ratio,
        otm_open_interest_bias_ratio: put_call_bias.otm_open_interest_bias_ratio,
        average_iv_bias: put_call_bias.average_iv_bias,
        otm_average_iv_bias: put_call_bias.otm_average_iv_bias,
        downside_protection,
        term_structure_shape,
        wing_shape,
        positioning_bias,
        overall_tone,
        summary_sentence,
    }
}

fn classify_downside_protection(
    delta_skew: Option<f64>,
    otm_skew: Option<f64>,
    otm_average_iv_bias: Option<f64>,
) -> String {
    let score = [delta_skew, otm_skew, otm_average_iv_bias]
        .into_iter()
        .flatten()
        .sum::<f64>();

    if score >= 0.20 {
        "elevated_downside_hedging".to_string()
    } else if score >= 0.05 {
        "moderate_downside_hedging".to_string()
    } else if score <= -0.05 {
        "call_side_rich".to_string()
    } else {
        "balanced".to_string()
    }
}

fn classify_term_structure_shape(change_from_front: Option<f64>) -> String {
    match change_from_front {
        Some(change) if change >= 0.05 => "contango_up".to_string(),
        Some(change) if change >= 0.015 => "mild_contango".to_string(),
        Some(change) if change <= -0.05 => "backwardation_down".to_string(),
        Some(change) if change <= -0.015 => "mild_backwardation".to_string(),
        Some(_) => "flat".to_string(),
        None => "insufficient_term_data".to_string(),
    }
}

fn classify_wing_shape(put_wing_slope: Option<f64>, call_wing_slope: Option<f64>) -> String {
    match (put_wing_slope, call_wing_slope) {
        (Some(put), Some(call)) if put > call + 0.5 => "left_tail_heavy".to_string(),
        (Some(put), Some(call)) if call > put + 0.5 => "right_tail_speculative".to_string(),
        (Some(put), Some(call)) if put > 0.0 && call > 0.0 => "both_wings_bid".to_string(),
        (Some(_), Some(_)) => "flat_or_mixed".to_string(),
        _ => "insufficient_smile_data".to_string(),
    }
}

fn classify_positioning_bias(
    open_interest_bias_ratio: Option<f64>,
    otm_open_interest_bias_ratio: Option<f64>,
) -> String {
    let effective = otm_open_interest_bias_ratio.or(open_interest_bias_ratio);
    match effective {
        Some(value) if value >= 1.5 => "put_heavy".to_string(),
        Some(value) if value >= 1.1 => "put_lean".to_string(),
        Some(value) if value <= 0.7 => "call_heavy".to_string(),
        Some(value) if value <= 0.9 => "call_lean".to_string(),
        Some(_) => "balanced".to_string(),
        None => "insufficient_positioning_data".to_string(),
    }
}

fn classify_overall_tone(
    downside_protection: &str,
    term_structure_shape: &str,
    wing_shape: &str,
    positioning_bias: &str,
) -> String {
    let defensive_score = usize::from(downside_protection.contains("downside"))
        + usize::from(term_structure_shape.contains("backwardation"))
        + usize::from(wing_shape == "left_tail_heavy")
        + usize::from(positioning_bias.contains("put"));
    let speculative_score = usize::from(downside_protection == "call_side_rich")
        + usize::from(term_structure_shape.contains("contango"))
        + usize::from(wing_shape == "right_tail_speculative")
        + usize::from(positioning_bias.contains("call"));

    if defensive_score >= speculative_score + 2 {
        "defensive".to_string()
    } else if speculative_score >= defensive_score + 2 {
        "speculative".to_string()
    } else {
        "balanced".to_string()
    }
}

fn build_market_tone_sentence(
    overall_tone: &str,
    downside_protection: &str,
    term_structure_shape: &str,
    wing_shape: &str,
    positioning_bias: &str,
) -> String {
    let tone = match overall_tone {
        "defensive" => "Market tone looks defensive",
        "speculative" => "Market tone looks speculative",
        _ => "Market tone looks balanced",
    };

    let protection = match downside_protection {
        "elevated_downside_hedging" => "downside protection is priced aggressively",
        "moderate_downside_hedging" => "downside hedging demand is above neutral",
        "call_side_rich" => "call-side pricing is relatively richer",
        _ => "downside protection pricing is broadly balanced",
    };

    let term = match term_structure_shape {
        "contango_up" => "the term structure is strongly upward sloping",
        "mild_contango" => "the term structure is mildly upward sloping",
        "backwardation_down" => "the term structure is clearly inverted",
        "mild_backwardation" => "the term structure is mildly inverted",
        "flat" => "the term structure is mostly flat",
        _ => "term structure data is limited",
    };

    let wings = match wing_shape {
        "left_tail_heavy" => "put wings are heavier than call wings",
        "right_tail_speculative" => "call wings are richer than put wings",
        "both_wings_bid" => "both wings are bid versus ATM",
        "flat_or_mixed" => "wing pricing is mixed",
        _ => "smile data is limited",
    };

    let positioning = match positioning_bias {
        "put_heavy" => "positioning leans heavily toward puts",
        "put_lean" => "positioning leans modestly toward puts",
        "call_heavy" => "positioning leans heavily toward calls",
        "call_lean" => "positioning leans modestly toward calls",
        "balanced" => "positioning is roughly balanced",
        _ => "positioning data is limited",
    };

    format!("{tone}; {protection}; {term}; {wings}; {positioning}.")
}

fn find_atm_strike_price(rows: &[ChainAnalysisRow]) -> Result<String> {
    rows.iter()
        .min_by(|a, b| {
            a.diagnostics
                .otm_percent
                .abs()
                .total_cmp(&b.diagnostics.otm_percent.abs())
                .then_with(|| {
                    (a.local_greeks.delta.abs() - 0.5)
                        .abs()
                        .total_cmp(&(b.local_greeks.delta.abs() - 0.5).abs())
                })
                .then_with(|| a.strike_price.cmp(&b.strike_price))
        })
        .map(|row| row.strike_price.clone())
        .ok_or_else(|| anyhow::anyhow!("no option rows available for skew calculation"))
}

fn average_iv_for_strike(rows: &[ChainAnalysisRow], strike_price: &str) -> Result<f64> {
    let mut count = 0usize;
    let mut total = 0.0;

    for row in rows {
        if row.strike_price == strike_price {
            total += parse_iv(&row.implied_volatility)?;
            count += 1;
        }
    }

    if count == 0 {
        bail!("no rows found for atm strike");
    }

    Ok(total / count as f64)
}

fn find_leg_iv_for_strike(
    rows: &[ChainAnalysisRow],
    strike_price: &str,
    side: ContractSide,
) -> Result<Option<f64>> {
    rows.iter()
        .find(|row| row.strike_price == strike_price && row.option_type == side)
        .map(|row| parse_iv(&row.implied_volatility))
        .transpose()
}

fn select_front_expiry(expiries: Vec<String>, today: time::Date) -> Result<time::Date> {
    let mut parsed = Vec::with_capacity(expiries.len());
    for expiry in expiries {
        parsed.push(parse_expiry_date(&expiry)?);
    }

    if let Some(expiry) = parsed
        .iter()
        .copied()
        .find(|expiry| (*expiry - today).whole_days() > 1)
    {
        return Ok(expiry);
    }

    if let Some(expiry) = parsed.iter().copied().find(|expiry| *expiry >= today) {
        return Ok(expiry);
    }

    parsed
        .into_iter()
        .max()
        .ok_or_else(|| anyhow::anyhow!("no option expiries available"))
}

fn select_expiry_by_dte(
    expiries: Vec<String>,
    today: time::Date,
    selection: ExpirySelection,
) -> Result<time::Date> {
    if selection.min_days_to_expiry > selection.max_days_to_expiry {
        bail!(
            "invalid DTE range: min {} exceeds max {}",
            selection.min_days_to_expiry,
            selection.max_days_to_expiry
        );
    }

    let mut candidates = Vec::new();
    for expiry in expiries {
        let expiry = parse_expiry_date(&expiry)?;
        let days_to_expiry = (expiry - today).whole_days();
        if days_to_expiry < 0 {
            continue;
        }

        candidates.push((
            expiry,
            if days_to_expiry == 0 {
                1
            } else {
                days_to_expiry
            },
        ));
    }

    let Some((expiry, _)) = candidates
        .into_iter()
        .filter(|(_, dte)| {
            *dte >= selection.min_days_to_expiry && *dte <= selection.max_days_to_expiry
        })
        .min_by(|(left_expiry, left_dte), (right_expiry, right_dte)| {
            (left_dte - selection.target_days_to_expiry)
                .abs()
                .cmp(&(right_dte - selection.target_days_to_expiry).abs())
                .then_with(|| left_dte.cmp(right_dte))
                .then_with(|| left_expiry.cmp(right_expiry))
        })
    else {
        bail!(
            "no option expiries available in {}-{} DTE range",
            selection.min_days_to_expiry,
            selection.max_days_to_expiry
        );
    };

    Ok(expiry)
}

fn select_by_delta(
    rows: &[ChainAnalysisRow],
    side: ContractSide,
    target_delta: f64,
) -> Result<Option<SkewLegPoint>> {
    rows.iter()
        .filter(|row| row.option_type == side)
        .min_by(|a, b| {
            delta_distance(a, target_delta)
                .total_cmp(&delta_distance(b, target_delta))
                .then_with(|| {
                    a.diagnostics
                        .otm_percent
                        .abs()
                        .total_cmp(&b.diagnostics.otm_percent.abs())
                })
                .then_with(|| a.option_symbol.cmp(&b.option_symbol))
        })
        .map(to_skew_leg_point)
        .transpose()
}

fn select_by_otm(
    rows: &[ChainAnalysisRow],
    side: ContractSide,
    target_otm_percent: f64,
) -> Result<Option<SkewLegPoint>> {
    rows.iter()
        .filter(|row| row.option_type == side)
        .min_by(|a, b| {
            (a.diagnostics.otm_percent.abs() - target_otm_percent)
                .abs()
                .total_cmp(&(b.diagnostics.otm_percent.abs() - target_otm_percent).abs())
                .then_with(|| delta_distance(a, 0.25).total_cmp(&delta_distance(b, 0.25)))
                .then_with(|| a.option_symbol.cmp(&b.option_symbol))
        })
        .map(to_skew_leg_point)
        .transpose()
}

fn build_smile_points(
    rows: &[ChainAnalysisRow],
    side: ContractSide,
    target_otm_percents: &[f64],
    atm_iv: f64,
) -> Result<Vec<SmilePoint>> {
    let mut points = Vec::with_capacity(target_otm_percents.len());

    for target in target_otm_percents {
        if let Some(point) = select_by_otm(rows, side, *target)? {
            points.push(SmilePoint {
                target_otm_percent: *target,
                option_symbol: point.option_symbol,
                strike_price: point.strike_price,
                delta: point.delta,
                otm_percent: point.otm_percent,
                implied_volatility: point.implied_volatility,
                iv_vs_atm: point.implied_volatility - atm_iv,
            });
        }
    }

    Ok(points)
}

fn summarize_side(
    rows: &[ChainAnalysisRow],
    side: ContractSide,
    min_otm_percent: Option<f64>,
) -> Result<PutCallSideTotals> {
    let mut contracts = 0usize;
    let mut total_volume = 0i64;
    let mut total_open_interest = 0i64;
    let mut iv_total = 0.0;
    let mut iv_count = 0usize;

    for row in rows {
        if row.option_type != side {
            continue;
        }
        if let Some(min_otm_percent) = min_otm_percent
            && row.diagnostics.otm_percent.abs() + 1.0e-12 < min_otm_percent
        {
            continue;
        }

        contracts += 1;
        total_volume += row.volume;
        total_open_interest += row.open_interest;
        iv_total += parse_iv(&row.implied_volatility)?;
        iv_count += 1;
    }

    Ok(PutCallSideTotals {
        contracts,
        total_volume,
        total_open_interest,
        average_iv: if iv_count > 0 {
            Some(iv_total / iv_count as f64)
        } else {
            None
        },
    })
}

fn to_skew_leg_point(row: &ChainAnalysisRow) -> Result<SkewLegPoint> {
    Ok(SkewLegPoint {
        option_symbol: row.option_symbol.clone(),
        strike_price: row.strike_price.clone(),
        delta: row.local_greeks.delta,
        otm_percent: row.diagnostics.otm_percent,
        implied_volatility: parse_iv(&row.implied_volatility)?,
    })
}

fn parse_iv(value: &str) -> Result<f64> {
    value
        .parse::<f64>()
        .map_err(|_| anyhow::anyhow!("failed to parse implied volatility"))
}

fn delta_distance(row: &ChainAnalysisRow, target_delta: f64) -> f64 {
    match row.option_type {
        ContractSide::Put => (row.local_greeks.delta.abs() - target_delta).abs(),
        ContractSide::Call => (row.local_greeks.delta - target_delta).abs(),
    }
}

fn pair_diff(left: Option<&SkewLegPoint>, right: Option<&SkewLegPoint>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.implied_volatility - right.implied_volatility),
        _ => None,
    }
}

fn safe_ratio(left: f64, right: f64) -> Option<f64> {
    if right.abs() <= 1.0e-12 {
        None
    } else {
        Some(left / right)
    }
}

fn option_diff(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left - right),
        _ => None,
    }
}

fn wing_slope(points: &[SmilePoint]) -> Option<f64> {
    let first = points.first()?;
    let last = points.last()?;
    let gap = last.target_otm_percent - first.target_otm_percent;
    if gap.abs() <= 1.0e-12 {
        return None;
    }
    Some((last.implied_volatility - first.implied_volatility) / gap)
}

fn apply_term_structure_deltas(points: &mut [TermStructurePoint]) {
    if points.is_empty() {
        return;
    }

    let front_iv = points[0].atm_iv;
    let mut prev_iv: Option<f64> = None;

    for point in points {
        point.iv_change_from_prev = prev_iv.map(|value| point.atm_iv - value);
        point.iv_change_from_front = Some(point.atm_iv - front_iv);
        prev_iv = Some(point.atm_iv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::{ContractSide, OptionMetrics};
    use crate::domain::{ChainAnalysisRow, ChainAnalysisView, ContractDiagnostics};

    #[test]
    fn builds_skew_signal_from_chain_analysis() {
        let view = build_skew_signal_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row("PUT25", ContractSide::Put, "380", "0.40", -0.25, 0.05),
                    sample_row("CALL25", ContractSide::Call, "420", "0.30", 0.25, 0.05),
                    sample_row("ATM_CALL", ContractSide::Call, "400", "0.32", 0.50, 0.00),
                    sample_row("ATM_PUT", ContractSide::Put, "400", "0.34", -0.50, 0.00),
                ],
            },
            0.25,
            0.05,
        )
        .expect("skew builds");

        assert_eq!(view.atm_strike_price, "400");
        assert!((view.atm_iv - 0.33).abs() < 1.0e-9);
        assert_eq!(
            view.delta_put
                .as_ref()
                .map(|leg| leg.option_symbol.as_str()),
            Some("PUT25")
        );
        assert_eq!(
            view.delta_call
                .as_ref()
                .map(|leg| leg.option_symbol.as_str()),
            Some("CALL25")
        );
        assert!((view.delta_skew.expect("delta skew") - 0.10).abs() < 1.0e-9);
        assert!((view.otm_skew.expect("otm skew") - 0.10).abs() < 1.0e-9);
    }

    #[test]
    fn builds_term_structure_point_from_chain_analysis() {
        let point = build_term_structure_point(&ChainAnalysisView {
            underlying_symbol: "TSLA.US".to_string(),
            underlying_price: "400".to_string(),
            expiry: "2026-03-20".to_string(),
            days_to_expiry: 30,
            rate: 0.04,
            rate_source: "curve_default".to_string(),
            rows: vec![
                sample_row("ATM_CALL", ContractSide::Call, "400", "0.32", 0.50, 0.00),
                sample_row("ATM_PUT", ContractSide::Put, "400", "0.34", -0.50, 0.00),
            ],
        })
        .expect("point builds");

        assert_eq!(point.atm_strike_price, "400");
        assert_eq!(point.atm_call_iv, Some(0.32));
        assert_eq!(point.atm_put_iv, Some(0.34));
        assert!((point.atm_iv - 0.33).abs() < 1.0e-9);
    }

    #[test]
    fn applies_term_structure_deltas() {
        let mut points = vec![
            TermStructurePoint {
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                atm_strike_price: "400".to_string(),
                atm_call_iv: Some(0.30),
                atm_put_iv: Some(0.34),
                atm_iv: 0.32,
                iv_change_from_prev: None,
                iv_change_from_front: None,
            },
            TermStructurePoint {
                expiry: "2026-04-17".to_string(),
                days_to_expiry: 58,
                atm_strike_price: "400".to_string(),
                atm_call_iv: Some(0.31),
                atm_put_iv: Some(0.35),
                atm_iv: 0.33,
                iv_change_from_prev: None,
                iv_change_from_front: None,
            },
        ];

        apply_term_structure_deltas(&mut points);

        assert_eq!(points[0].iv_change_from_prev, None);
        assert_eq!(points[0].iv_change_from_front, Some(0.0));
        assert!((points[1].iv_change_from_prev.expect("prev delta") - 0.01).abs() < 1.0e-9);
        assert!((points[1].iv_change_from_front.expect("front delta") - 0.01).abs() < 1.0e-9);
    }

    #[test]
    fn builds_smile_signal_from_chain_analysis() {
        let view = build_smile_signal_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row("PUT5", ContractSide::Put, "380", "0.40", -0.25, 0.05),
                    sample_row("PUT10", ContractSide::Put, "360", "0.45", -0.15, 0.10),
                    sample_row("CALL5", ContractSide::Call, "420", "0.30", 0.25, 0.05),
                    sample_row("CALL10", ContractSide::Call, "440", "0.33", 0.15, 0.10),
                    sample_row("ATM_CALL", ContractSide::Call, "400", "0.32", 0.50, 0.00),
                    sample_row("ATM_PUT", ContractSide::Put, "400", "0.34", -0.50, 0.00),
                ],
            },
            &[0.05, 0.10],
        )
        .expect("smile builds");

        assert_eq!(view.put_points.len(), 2);
        assert_eq!(view.call_points.len(), 2);
        assert_eq!(view.put_points[0].option_symbol, "PUT5");
        assert_eq!(view.put_points[1].option_symbol, "PUT10");
        assert_eq!(view.call_points[0].option_symbol, "CALL5");
        assert_eq!(view.call_points[1].option_symbol, "CALL10");
        assert!(view.put_wing_slope.expect("put slope") > 0.0);
        assert!(view.call_wing_slope.expect("call slope") > 0.0);
    }

    #[test]
    fn builds_put_call_bias_from_chain_analysis() {
        let view = build_put_call_bias_view(
            ChainAnalysisView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                rows: vec![
                    sample_row_with_counts(
                        "PUT5",
                        ContractSide::Put,
                        "380",
                        "0.40",
                        -0.25,
                        0.05,
                        100,
                        200,
                    ),
                    sample_row_with_counts(
                        "PUT10",
                        ContractSide::Put,
                        "360",
                        "0.45",
                        -0.15,
                        0.10,
                        50,
                        80,
                    ),
                    sample_row_with_counts(
                        "CALL5",
                        ContractSide::Call,
                        "420",
                        "0.30",
                        0.25,
                        0.05,
                        40,
                        100,
                    ),
                    sample_row_with_counts(
                        "CALL10",
                        ContractSide::Call,
                        "440",
                        "0.33",
                        0.15,
                        0.10,
                        20,
                        60,
                    ),
                ],
            },
            0.05,
        )
        .expect("bias builds");

        assert_eq!(view.all_puts.total_volume, 150);
        assert_eq!(view.all_calls.total_volume, 60);
        assert!((view.volume_bias_ratio.expect("volume ratio") - 2.5).abs() < 1.0e-9);
        assert!((view.open_interest_bias_ratio.expect("oi ratio") - 1.75).abs() < 1.0e-9);
        assert!(view.average_iv_bias.expect("iv bias") > 0.0);
        assert_eq!(view.otm_puts.contracts, 2);
        assert_eq!(view.otm_calls.contracts, 2);
    }

    #[test]
    fn builds_market_tone_summary_from_component_signals() {
        let summary = build_market_tone_summary(
            &SkewSignalView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                target_delta: 0.25,
                target_otm_percent: 0.05,
                atm_strike_price: "400".to_string(),
                atm_iv: 0.33,
                delta_put: None,
                delta_call: None,
                delta_skew: Some(0.10),
                delta_put_wing_vs_atm: None,
                delta_call_wing_vs_atm: None,
                otm_put: None,
                otm_call: None,
                otm_skew: Some(0.08),
                otm_put_wing_vs_atm: None,
                otm_call_wing_vs_atm: None,
            },
            &SmileSignalView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                atm_strike_price: "400".to_string(),
                atm_iv: 0.33,
                put_points: Vec::new(),
                call_points: Vec::new(),
                put_wing_slope: Some(1.2),
                call_wing_slope: Some(0.6),
            },
            &PutCallBiasView {
                underlying_symbol: "TSLA.US".to_string(),
                underlying_price: "400".to_string(),
                expiry: "2026-03-20".to_string(),
                days_to_expiry: 30,
                rate: 0.04,
                rate_source: "curve_default".to_string(),
                min_otm_percent: 0.05,
                all_puts: PutCallSideTotals {
                    contracts: 1,
                    total_volume: 1,
                    total_open_interest: 1,
                    average_iv: Some(0.40),
                },
                all_calls: PutCallSideTotals {
                    contracts: 1,
                    total_volume: 1,
                    total_open_interest: 1,
                    average_iv: Some(0.30),
                },
                otm_puts: PutCallSideTotals {
                    contracts: 1,
                    total_volume: 1,
                    total_open_interest: 2,
                    average_iv: Some(0.42),
                },
                otm_calls: PutCallSideTotals {
                    contracts: 1,
                    total_volume: 1,
                    total_open_interest: 1,
                    average_iv: Some(0.31),
                },
                volume_bias_ratio: Some(1.0),
                open_interest_bias_ratio: Some(1.0),
                otm_volume_bias_ratio: Some(1.0),
                otm_open_interest_bias_ratio: Some(2.0),
                average_iv_bias: Some(0.10),
                otm_average_iv_bias: Some(0.11),
            },
            &TermStructureView {
                underlying_symbol: "TSLA.US".to_string(),
                target_expiries: 2,
                points: vec![
                    TermStructurePoint {
                        expiry: "2026-03-20".to_string(),
                        days_to_expiry: 30,
                        atm_strike_price: "400".to_string(),
                        atm_call_iv: Some(0.32),
                        atm_put_iv: Some(0.34),
                        atm_iv: 0.33,
                        iv_change_from_prev: None,
                        iv_change_from_front: Some(0.0),
                    },
                    TermStructurePoint {
                        expiry: "2026-04-17".to_string(),
                        days_to_expiry: 58,
                        atm_strike_price: "400".to_string(),
                        atm_call_iv: Some(0.34),
                        atm_put_iv: Some(0.36),
                        atm_iv: 0.35,
                        iv_change_from_prev: Some(0.02),
                        iv_change_from_front: Some(0.02),
                    },
                ],
            },
        );

        assert_eq!(summary.delta_skew, Some(0.10));
        assert_eq!(summary.otm_skew, Some(0.08));
        assert_eq!(summary.front_atm_iv, 0.33);
        assert_eq!(summary.farthest_atm_iv, Some(0.35));
        assert_eq!(summary.term_structure_change_from_front, Some(0.02));
        assert_eq!(summary.otm_open_interest_bias_ratio, Some(2.0));
        assert_eq!(summary.downside_protection, "elevated_downside_hedging");
        assert_eq!(summary.positioning_bias, "put_heavy");
        assert!(summary.summary_sentence.contains("defensive"));
    }

    fn sample_row(
        symbol: &str,
        option_type: ContractSide,
        strike_price: &str,
        implied_volatility: &str,
        delta: f64,
        otm_percent: f64,
    ) -> ChainAnalysisRow {
        ChainAnalysisRow {
            option_symbol: symbol.to_string(),
            option_type,
            option_price: "1.0".to_string(),
            volume: 10,
            open_interest: 10,
            strike_price: strike_price.to_string(),
            implied_volatility: implied_volatility.to_string(),
            implied_volatility_source: "provider".to_string(),
            provider_reported_iv: implied_volatility.to_string(),
            diagnostics: ContractDiagnostics {
                is_liquid: true,
                otm_percent,
                ..ContractDiagnostics::default()
            },
            local_greeks: OptionMetrics {
                option_type,
                fair_value: 1.0,
                delta,
                gamma: 0.1,
                vega: 0.2,
                theta_per_day: -0.01,
                rho: 0.1,
                d1: 0.0,
                d2: 0.0,
            },
        }
    }

    fn sample_row_with_counts(
        symbol: &str,
        option_type: ContractSide,
        strike_price: &str,
        implied_volatility: &str,
        delta: f64,
        otm_percent: f64,
        volume: i64,
        open_interest: i64,
    ) -> ChainAnalysisRow {
        let mut row = sample_row(
            symbol,
            option_type,
            strike_price,
            implied_volatility,
            delta,
            otm_percent,
        );
        row.volume = volume;
        row.open_interest = open_interest;
        row
    }

    #[test]
    fn select_front_expiry_skips_expired_and_near_expiry_dates() {
        let today = time::macros::date!(2026 - 03 - 06);
        let expiry = super::select_front_expiry(
            vec![
                "2026-03-02".to_string(),
                "2026-03-06".to_string(),
                "2026-03-09".to_string(),
                "2026-03-20".to_string(),
            ],
            today,
        )
        .expect("front expiry should resolve");

        assert_eq!(expiry, time::macros::date!(2026 - 03 - 09));
    }

    #[test]
    fn select_front_expiry_falls_back_to_same_day_when_needed() {
        let today = time::macros::date!(2026 - 03 - 06);
        let expiry = super::select_front_expiry(vec!["2026-03-06".to_string()], today)
            .expect("same-day expiry should resolve");

        assert_eq!(expiry, today);
    }

    #[test]
    fn select_expiry_by_dte_prefers_nearest_target_within_range() {
        let today = time::macros::date!(2026 - 03 - 06);
        let expiry = super::select_expiry_by_dte(
            vec![
                "2026-03-09".to_string(),
                "2026-03-20".to_string(),
                "2026-04-03".to_string(),
                "2026-04-17".to_string(),
            ],
            today,
            super::ExpirySelection {
                min_days_to_expiry: 14,
                max_days_to_expiry: 45,
                target_days_to_expiry: 30,
            },
        )
        .expect("target expiry should resolve");

        assert_eq!(expiry, time::macros::date!(2026 - 04 - 03));
    }

    #[test]
    fn select_expiry_by_dte_errors_when_no_expiry_fits_range() {
        let today = time::macros::date!(2026 - 03 - 06);
        let err = super::select_expiry_by_dte(
            vec!["2026-03-09".to_string(), "2026-03-13".to_string()],
            today,
            super::ExpirySelection {
                min_days_to_expiry: 14,
                max_days_to_expiry: 45,
                target_days_to_expiry: 30,
            },
        )
        .expect_err("selection should fail");

        assert!(
            err.to_string()
                .contains("no option expiries available in 14-45 DTE range")
        );
    }

    #[test]
    fn detects_transient_quote_limit_errors() {
        assert!(super::is_transient_quote_limit_error(&anyhow::anyhow!(
            "SDK Proxy Error [option_quote]: response error: 7: detail:Some(WsResponseErrorDetail {{ code: 301607, msg: \"Too many option securities request within one minute\" }})"
        )));
        assert!(super::is_transient_quote_limit_error(&anyhow::anyhow!(
            "SDK Proxy Error [option_quote]: response error: 301606 Request rate limit"
        )));
        assert!(!super::is_transient_quote_limit_error(&anyhow::anyhow!(
            "target_price is outside solvable range"
        )));
    }
}
