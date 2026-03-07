use crate::analytics::ContractSide;
use crate::domain::ChainAnalysisRow;

#[derive(Copy, Clone, Debug, Eq, PartialEq, clap::ValueEnum)]
pub enum ChainSortField {
    Delta,
    Theta,
    Vega,
    Iv,
    Strike,
}

#[derive(Debug, Clone, Default)]
pub struct ChainScreeningRequest {
    pub side: Option<ContractSide>,
    pub min_strike: Option<f64>,
    pub max_strike: Option<f64>,
    pub min_delta: Option<f64>,
    pub max_delta: Option<f64>,
    pub min_theta: Option<f64>,
    pub max_theta: Option<f64>,
    pub min_vega: Option<f64>,
    pub max_vega: Option<f64>,
    pub min_iv: Option<f64>,
    pub max_iv: Option<f64>,
    pub min_option_price: Option<f64>,
    pub max_option_price: Option<f64>,
    pub min_otm_percent: Option<f64>,
    pub max_otm_percent: Option<f64>,
    pub only_liquid: bool,
    pub exclude_abnormal: bool,
    pub exclude_near_expiry: bool,
    pub sort_by: Option<ChainSortField>,
    pub limit: Option<usize>,
}

pub fn apply_chain_screening(
    rows: &mut Vec<ChainAnalysisRow>,
    req: &ChainScreeningRequest,
    underlying_price: f64,
) {
    rows.retain(|row| {
        matches_side(row, req.side)
            && matches_strike(row, req.min_strike, req.max_strike)
            && matches_metric(row.local_greeks.delta, req.min_delta, req.max_delta)
            && matches_metric(row.local_greeks.theta_per_day, req.min_theta, req.max_theta)
            && matches_metric(row.local_greeks.vega, req.min_vega, req.max_vega)
            && matches_metric(parsed_iv(row), req.min_iv, req.max_iv)
            && matches_metric(
                parsed_option_price(row),
                req.min_option_price,
                req.max_option_price,
            )
            && matches_otm_percent(
                row,
                underlying_price,
                req.min_otm_percent,
                req.max_otm_percent,
            )
            && matches_diagnostics(row, req)
    });
    sort_chain_rows(rows, req.sort_by);
    if let Some(limit) = req.limit {
        rows.truncate(limit);
    }
}

pub fn validate_strike_bounds(
    min_strike: Option<f64>,
    max_strike: Option<f64>,
) -> anyhow::Result<()> {
    if let (Some(min), Some(max)) = (min_strike, max_strike)
        && min > max
    {
        anyhow::bail!("min_strike must be less than or equal to max_strike");
    }
    Ok(())
}

pub fn validate_metric_bounds(
    min: Option<f64>,
    max: Option<f64>,
    label: &str,
) -> anyhow::Result<()> {
    if let (Some(min), Some(max)) = (min, max)
        && min > max
    {
        anyhow::bail!("{label} min must be less than or equal to max");
    }
    Ok(())
}

fn matches_side(row: &ChainAnalysisRow, side: Option<ContractSide>) -> bool {
    side.is_none_or(|side| row.option_type == side)
}

