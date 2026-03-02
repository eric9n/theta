use anyhow::{bail, Result};
use clap::ValueEnum;
use serde::Serialize;
use std::f64::consts::{PI, SQRT_2};

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContractSide {
    Call,
    Put,
}

#[derive(Copy, Clone, Debug)]
pub struct PricingInput {
    pub spot: f64,
    pub strike: f64,
    pub rate: f64,
    pub volatility: f64,
    pub time_to_expiry_years: f64,
    pub dividend: f64,
    pub option_type: ContractSide,
}

#[derive(Copy, Clone, Debug, Serialize)]
pub struct OptionMetrics {
    pub option_type: ContractSide,
    pub fair_value: f64,
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta_per_day: f64,
    pub rho: f64,
    pub d1: f64,
    pub d2: f64,
}

pub const MIN_VOLATILITY: f64 = 1.0e-4;
pub const MAX_VOLATILITY: f64 = 5.0;
const IV_TOLERANCE: f64 = 1.0e-8;
const IV_MAX_ITERATIONS: usize = 80;

impl PricingInput {
    pub fn new(
        spot: f64,
        strike: f64,
        rate: f64,
        volatility: f64,
        days: f64,
        dividend: f64,
        option_type: ContractSide,
    ) -> Result<Self> {
        if spot <= 0.0 {
            bail!("spot must be greater than 0");
        }
        if strike <= 0.0 {
            bail!("strike must be greater than 0");
        }
        if volatility <= 0.0 {
            bail!("volatility must be greater than 0");
        }
        if days <= 0.0 {
            bail!("days must be greater than 0");
        }

        Ok(Self {
            spot,
            strike,
            rate,
            volatility,
            time_to_expiry_years: days / 365.0,
            dividend,
            option_type,
        })
    }
}

#[inline]
pub fn calculate_metrics(input: &PricingInput) -> OptionMetrics {
    let t = input.time_to_expiry_years;
    let sqrt_t = t.sqrt();
    let sigma_sqrt_t = input.volatility * sqrt_t;
    let log_moneyness = (input.spot / input.strike).ln();
    let carry = input.rate - input.dividend;

    let d1 = (log_moneyness + (carry + 0.5 * input.volatility.powi(2)) * t) / sigma_sqrt_t;
    let d2 = d1 - sigma_sqrt_t;

    let discount_dividend = (-input.dividend * t).exp();
    let discount_rate = (-input.rate * t).exp();
    let discounted_spot = input.spot * discount_dividend;
    let discounted_strike = input.strike * discount_rate;
    let pdf_d1 = normal_pdf(d1);

    let (fair_value, delta, theta_annual, rho) = match input.option_type {
        ContractSide::Call => {
            let fair_value = discounted_spot * normal_cdf(d1) - discounted_strike * normal_cdf(d2);
            let delta = discount_dividend * normal_cdf(d1);
            let theta = -(discounted_spot * pdf_d1 * input.volatility) / (2.0 * sqrt_t)
                - input.rate * discounted_strike * normal_cdf(d2)
                + input.dividend * discounted_spot * normal_cdf(d1);
            let rho = input.strike * t * discount_rate * normal_cdf(d2) / 100.0;
            (fair_value, delta, theta, rho)
        }
        ContractSide::Put => {
            let fair_value =
                discounted_strike * normal_cdf(-d2) - discounted_spot * normal_cdf(-d1);
            let delta = discount_dividend * (normal_cdf(d1) - 1.0);
            let theta = -(discounted_spot * pdf_d1 * input.volatility) / (2.0 * sqrt_t)
                + input.rate * discounted_strike * normal_cdf(-d2)
                - input.dividend * discounted_spot * normal_cdf(-d1);
            let rho = -input.strike * t * discount_rate * normal_cdf(-d2) / 100.0;
            (fair_value, delta, theta, rho)
        }
    };

    let gamma = discount_dividend * pdf_d1 / (input.spot * sigma_sqrt_t);
    let vega = input.spot * discount_dividend * pdf_d1 * sqrt_t / 100.0;

    OptionMetrics {
        option_type: input.option_type,
        fair_value,
        delta,
        gamma,
        vega,
        theta_per_day: theta_annual / 365.0,
        rho,
        d1,
        d2,
    }
}

pub fn calculate_metrics_batch(inputs: &[PricingInput]) -> Vec<OptionMetrics> {
    let mut out = Vec::with_capacity(inputs.len());
    calculate_metrics_batch_into(inputs, &mut out);
    out
}

pub fn calculate_metrics_batch_into(inputs: &[PricingInput], out: &mut Vec<OptionMetrics>) {
    out.clear();
    out.reserve(inputs.len().saturating_sub(out.capacity()));

    for input in inputs {
        out.push(calculate_metrics(input));
    }
}

