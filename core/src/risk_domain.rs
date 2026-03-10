use crate::analytics::OptionMetrics;
use serde::Serialize;

// ---------------------------------------------------------------------------
// Strategy types (matching Firstrade categories)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum StrategyKind {
    CoveredCall,
    CashSecuredPut,
    BullPutSpread,
    BearCallSpread,
    BullCallSpread,
    BearPutSpread,
    CalendarCallSpread,
    CalendarPutSpread,
    DiagonalCallSpread,
    DiagonalPutSpread,
    Straddle,
    Strangle,
    IronCondor,
    Butterfly,
    LongCall,
    LongPut,
    NakedPut,
    /// Standalone position that couldn't be matched into a strategy
    Unmatched,
}

impl std::fmt::Display for StrategyKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            StrategyKind::CoveredCall => "Covered Call",
            StrategyKind::CashSecuredPut => "Cash-Secured Put",
            StrategyKind::BullPutSpread => "Bull Put Spread",
            StrategyKind::BearCallSpread => "Bear Call Spread",
            StrategyKind::BullCallSpread => "Bull Call Spread",
            StrategyKind::BearPutSpread => "Bear Put Spread",
            StrategyKind::CalendarCallSpread => "Calendar Call Spread",
            StrategyKind::CalendarPutSpread => "Calendar Put Spread",
            StrategyKind::DiagonalCallSpread => "Diagonal Call Spread",
            StrategyKind::DiagonalPutSpread => "Diagonal Put Spread",
            StrategyKind::Straddle => "Straddle",
            StrategyKind::Strangle => "Strangle",
            StrategyKind::IronCondor => "Iron Condor",
            StrategyKind::Butterfly => "Butterfly",
            StrategyKind::LongCall => "Long Call",
            StrategyKind::LongPut => "Long Put",
            StrategyKind::NakedPut => "Naked Put",
            StrategyKind::Unmatched => "Unmatched",
        };
        write!(f, "{}", s)
    }
}

// ---------------------------------------------------------------------------
// Position with live data enriched
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct EnrichedPosition {
    pub symbol: String,
    pub underlying: String,
    /// Live underlying spot price when available
    pub underlying_spot: Option<f64>,
    /// "call", "put", or "stock"
    pub side: String,
    pub strike: Option<f64>,
    pub expiry: Option<String>,
    /// Positive = long, negative = short
    pub net_quantity: i64,
    pub avg_cost: f64,
    /// Current market price (from LongPort or manual)
    pub current_price: f64,
    /// Unrealized P&L per unit
    pub unrealized_pnl_per_unit: f64,
    /// Total unrealized P&L (quantity × per-unit × multiplier)
    pub unrealized_pnl: f64,
    /// Option Greeks (None for stock positions)
    pub greeks: Option<OptionMetrics>,
}

// ---------------------------------------------------------------------------
// Identified strategy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct IdentifiedStrategy {
    pub kind: StrategyKind,
    pub underlying: String,
    pub legs: Vec<StrategyLeg>,
    pub margin: StrategyMargin,
    pub max_profit: Option<f64>,
    pub max_loss: Option<f64>,
    pub breakeven: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StrategyLeg {
    pub symbol: String,
    pub side: String, // "call", "put", "stock"
    pub strike: Option<f64>,
    pub expiry: Option<String>,
    pub quantity: i64, // signed: + long, - short
    pub price: f64,    // current price
}

// ---------------------------------------------------------------------------
// Margin
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct StrategyMargin {
    /// Margin requirement for this strategy
    pub margin_required: f64,
    /// Description of how margin was calculated
    pub method: String,
}

// ---------------------------------------------------------------------------
// Portfolio-level Greeks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Default)]
pub struct PortfolioGreeks {
    /// Net delta in shares (delta × quantity × multiplier)
    pub net_delta_shares: f64,
    /// Total gamma
    pub total_gamma: f64,
    /// Total theta per day ($)
    pub total_theta_per_day: f64,
    /// Total vega
    pub total_vega: f64,
}

// ---------------------------------------------------------------------------
// Per-underlying risk exposure
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct UnderlyingExposure {
    pub underlying: String,
    pub spot_price: f64,
    /// Net delta exposure in dollar terms
    pub delta_exposure_dollars: f64,
    /// Net delta in shares
    pub delta_shares: f64,
    /// Sum of all position P&L for this underlying
    pub unrealized_pnl: f64,
    /// Sum of margin for strategies on this underlying
    pub total_margin: f64,
    /// Strategies on this underlying
    pub strategies: Vec<IdentifiedStrategy>,
}

// ---------------------------------------------------------------------------
// Full report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioRiskReport {
    pub positions: Vec<EnrichedPosition>,
    pub strategies: Vec<IdentifiedStrategy>,
    pub portfolio_greeks: PortfolioGreeks,
    pub exposures: Vec<UnderlyingExposure>,
    pub total_margin_required: f64,
    pub total_unrealized_pnl: f64,
}
