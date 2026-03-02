use crate::rate::RateCurve;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AppConfigFile {
    #[serde(default)]
    pub rate_curve: Option<RateCurveConfig>,
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
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let mut rate_curve = RateCurve::default();
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
            }
        }
        Ok(Self { rate_curve })
    }
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
