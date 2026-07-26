//! Explainable hyperlocal wildfire ignition scoring.

use chrono::{DateTime, Datelike as _, NaiveDate, Utc, Weekday};
use grid::CellIndex;
use serde::{Deserialize, Serialize};

/// Stable ordered feature set used by the learned human-ignition component.
pub const HUMAN_FEATURE_NAMES: [&str; 11] = [
    "wildland_urban_interface",
    "road_density",
    "agricultural_activity",
    "population",
    "points_of_interest",
    "power_lines",
    "weekend",
    "school_holiday",
    "public_holiday",
    "season_sine",
    "season_cosine",
];

/// Number of inputs expected by [`LearnedHumanModel`].
pub const HUMAN_FEATURE_COUNT: usize = HUMAN_FEATURE_NAMES.len();

/// A replaceable ignition-scoring model.
pub trait IgnitionModel: Send + Sync {
    /// Scores one H3 cell from its physical and human features.
    fn score(
        &self,
        cell: CellIndex,
        features: &CellFeatures,
        computed_at: DateTime<Utc>,
    ) -> RiskScore;
}

/// Runtime inputs needed by the v1 heuristic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CellFeatures {
    /// Current Fire Weather Index.
    pub fwi: f32,
    /// Normalized historical ignition density.
    pub hist: f32,
    /// Wildland-urban interface indicator.
    pub wui: f32,
    /// Normalized road density.
    pub road: f32,
    /// Agricultural land indicator.
    pub agri: f32,
    /// Normalized residential population.
    pub population: f32,
    /// Normalized activity-point density.
    pub poi: f32,
    /// Normalized power-line density.
    pub power_line: f32,
    /// Whether burnable land cover is present.
    pub combustible: bool,
    /// Date represented by the FWI state.
    pub date: NaiveDate,
    /// Whether the date is in a school holiday.
    pub school_holiday: bool,
    /// Whether the date is a public holiday.
    pub public_holiday: bool,
}

/// Versionable logistic model estimating relative human ignition propensity.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct LearnedHumanModel {
    /// Logistic intercept fitted on the case-control sample.
    pub intercept: f32,
    /// Coefficients in [`HUMAN_FEATURE_NAMES`] order.
    pub weights: [f32; HUMAN_FEATURE_COUNT],
    /// Training-set means in [`HUMAN_FEATURE_NAMES`] order.
    pub means: [f32; HUMAN_FEATURE_COUNT],
    /// Training-set standard deviations in [`HUMAN_FEATURE_NAMES`] order.
    pub scales: [f32; HUMAN_FEATURE_COUNT],
}

impl LearnedHumanModel {
    /// Validates persisted model parameters.
    ///
    /// # Errors
    ///
    /// Returns an error when a parameter is non-finite or a scale is not positive.
    pub fn validate(&self) -> Result<(), RiskError> {
        if !self.intercept.is_finite()
            || self
                .weights
                .iter()
                .chain(self.means.iter())
                .any(|value| !value.is_finite())
            || self
                .scales
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return Err(RiskError::InvalidConfig(
                "learned human model parameters are invalid",
            ));
        }
        Ok(())
    }

    /// Estimates relative human ignition propensity for one cell-day.
    #[must_use]
    pub fn predict(&self, features: &CellFeatures) -> f32 {
        let raw = human_feature_vector(features);
        let linear = self
            .weights
            .iter()
            .zip(raw)
            .zip(self.means.iter().zip(self.scales.iter()))
            .fold(self.intercept, |value, ((weight, raw), (mean, scale))| {
                value + weight * ((raw - mean) / scale)
            });
        sigmoid(linear)
    }
}

/// Configurable constants for [`HeuristicV1`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeuristicConfig {
    /// FWI value mapped to physical risk 1.
    pub fwi_max: f32,
    /// Physical exponent in the multiplicative fusion.
    pub alpha: f32,
    /// Human exponent in the multiplicative fusion.
    pub beta: f32,
    /// Historical ignition weight.
    pub w_hist: f32,
    /// Wildland-urban interface weight.
    pub w_wui: f32,
    /// Road density weight.
    pub w_road: f32,
    /// Agricultural activity weight.
    pub w_agri: f32,
}

