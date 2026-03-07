use crate::rate::RateCurve;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AppConfigFile {
    #[serde(default)]
    pub rate_curve: Option<RateCurveConfig>,
    #[serde(default)]
    pub default_dividend_yield: Option<f64>,
    #[serde(default)]
    pub dividend_yields: Option<HashMap<String, f64>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateCurveConfig {
    pub short_rate: Option<f64>,
    pub medium_rate: Option<f64>,
    pub long_rate: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub rate_curve: RateCurve,
    pub default_dividend_yield: f64,
    pub dividend_yields: HashMap<String, f64>,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let mut rate_curve = RateCurve::default();
        let mut default_dividend_yield = 0.0;
        let mut dividend_yields = HashMap::new();
        if let Some(path) = config_path() {
            if path.exists() {
                let raw = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read config file {}", path.display()))?;
                let parsed: AppConfigFile = serde_json::from_str(&raw)
                    .with_context(|| format!("failed to parse config file {}", path.display()))?;
                if let Some(cfg) = parsed.rate_curve {
                    if let Some(v) = cfg.short_rate {
                        rate_curve.short_rate = v;
                    }
                    if let Some(v) = cfg.medium_rate {
                        rate_curve.medium_rate = v;
                    }
                    if let Some(v) = cfg.long_rate {
                        rate_curve.long_rate = v;
                    }
                }
                if let Some(v) = parsed.default_dividend_yield {
                    default_dividend_yield = v;
                }
                if let Some(cfg) = parsed.dividend_yields {
                    dividend_yields = cfg
                        .into_iter()
                        .map(|(symbol, yield_rate)| (normalize_symbol_key(&symbol), yield_rate))
                        .collect();
                }
            }
        }
        Ok(Self {
            rate_curve,
            default_dividend_yield,
            dividend_yields,
        })
    }
}

fn normalize_symbol_key(symbol: &str) -> String {
    let upper = symbol.trim().to_ascii_uppercase();
    upper.trim_end_matches(".US").to_string()
}

fn config_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("THETA_CONFIG") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".theta").join("config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_dividend_symbol_keys() {
        assert_eq!(normalize_symbol_key("spy"), "SPY");
        assert_eq!(normalize_symbol_key("SPY.US"), "SPY");
        assert_eq!(normalize_symbol_key(" tsla.us "), "TSLA");
    }
}
