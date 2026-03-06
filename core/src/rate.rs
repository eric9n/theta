use serde::Deserialize;

#[derive(Copy, Clone, Debug, Deserialize)]
pub struct RateCurve {
    pub short_rate: f64,
    pub medium_rate: f64,
    pub long_rate: f64,
}

impl Default for RateCurve {
    fn default() -> Self {
        Self {
            short_rate: 0.04,
            medium_rate: 0.0425,
            long_rate: 0.045,
        }
    }
}

impl RateCurve {
    pub fn rate_for_days(&self, days_to_expiry: i64) -> f64 {
        match days_to_expiry {
            i64::MIN..=90 => self.short_rate,
            91..=180 => self.medium_rate,
            _ => self.long_rate,
        }
    }
}