impl HeuristicConfig {
    /// Validates all scoring constants.
    ///
    /// # Errors
    ///
    /// Returns an error for non-finite or out-of-range constants.
    pub fn validate(self) -> Result<Self, RiskError> {
        if !self.fwi_max.is_finite() || self.fwi_max <= 0.0 {
            return Err(RiskError::InvalidConfig("fwi_max must be positive"));
        }
        if !self.alpha.is_finite()
            || !self.beta.is_finite()
            || self.alpha <= 0.0
            || self.beta <= 0.0
        {
            return Err(RiskError::InvalidConfig("alpha and beta must be positive"));
        }
        if [self.w_hist, self.w_wui, self.w_road, self.w_agri]
            .into_iter()
            .any(|weight| !weight.is_finite() || !(0.0..=1.0).contains(&weight))
        {
            return Err(RiskError::InvalidConfig(
                "human weights must be between zero and one",
            ));
        }
        Ok(self)
    }
}

/// Explicit first-version risk heuristic.
#[derive(Clone, Debug)]
pub struct HeuristicV1 {
    config: HeuristicConfig,
    learned_human: Option<LearnedHumanModel>,
}

impl HeuristicV1 {
    /// Creates a validated v1 model.
    ///
    /// # Errors
    ///
    /// Returns an error when a scoring constant is invalid.
    pub fn new(config: HeuristicConfig) -> Result<Self, RiskError> {
        Ok(Self {
            config: config.validate()?,
            learned_human: None,
        })
    }

    /// Replaces the human heuristic with a validated learned component.
    ///
    /// # Errors
    ///
    /// Returns an error when persisted model parameters are invalid.
    pub fn with_learned_human(
        mut self,
        learned_human: LearnedHumanModel,
    ) -> Result<Self, RiskError> {
        learned_human.validate()?;
        self.learned_human = Some(learned_human);
        Ok(self)
    }
}

impl IgnitionModel for HeuristicV1 {
    fn score(
        &self,
        cell: CellIndex,
        features: &CellFeatures,
        computed_at: DateTime<Utc>,
    ) -> RiskScore {
        let physical = clamp(features.fwi / self.config.fwi_max);
        let harvest = if (6..=8).contains(&features.date.month()) {
            1.0
        } else {
            0.0
        };
        let (human, mut factors) = if let Some(model) = &self.learned_human {
            let raw = human_feature_vector(features);
            let factors = HUMAN_FEATURE_NAMES
                .iter()
                .zip(raw)
                .zip(
                    model
                        .weights
                        .iter()
                        .zip(model.means.iter().zip(model.scales.iter())),
                )
                .filter_map(|((name, value), (weight, (mean, scale)))| {
                    let contribution = weight * ((value - mean) / scale);
                    (contribution > 0.0).then(|| Factor::new(name, value, contribution))
                })
                .collect();
            (model.predict(features), factors)
        } else {
            let calendar = calendar_multiplier(features);
            let factors = vec![
                Factor::new(
                    "historical_ignitions",
                    clamp(features.hist),
                    self.config.w_hist * clamp(features.hist) * calendar,
                ),
                Factor::new(
                    "wildland_urban_interface",
                    clamp(features.wui),
                    self.config.w_wui * clamp(features.wui) * calendar,
                ),
                Factor::new(
                    "road_density",
                    clamp(features.road),
                    self.config.w_road * clamp(features.road) * calendar,
                ),
                Factor::new(
                    "agricultural_activity",
                    clamp(features.agri) * harvest,
                    self.config.w_agri * clamp(features.agri) * harvest * calendar,
                ),
            ];
            let human = clamp(
                (self.config.w_hist * clamp(features.hist)
                    + self.config.w_wui * clamp(features.wui)
                    + self.config.w_road * clamp(features.road)
                    + self.config.w_agri * clamp(features.agri) * harvest)
                    * calendar,
            );
            (human, factors)
        };
        factors.push(Factor::new("fwi", physical, physical));
        factors.retain(|factor| factor.contribution > 0.0);
        factors.sort_by(|left, right| right.contribution.total_cmp(&left.contribution));
        factors.truncate(3);

        let score = if features.combustible {
            clamp(physical.powf(self.config.alpha) * human.powf(self.config.beta))
        } else {
            factors.clear();
            0.0
        };

        RiskScore {
            cell,
            computed_at,
            valid_at: computed_at,
            horizon: Horizon::Nowcast,
            score,
            physical,
            human,
            top_factors: factors,
        }
    }
}

