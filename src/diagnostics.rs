use crate::analytics::ContractSide;
use crate::domain::{
    ContractDiagnostics, NormalizedOptionChainSnapshot, OptionContractSnapshot,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct NormalizedChainDiagnosticsFilter {
    pub only_liquid: bool,
    pub exclude_abnormal: bool,
    pub exclude_near_expiry: bool,
}

pub fn analyze_contract(
    contract: &OptionContractSnapshot,
    underlying_price: f64,
    days_to_expiry: i64,
) -> ContractDiagnostics {
    let zero_last_done = contract.last_done_f64 <= 0.0;
    let zero_volume = contract.volume <= 0;
    let zero_open_interest = contract.open_interest <= 0;
    let non_positive_iv = contract.provider_reported_iv_f64 <= 0.0;
    let non_standard_contract = !matches!(
        contract.contract_style.as_str(),
        "American" | "European" | "Europe"
    );
    let halted_or_abnormal_trade_status = contract.trade_status.as_str() != "Normal";
    let near_expiry = days_to_expiry <= 1;
    let is_liquid = !zero_last_done && !zero_volume && !zero_open_interest;

    let otm_percent = if underlying_price > 0.0 {
        match contract.option_type {
            ContractSide::Call => (contract.strike_price_f64 - underlying_price) / underlying_price,
            ContractSide::Put => (underlying_price - contract.strike_price_f64) / underlying_price,
        }
    } else {
        f64::NAN
    };

    let intrinsic_value = match contract.option_type {
        ContractSide::Call => (underlying_price - contract.strike_price_f64).max(0.0),
        ContractSide::Put => (contract.strike_price_f64 - underlying_price).max(0.0),
    };
    let extrinsic_value = (contract.last_done_f64 - intrinsic_value).max(0.0);
    let breakeven = match contract.option_type {
        ContractSide::Call => contract.strike_price_f64 + contract.last_done_f64,
        ContractSide::Put => contract.strike_price_f64 - contract.last_done_f64,
    };

    let mut quality_flags = Vec::new();
    if non_positive_iv {
        quality_flags.push("non_positive_iv".to_string());
    }
    if non_standard_contract {
        quality_flags.push("non_standard_contract".to_string());
    }
    if halted_or_abnormal_trade_status {
        quality_flags.push("abnormal_trade_status".to_string());
    }
    if near_expiry {
        quality_flags.push("near_expiry".to_string());
    }

    let mut liquidity_flags = Vec::new();
    if zero_last_done {
        liquidity_flags.push("zero_last_done".to_string());
    }
    if zero_volume {
        liquidity_flags.push("zero_volume".to_string());
    }
    if zero_open_interest {
        liquidity_flags.push("zero_open_interest".to_string());
    }

    ContractDiagnostics {
        zero_last_done,
        zero_volume,
        zero_open_interest,
        non_positive_iv,
        non_standard_contract,
        halted_or_abnormal_trade_status,
        near_expiry,
        is_liquid,
        otm_percent,
        intrinsic_value,
        extrinsic_value,
        breakeven,
        quality_flags,
        liquidity_flags,
    }
}

pub fn apply_normalized_chain_diagnostics_filter(
    chain: &mut NormalizedOptionChainSnapshot,
    filter: NormalizedChainDiagnosticsFilter,
) {
    for row in &mut chain.rows {
        if row
            .call_diagnostics
            .as_ref()
            .is_some_and(|diagnostics| !matches_filter(diagnostics, filter))
        {
            row.call = None;
            row.call_diagnostics = None;
        }
        if row
            .put_diagnostics
            .as_ref()
            .is_some_and(|diagnostics| !matches_filter(diagnostics, filter))
        {
            row.put = None;
            row.put_diagnostics = None;
        }
    }

    chain.rows.retain(|row| row.call.is_some() || row.put.is_some());
}

fn matches_filter(
    diagnostics: &ContractDiagnostics,
    filter: NormalizedChainDiagnosticsFilter,
) -> bool {
    if filter.only_liquid && !diagnostics.is_liquid {
        return false;
    }
    if filter.exclude_abnormal
        && (diagnostics.halted_or_abnormal_trade_status || diagnostics.non_standard_contract)
    {
        return false;
    }
    if filter.exclude_near_expiry && diagnostics.near_expiry {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{NormalizedOptionChainSnapshot, OptionChainStrikeRow, UnderlyingSnapshot};
    use time::Date;

    #[test]
    fn computes_basic_contract_diagnostics() {
        let contract = OptionContractSnapshot {
            symbol: "TSLA_TEST_P".to_string(),
            underlying_symbol: "TSLA.US".to_string(),
            option_type: ContractSide::Put,
            last_done: "2.5".to_string(),
            last_done_f64: 2.5,
            prev_close: "2.0".to_string(),
            prev_close_f64: 2.0,
            open: "2.1".to_string(),
            open_f64: 2.1,
            high: "2.8".to_string(),
            high_f64: 2.8,
            low: "1.9".to_string(),
            low_f64: 1.9,
            timestamp: "2026-02-28 09:30:00".to_string(),
            volume: 10,
            turnover: "2500".to_string(),
            turnover_f64: 2500.0,
            trade_status: "Normal".to_string(),
            strike_price: "380".to_string(),
            strike_price_f64: 380.0,
            expiry: Date::from_calendar_date(2026, time::Month::March, 20).expect("valid date"),
            provider_reported_iv: "0.4".to_string(),
            provider_reported_iv_f64: 0.4,
            open_interest: 100,
            historical_volatility: "0.35".to_string(),
            historical_volatility_f64: 0.35,
            contract_multiplier: "100".to_string(),
            contract_multiplier_f64: 100.0,
            contract_size: "100".to_string(),
            contract_size_f64: 100.0,
            contract_style: "American".to_string(),
        };

        let diagnostics = analyze_contract(&contract, 400.0, 20);
        assert!(diagnostics.is_liquid);
        assert!(!diagnostics.near_expiry);
        assert!((diagnostics.otm_percent - 0.05).abs() < 1e-9);
        assert_eq!(diagnostics.intrinsic_value, 0.0);
        assert_eq!(diagnostics.extrinsic_value, 2.5);
        assert_eq!(diagnostics.breakeven, 377.5);
    }

    #[test]
    fn filters_normalized_chain_legs_by_diagnostics() {
        let mut chain = NormalizedOptionChainSnapshot {
            underlying: UnderlyingSnapshot {
                symbol: "TSLA.US".to_string(),
                last_done: "400".to_string(),
                last_done_f64: 400.0,
                prev_close: "390".to_string(),
                prev_close_f64: 390.0,
                open: "395".to_string(),
                open_f64: 395.0,
                high: "405".to_string(),
                high_f64: 405.0,
                low: "392".to_string(),
                low_f64: 392.0,
                volume: 1,
                turnover: "1".to_string(),
                turnover_f64: 1.0,
                timestamp: "2026-02-28 09:30:00".to_string(),
            },
            expiry: Date::from_calendar_date(2026, time::Month::March, 20).expect("valid date"),
            days_to_expiry: 20,
            rows: vec![OptionChainStrikeRow {
                strike_price: "400".to_string(),
                strike_price_f64: 400.0,
                call: Some(build_contract("C1", ContractSide::Call)),
                call_diagnostics: Some(ContractDiagnostics {
                    is_liquid: false,
                    ..ContractDiagnostics::default()
                }),
                put: Some(build_contract("P1", ContractSide::Put)),
                put_diagnostics: Some(ContractDiagnostics {
                    is_liquid: true,
                    ..ContractDiagnostics::default()
                }),
            }],
        };

        apply_normalized_chain_diagnostics_filter(
            &mut chain,
            NormalizedChainDiagnosticsFilter {
                only_liquid: true,
                ..NormalizedChainDiagnosticsFilter::default()
            },
        );

        assert_eq!(chain.rows.len(), 1);
        assert!(chain.rows[0].call.is_none());
        assert!(chain.rows[0].call_diagnostics.is_none());
        assert!(chain.rows[0].put.is_some());
    }

    fn build_contract(symbol: &str, option_type: ContractSide) -> OptionContractSnapshot {
        OptionContractSnapshot {
            symbol: symbol.to_string(),
            underlying_symbol: "TSLA.US".to_string(),
            option_type,
            last_done: "2.5".to_string(),
            last_done_f64: 2.5,
            prev_close: "2.0".to_string(),
            prev_close_f64: 2.0,
            open: "2.1".to_string(),
            open_f64: 2.1,
            high: "2.8".to_string(),
            high_f64: 2.8,
            low: "1.9".to_string(),
            low_f64: 1.9,
            timestamp: "2026-02-28 09:30:00".to_string(),
            volume: 10,
            turnover: "2500".to_string(),
            turnover_f64: 2500.0,
            trade_status: "Normal".to_string(),
            strike_price: "400".to_string(),
            strike_price_f64: 400.0,
            expiry: Date::from_calendar_date(2026, time::Month::March, 20).expect("valid date"),
            provider_reported_iv: "0.4".to_string(),
            provider_reported_iv_f64: 0.4,
            open_interest: 100,
            historical_volatility: "0.35".to_string(),
            historical_volatility_f64: 0.35,
            contract_multiplier: "100".to_string(),
            contract_multiplier_f64: 100.0,
            contract_size: "100".to_string(),
            contract_size_f64: 100.0,
            contract_style: "American".to_string(),
        }
    }
}