pub fn option_price(input: &PricingInput) -> f64 {
    calculate_metrics(input).fair_value
}

pub fn implied_volatility_from_price(
    spot: f64,
    strike: f64,
    rate: f64,
    days: f64,
    dividend: f64,
    option_type: ContractSide,
    target_price: f64,
) -> Result<f64> {
    if target_price <= 0.0 {
        bail!("target_price must be greater than 0");
    }

    let mut low = MIN_VOLATILITY;
    let mut high = MAX_VOLATILITY;
    let mut low_price = option_price(&PricingInput::new(
        spot,
        strike,
        rate,
        low,
        days,
        dividend,
        option_type,
    )?);
    let high_price = option_price(&PricingInput::new(
        spot,
        strike,
        rate,
        high,
        days,
        dividend,
        option_type,
    )?);

    if target_price < low_price || target_price > high_price {
        bail!(
            "target_price is outside solvable range for current model assumptions"
        );
    }

    for _ in 0..IV_MAX_ITERATIONS {
        let mid = (low + high) * 0.5;
        let mid_price = option_price(&PricingInput::new(
            spot,
            strike,
            rate,
            mid,
            days,
            dividend,
            option_type,
        )?);
        let error = mid_price - target_price;

        if error.abs() <= IV_TOLERANCE {
            return Ok(mid);
        }

        if error > 0.0 {
            high = mid;
        } else {
            low = mid;
            low_price = mid_price;
        }

        if (high - low) <= IV_TOLERANCE || (target_price - low_price).abs() <= IV_TOLERANCE {
            return Ok((low + high) * 0.5);
        }
    }

    Ok((low + high) * 0.5)
}

#[inline]
fn normal_pdf(x: f64) -> f64 {
    (-(x * x) / 2.0).exp() / (2.0 * PI).sqrt()
}

#[inline]
fn normal_cdf(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / SQRT_2))
}

#[inline]
fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();

    let a1 = 0.254_829_592;
    let a2 = -0.284_496_736;
    let a3 = 1.421_413_741;
    let a4 = -1.453_152_027;
    let a5 = 1.061_405_429;
    let p = 0.327_591_1;

    let t = 1.0 / (1.0 + p * x);
    let y = 1.0
        - (((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t * (-(x * x)).exp());

    sign * y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_call_metrics() {
        let input = PricingInput::new(100.0, 100.0, 0.05, 0.2, 30.0, 0.0, ContractSide::Call)
            .expect("valid input");
        let metrics = calculate_metrics(&input);

        assert!(metrics.fair_value > 2.0);
        assert!(metrics.delta > 0.5);
        assert!(metrics.gamma > 0.0);
        assert!(metrics.vega > 0.0);
        assert!(metrics.theta_per_day < 0.0);
    }

    #[test]
    fn calculates_put_metrics() {
        let input = PricingInput::new(100.0, 105.0, 0.03, 0.25, 45.0, 0.0, ContractSide::Put)
            .expect("valid input");
        let metrics = calculate_metrics(&input);

        assert!(metrics.fair_value > 0.0);
        assert!(metrics.delta < 0.0);
        assert!(metrics.gamma > 0.0);
        assert!(metrics.theta_per_day < 0.0);
    }

    #[test]
    fn batch_path_matches_scalar_path() {
        let inputs = [
            PricingInput::new(100.0, 100.0, 0.05, 0.2, 30.0, 0.0, ContractSide::Call)
                .expect("valid input"),
            PricingInput::new(100.0, 105.0, 0.03, 0.25, 45.0, 0.0, ContractSide::Put)
                .expect("valid input"),
        ];

        let batch = calculate_metrics_batch(&inputs);

        assert_eq!(batch.len(), inputs.len());
        assert_eq!(batch[0].fair_value, calculate_metrics(&inputs[0]).fair_value);
        assert_eq!(batch[1].delta, calculate_metrics(&inputs[1]).delta);
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert!(PricingInput::new(0.0, 100.0, 0.03, 0.2, 30.0, 0.0, ContractSide::Call).is_err());
        assert!(PricingInput::new(100.0, 0.0, 0.03, 0.2, 30.0, 0.0, ContractSide::Call).is_err());
        assert!(PricingInput::new(100.0, 100.0, 0.03, 0.0, 30.0, 0.0, ContractSide::Call).is_err());
    }

    #[test]
    fn solves_implied_volatility_from_price() {
        let input = PricingInput::new(100.0, 100.0, 0.05, 0.24, 30.0, 0.0, ContractSide::Call)
            .expect("valid input");
        let price = option_price(&input);
        let iv = implied_volatility_from_price(
            input.spot,
            input.strike,
            input.rate,
            30.0,
            input.dividend,
            input.option_type,
            price,
        )
        .expect("solvable");

        assert!((iv - 0.24).abs() < 1.0e-6);
    }
}