/// Builds the stable input vector used by training and operational scoring.
#[must_use]
pub fn human_feature_vector(features: &CellFeatures) -> [f32; HUMAN_FEATURE_COUNT] {
    let weekend = matches!(features.date.weekday(), Weekday::Sat | Weekday::Sun);
    #[allow(clippy::cast_precision_loss)]
    let seasonal_angle = std::f32::consts::TAU * (features.date.ordinal0() as f32 / 365.25_f32);
    [
        clamp(features.wui),
        clamp(features.road),
        clamp(features.agri),
        clamp(features.population),
        clamp(features.poi),
        clamp(features.power_line),
        f32::from(weekend),
        f32::from(features.school_holiday),
        f32::from(features.public_holiday),
        seasonal_angle.sin(),
        seasonal_angle.cos(),
    ]
}

fn calendar_multiplier(features: &CellFeatures) -> f32 {
    let weekend = matches!(features.date.weekday(), Weekday::Sat | Weekday::Sun);
    let day_off = weekend || features.public_holiday;
    let summer = (6..=8).contains(&features.date.month());
    if day_off && features.school_holiday && summer {
        1.4
    } else if day_off {
        1.2
    } else if matches!(features.date.month(), 12 | 1 | 2) {
        0.6
    } else {
        1.0
    }
}

fn clamp(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn sigmoid(value: f32) -> f32 {
    if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp = value.exp();
        exp / (1.0 + exp)
    }
}

/// Supported prediction horizon.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Horizon {
    /// Current conditions only.
    Nowcast,
    /// Conditions forecast six hours ahead.
    #[serde(rename = "hours_6")]
    Hours6,
    /// Conditions forecast twenty-four hours ahead.
    #[serde(rename = "hours_24")]
    Hours24,
    /// Conditions forecast forty-eight hours ahead.
    #[serde(rename = "hours_48")]
    Hours48,
}

impl Horizon {
    /// Stable database representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nowcast => "nowcast",
            Self::Hours6 => "hours_6",
            Self::Hours24 => "hours_24",
            Self::Hours48 => "hours_48",
        }
    }

    /// Forecast offset in hours.
    #[must_use]
    pub const fn hours(self) -> i64 {
        match self {
            Self::Nowcast => 0,
            Self::Hours6 => 6,
            Self::Hours24 => 24,
            Self::Hours48 => 48,
        }
    }

    /// Stable list used by forecast pipelines and user interfaces.
    pub const ALL: [Self; 4] = [Self::Nowcast, Self::Hours6, Self::Hours24, Self::Hours48];
}

impl std::str::FromStr for Horizon {
    type Err = HorizonParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "nowcast" => Ok(Self::Nowcast),
            "hours_6" => Ok(Self::Hours6),
            "hours_24" => Ok(Self::Hours24),
            "hours_48" => Ok(Self::Hours48),
            _ => Err(HorizonParseError(value.to_owned())),
        }
    }
}

/// Unsupported prediction-horizon identifier.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unsupported risk horizon `{0}`")]
pub struct HorizonParseError(String);

/// One explainable model input ranked by contribution.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Factor {
    /// Stable factor identifier.
    pub name: String,
    /// Normalized input value used by the model.
    pub value: f32,
    /// Weighted contribution used for ranking.
    pub contribution: f32,
}

impl Factor {
    fn new(name: &str, value: f32, contribution: f32) -> Self {
        Self {
            name: name.to_owned(),
            value,
            contribution,
        }
    }
}