fn matches_strike(
    row: &ChainAnalysisRow,
    min_strike: Option<f64>,
    max_strike: Option<f64>,
) -> bool {
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

fn matches_metric(value: f64, min: Option<f64>, max: Option<f64>) -> bool {
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

fn matches_otm_percent(
    row: &ChainAnalysisRow,
    underlying_price: f64,
    min_otm_percent: Option<f64>,
    max_otm_percent: Option<f64>,
) -> bool {
    if min_otm_percent.is_none() && max_otm_percent.is_none() {
        return true;
    }

    let strike = row.strike_price.parse::<f64>().unwrap_or(f64::NAN);
    if !strike.is_finite() || underlying_price <= 0.0 {
        return false;
    }

    let otm_percent = match row.option_type {
        ContractSide::Call => (strike - underlying_price) / underlying_price,
        ContractSide::Put => (underlying_price - strike) / underlying_price,
    };

    matches_metric(otm_percent, min_otm_percent, max_otm_percent)
}

fn parsed_iv(row: &ChainAnalysisRow) -> f64 {
    row.implied_volatility.parse::<f64>().unwrap_or(f64::NAN)
}

fn parsed_option_price(row: &ChainAnalysisRow) -> f64 {
    row.option_price.parse::<f64>().unwrap_or(f64::NAN)
}

fn matches_diagnostics(row: &ChainAnalysisRow, req: &ChainScreeningRequest) -> bool {
    if req.only_liquid && !row.diagnostics.is_liquid {
        return false;
    }
    if req.exclude_abnormal
        && (row.diagnostics.halted_or_abnormal_trade_status
            || row.diagnostics.non_standard_contract
            || row.diagnostics.below_intrinsic_value)
    {
        return false;
    }
    if req.exclude_near_expiry && row.diagnostics.near_expiry {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analytics::OptionMetrics;
    use crate::domain::ContractDiagnostics;

    #[test]
    fn sorts_by_theta_desc() {
        let mut rows = vec![
            ChainAnalysisRow {
                option_symbol: "B".to_string(),
                option_type: ContractSide::Call,
                option_price: "1".to_string(),
                volume: 10,
                open_interest: 10,
                strike_price: "100".to_string(),
                implied_volatility: "0.2".to_string(),
                implied_volatility_source: "provider".to_string(),
                provider_reported_iv: "0.2".to_string(),
                diagnostics: ContractDiagnostics::default(),
                local_greeks: OptionMetrics {
                    option_type: ContractSide::Call,
                    fair_value: 1.0,
                    delta: 0.5,
                    gamma: 0.1,
                    vega: 0.2,
                    theta_per_day: -0.03,
                    rho: 0.1,
                    d1: 0.0,
                    d2: 0.0,
                },
            },
            ChainAnalysisRow {
                option_symbol: "A".to_string(),
                option_type: ContractSide::Call,
                option_price: "1".to_string(),
                volume: 10,
                open_interest: 10,
                strike_price: "100".to_string(),
                implied_volatility: "0.2".to_string(),
                implied_volatility_source: "provider".to_string(),
                provider_reported_iv: "0.2".to_string(),
                diagnostics: ContractDiagnostics::default(),
                local_greeks: OptionMetrics {
                    option_type: ContractSide::Call,
                    fair_value: 1.0,
                    delta: 0.5,
                    gamma: 0.1,
                    vega: 0.2,
                    theta_per_day: -0.01,
                    rho: 0.1,
                    d1: 0.0,
                    d2: 0.0,
                },
            },
        ];

        apply_chain_screening(
            &mut rows,
            &ChainScreeningRequest {
                side: None,
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
                only_liquid: false,
                exclude_abnormal: false,
                exclude_near_expiry: false,
                sort_by: Some(ChainSortField::Theta),
                limit: None,
            },
            100.0,
        );

        assert_eq!(rows[0].option_symbol, "A");
        assert_eq!(rows[1].option_symbol, "B");
    }

    #[test]
    fn filters_by_delta_range() {
        let mut rows = vec![
            ChainAnalysisRow {
                option_symbol: "A".to_string(),
                option_type: ContractSide::Call,
                option_price: "1".to_string(),
                volume: 10,
                open_interest: 10,
                strike_price: "100".to_string(),
                implied_volatility: "0.2".to_string(),
                implied_volatility_source: "provider".to_string(),
                provider_reported_iv: "0.2".to_string(),
                diagnostics: ContractDiagnostics::default(),
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
            },
            ChainAnalysisRow {
                option_symbol: "B".to_string(),
                option_type: ContractSide::Call,
                option_price: "1".to_string(),
                volume: 10,
                open_interest: 10,
                strike_price: "100".to_string(),
                implied_volatility: "0.2".to_string(),
                implied_volatility_source: "provider".to_string(),
                provider_reported_iv: "0.2".to_string(),
                diagnostics: ContractDiagnostics::default(),
                local_greeks: OptionMetrics {
                    option_type: ContractSide::Call,
                    fair_value: 1.0,
                    delta: 0.6,
                    gamma: 0.1,
                    vega: 0.2,
                    theta_per_day: -0.01,
                    rho: 0.1,
                    d1: 0.0,
                    d2: 0.0,
                },
            },
        ];

        apply_chain_screening(
            &mut rows,
            &ChainScreeningRequest {
                side: None,
                min_strike: None,
                max_strike: None,
                min_delta: Some(0.5),
                max_delta: Some(0.7),
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
                only_liquid: false,
                exclude_abnormal: false,
                exclude_near_expiry: false,
                sort_by: None,
                limit: None,
            },
            100.0,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].option_symbol, "B");
    }

    #[test]
    fn filters_out_illiquid_rows() {
        let mut rows = vec![
            ChainAnalysisRow {
                option_symbol: "A".to_string(),
                option_type: ContractSide::Put,
                option_price: "1".to_string(),
                volume: 0,
                open_interest: 10,
                strike_price: "100".to_string(),
                implied_volatility: "0.2".to_string(),
                implied_volatility_source: "provider".to_string(),
                provider_reported_iv: "0.2".to_string(),
                diagnostics: ContractDiagnostics {
                    is_liquid: false,
                    liquidity_flags: vec!["zero_volume".to_string()],
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
            },
            ChainAnalysisRow {
                option_symbol: "B".to_string(),
                option_type: ContractSide::Put,
                option_price: "1".to_string(),
                volume: 10,
                open_interest: 10,
                strike_price: "100".to_string(),
                implied_volatility: "0.2".to_string(),
                implied_volatility_source: "provider".to_string(),
                provider_reported_iv: "0.2".to_string(),
                diagnostics: ContractDiagnostics {
                    is_liquid: true,
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
            },
        ];

        apply_chain_screening(
            &mut rows,
            &ChainScreeningRequest {
                side: None,
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
                exclude_abnormal: false,
                exclude_near_expiry: false,
                sort_by: None,
                limit: None,
            },
            100.0,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].option_symbol, "B");
    }

    #[test]
    fn exclude_abnormal_filters_below_intrinsic_rows() {
        let mut rows = vec![
            ChainAnalysisRow {
                option_symbol: "A".to_string(),
                option_type: ContractSide::Call,
                option_price: "40".to_string(),
                volume: 10,
                open_interest: 10,
                strike_price: "350".to_string(),
                implied_volatility: "0.2".to_string(),
                implied_volatility_source: "provider".to_string(),
                provider_reported_iv: "0.2".to_string(),
                diagnostics: ContractDiagnostics {
                    below_intrinsic_value: true,
                    extrinsic_value: -10.0,
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
            },
            ChainAnalysisRow {
                option_symbol: "B".to_string(),
                option_type: ContractSide::Call,
                option_price: "55".to_string(),
                volume: 10,
                open_interest: 10,
                strike_price: "350".to_string(),
                implied_volatility: "0.2".to_string(),
                implied_volatility_source: "provider".to_string(),
                provider_reported_iv: "0.2".to_string(),
                diagnostics: ContractDiagnostics::default(),
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
            },
        ];

        apply_chain_screening(
            &mut rows,
            &ChainScreeningRequest {
                exclude_abnormal: true,
                ..ChainScreeningRequest::default()
            },
            400.0,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].option_symbol, "B");
    }
}
