use crate::rate::RateCurve;
use anyhow::{Context, Result, bail};
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
        if let Some((path, explicit)) = config_path() {
            if explicit && !path.exists() {
                bail!(
                    "explicit theta config file does not exist: {}",
                    path.display()
                );
            }
            if path.exists() {
                let raw = fs::read_to_string(&path)
                    .with_context(|| format!("failed to read config file {}", path.display()))?;
                let parsed: AppConfigFile = serde_json::from_str(&raw)
                    .with_context(|| format!("failed to parse config file {}", path.display()))?;
                if let Some(cfg) = parsed.rate_curve {
                    if let Some(v) = cfg.short_rate {
                        rate_curve.short_rate = validate_finite(v, "rate_curve.short_rate")?;
                    }
                    if let Some(v) = cfg.medium_rate {
                        rate_curve.medium_rate = validate_finite(v, "rate_curve.medium_rate")?;
                    }
                    if let Some(v) = cfg.long_rate {
                        rate_curve.long_rate = validate_finite(v, "rate_curve.long_rate")?;
                    }
                }
                if let Some(v) = parsed.default_dividend_yield {
                    default_dividend_yield = validate_dividend_yield(v, "default_dividend_yield")?;
                }
                if let Some(cfg) = parsed.dividend_yields {
                    let mut normalized = HashMap::with_capacity(cfg.len());
                    for (symbol, yield_rate) in cfg {
                        let normalized_symbol = normalize_symbol_key(&symbol);
                        if normalized_symbol.is_empty() {
                            bail!("dividend_yields contains an empty symbol key");
                        }
                        normalized.insert(
                            normalized_symbol,
                            validate_dividend_yield(
                                yield_rate,
                                &format!("dividend_yields.{symbol}"),
                            )?,
                        );
                    }
                    dividend_yields = normalized;
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

fn validate_finite(value: f64, field: &str) -> Result<f64> {
    if !value.is_finite() {
        bail!("{field} must be finite");
    }
    Ok(value)
}

fn validate_dividend_yield(value: f64, field: &str) -> Result<f64> {
    let value = validate_finite(value, field)?;
    if value < 0.0 {
        bail!("{field} must be greater than or equal to 0");
    }
    Ok(value)
}

fn config_path() -> Option<(PathBuf, bool)> {
    if let Ok(explicit) = std::env::var("THETA_CONFIG") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Some((PathBuf::from(trimmed), true));
        }
    }

    let home = std::env::var("HOME").ok()?;
    Some((
        PathBuf::from(home).join(".theta").join("config.json"),
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_config_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("theta-config-{name}-{nanos}.json"))
    }

    #[test]
    fn normalizes_dividend_symbol_keys() {
        assert_eq!(normalize_symbol_key("spy"), "SPY");
        assert_eq!(normalize_symbol_key("SPY.US"), "SPY");
        assert_eq!(normalize_symbol_key(" tsla.us "), "TSLA");
    }

    #[test]
    fn explicit_missing_config_path_errors() {
        let _guard = env_lock().lock().expect("env lock");
        let missing = temp_config_path("missing");
        unsafe { std::env::set_var("THETA_CONFIG", &missing) };
        let result = AppConfig::load();
        unsafe { std::env::remove_var("THETA_CONFIG") };

        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("explicit theta config file does not exist")
        );
    }

    #[test]
    fn rejects_negative_dividend_yield() {
        let _guard = env_lock().lock().expect("env lock");
        let path = temp_config_path("negative-dividend");
        fs::write(&path, r#"{ "default_dividend_yield": -0.01 }"#).expect("write config");
        unsafe { std::env::set_var("THETA_CONFIG", &path) };

        let result = AppConfig::load();

        unsafe { std::env::remove_var("THETA_CONFIG") };
        let _ = fs::remove_file(&path);
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("default_dividend_yield must be greater than or equal to 0")
        );
    }

    #[test]
    fn rejects_non_finite_rate_curve_values() {
        assert!(validate_finite(f64::INFINITY, "rate_curve.long_rate").is_err());
        assert!(validate_finite(f64::NAN, "rate_curve.long_rate").is_err());
        assert!(validate_dividend_yield(-0.01, "default_dividend_yield").is_err());
    }

    #[test]
    fn loads_and_normalizes_dividend_map() {
        let _guard = env_lock().lock().expect("env lock");
        let path = temp_config_path("dividend-map");
        fs::write(
            &path,
            r#"{
                "default_dividend_yield": 0.01,
                "dividend_yields": { " spy.us ": 0.02, "QQQ": 0.005 }
            }"#,
        )
        .expect("write config");
        unsafe { std::env::set_var("THETA_CONFIG", &path) };

        let config = AppConfig::load().expect("config loads");

        unsafe { std::env::remove_var("THETA_CONFIG") };
        let _ = fs::remove_file(&path);
        assert_eq!(config.default_dividend_yield, 0.01);
        assert_eq!(config.dividend_yields.get("SPY"), Some(&0.02));
        assert_eq!(config.dividend_yields.get("QQQ"), Some(&0.005));
    }
}