/// Complete explainable score for one H3 cell.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct RiskScore {
    /// H3 target cell.
    #[serde(skip)]
    pub cell: CellIndex,
    /// UTC calculation timestamp.
    pub computed_at: DateTime<Utc>,
    /// UTC time represented by this prediction.
    pub valid_at: DateTime<Utc>,
    /// Prediction horizon.
    pub horizon: Horizon,
    /// Fused ignition probability score.
    pub score: f32,
    /// Normalized physical component.
    pub physical: f32,
    /// Normalized human component.
    pub human: f32,
    /// Three strongest model factors.
    pub top_factors: Vec<Factor>,
}

/// Risk model configuration failures.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RiskError {
    /// One model constant is invalid.
    #[error("invalid risk configuration: {0}")]
    InvalidConfig(&'static str),
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};
    use grid::H3Grid;

    use super::{CellFeatures, HeuristicConfig, HeuristicV1, Horizon, IgnitionModel};

    fn model() -> HeuristicV1 {
        HeuristicV1::new(HeuristicConfig {
            fwi_max: 30.0,
            alpha: 0.6,
            beta: 0.4,
            w_hist: 0.4,
            w_wui: 0.25,
            w_road: 0.2,
            w_agri: 0.15,
        })
        .expect("valid model")
    }

    fn features() -> CellFeatures {
        CellFeatures {
            fwi: 24.0,
            hist: 0.9,
            wui: 0.7,
            road: 0.5,
            agri: 0.4,
            population: 0.3,
            poi: 0.2,
            power_line: 0.1,
            combustible: true,
            date: chrono::NaiveDate::from_ymd_opt(2025, 7, 19).expect("valid date"),
            school_holiday: true,
            public_holiday: false,
        }
    }

    #[test]
    fn computes_explainable_bounded_score() {
        let grid = H3Grid::new(9).expect("valid grid");
        let cell = grid.cell_for_point(43.2122, 2.3537).expect("valid cell");
        let computed_at = Utc.with_ymd_and_hms(2025, 7, 19, 12, 0, 0).unwrap();
        let score = model().score(cell, &features(), computed_at);

        assert!((0.0..=1.0).contains(&score.score));
        assert!((score.physical - 0.8).abs() < f32::EPSILON);
        assert_eq!(score.top_factors.len(), 3);
        assert!(
            score
                .top_factors
                .windows(2)
                .all(|pair| pair[0].contribution >= pair[1].contribution)
        );
    }

    #[test]
    fn combustible_mask_forces_zero() {
        let grid = H3Grid::new(9).expect("valid grid");
        let cell = grid.cell_for_point(43.2122, 2.3537).expect("valid cell");
        let mut inputs = features();
        inputs.combustible = false;
        let score = model().score(cell, &inputs, Utc::now());

        assert!(score.score.abs() < f32::EPSILON);
        assert!(score.top_factors.is_empty());
    }

    #[test]
    fn serializes_stable_forecast_horizons() {
        assert_eq!(
            serde_json::to_string(&Horizon::Hours24).expect("serializable horizon"),
            r#""hours_24""#
        );
        assert_eq!("hours_48".parse(), Ok(Horizon::Hours48));
    }

    #[test]
    fn learned_human_component_is_bounded() {
        let learned = super::LearnedHumanModel {
            intercept: -1.0,
            weights: [0.5; super::HUMAN_FEATURE_COUNT],
            means: [0.0; super::HUMAN_FEATURE_COUNT],
            scales: [1.0; super::HUMAN_FEATURE_COUNT],
        };
        let grid = H3Grid::new(9).expect("valid grid");
        let cell = grid.cell_for_point(43.2122, 2.3537).expect("valid cell");
        let score = model()
            .with_learned_human(learned)
            .expect("valid learned model")
            .score(cell, &features(), Utc::now());

        assert!((0.0..=1.0).contains(&score.human));
        assert!(
            score
                .top_factors
                .iter()
                .all(|factor| factor.name != "historical_ignitions")
        );
    }
}
