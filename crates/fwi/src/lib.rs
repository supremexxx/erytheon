//! Canadian Forest Fire Weather Index System calculations.
//!
//! This crate implements the six components of the 1987 Canadian Forest Fire
//! Weather Index (`FWI`) System using the equations published by Van Wagner
//! and Pickett. Calculations are pure and deterministic: the crate performs no
//! file, database, clock, or network access.
//!
//! Daily weather observations are expected at noon local standard time. Units
//! are degrees Celsius, percent relative humidity, kilometres per hour for
//! wind speed, and millimetres of accumulated 24-hour precipitation.
//!
//! # Example
//!
//! ```
//! use fwi::{FwiState, Weather, calculate_daily};
//!
//! # fn main() -> Result<(), fwi::FwiError> {
//! let weather = Weather {
//!     temperature_c: 17.0,
//!     relative_humidity_pct: 42.0,
//!     wind_speed_kmh: 25.0,
//!     precipitation_mm: 0.0,
//!     month: 4,
//! };
//! let output = calculate_daily(weather, FwiState::default())?;
//!
//! assert!((output.fwi - 10.04).abs() < 0.01);
//! # Ok(())
//! # }
//! ```
//!
//! # References
//!
//! - C. E. Van Wagner and T. L. Pickett, *Equations and FORTRAN program for
//!   the Canadian Forest Fire Weather Index System*, Forestry Technical
//!   Report 33, 1985.
//! - C. E. Van Wagner, *Development and Structure of the Canadian Forest Fire
//!   Weather Index System*, Forestry Technical Report 35, 1987.
//! - Natural Resources Canada, [`cffdrs`](https://cran.r-project.org/package=cffdrs)
//!   reference implementation.

mod codes;
mod indices;

pub use codes::{drought_code, duff_moisture_code, fine_fuel_moisture_code};
pub use indices::{buildup_index, fire_weather_index, initial_spread_index};

/// Standard spring start-up value for the Fine Fuel Moisture Code.
pub const INITIAL_FFMC: f64 = 85.0;
/// Standard spring start-up value for the Duff Moisture Code.
pub const INITIAL_DMC: f64 = 6.0;
/// Standard spring start-up value for the Drought Code.
pub const INITIAL_DC: f64 = 15.0;

/// Noon local-standard-time weather used by one daily calculation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weather {
    /// Screen-level air temperature in degrees Celsius.
    pub temperature_c: f64,
    /// Relative humidity in percent, from 0 through 100.
    pub relative_humidity_pct: f64,
    /// Ten-metre open wind speed in kilometres per hour.
    pub wind_speed_kmh: f64,
    /// Accumulated precipitation over the previous 24 hours in millimetres.
    pub precipitation_mm: f64,
    /// Calendar month numbered from 1 through 12.
    pub month: u8,
}

impl Weather {
    fn validate(self) -> Result<Self, FwiError> {
        validate_finite("temperature_c", self.temperature_c)?;
        validate_percentage("relative_humidity_pct", self.relative_humidity_pct)?;
        validate_non_negative("wind_speed_kmh", self.wind_speed_kmh)?;
        validate_non_negative("precipitation_mm", self.precipitation_mm)?;
        validate_month(self.month)?;
        Ok(self)
    }
}

/// Moisture-code state carried from one daily calculation to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FwiState {
    /// Fine Fuel Moisture Code from the previous day.
    pub ffmc: f64,
    /// Duff Moisture Code from the previous day.
    pub dmc: f64,
    /// Drought Code from the previous day.
    pub dc: f64,
}

impl Default for FwiState {
    fn default() -> Self {
        Self {
            ffmc: INITIAL_FFMC,
            dmc: INITIAL_DMC,
            dc: INITIAL_DC,
        }
    }
}

impl FwiState {
    fn validate(self) -> Result<Self, FwiError> {
        validate_range("ffmc", self.ffmc, 0.0, 101.0)?;
        validate_non_negative("dmc", self.dmc)?;
        validate_non_negative("dc", self.dc)?;
        Ok(self)
    }
}

/// Six outputs produced by one daily `FWI` calculation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FwiOutput {
    /// Fine Fuel Moisture Code.
    pub ffmc: f64,
    /// Duff Moisture Code.
    pub dmc: f64,
    /// Drought Code.
    pub dc: f64,
    /// Initial Spread Index.
    pub isi: f64,
    /// Buildup Index.
    pub bui: f64,
    /// Fire Weather Index.
    pub fwi: f64,
}

impl FwiOutput {
    /// Returns the three moisture codes to persist for the next day.
    #[must_use]
    pub const fn state(self) -> FwiState {
        FwiState {
            ffmc: self.ffmc,
            dmc: self.dmc,
            dc: self.dc,
        }
    }
}

/// Calculates all six daily `FWI` components.
///
/// Standard northern-hemisphere monthly day-length factors are used for DMC
/// and DC, as defined by the original Canadian system and appropriate for the
/// project's default area of interest.
///
/// # Errors
///
/// Returns [`FwiError`] when an input is non-finite or outside its documented
/// range.
pub fn calculate_daily(weather: Weather, previous: FwiState) -> Result<FwiOutput, FwiError> {
    let weather = weather.validate()?;
    let previous = previous.validate()?;
    let ffmc = fine_fuel_moisture_code(
        previous.ffmc,
        weather.temperature_c,
        weather.relative_humidity_pct,
        weather.wind_speed_kmh,
        weather.precipitation_mm,
    )?;
    let dmc = duff_moisture_code(
        previous.dmc,
        weather.temperature_c,
        weather.relative_humidity_pct,
        weather.precipitation_mm,
        weather.month,
    )?;
    let dc = drought_code(
        previous.dc,
        weather.temperature_c,
        weather.precipitation_mm,
        weather.month,
    )?;
    let isi = initial_spread_index(ffmc, weather.wind_speed_kmh)?;
    let bui = buildup_index(dmc, dc)?;
    let fwi = fire_weather_index(isi, bui)?;

    Ok(FwiOutput {
        ffmc,
        dmc,
        dc,
        isi,
        bui,
        fwi,
    })
}

/// Invalid input supplied to an `FWI` equation.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum FwiError {
    /// A floating-point input was NaN or infinite.
    #[error("{field} must be finite")]
    NonFinite {
        /// Name of the invalid input.
        field: &'static str,
    },
    /// An input that represents a quantity was negative.
    #[error("{field} must be non-negative, got {value}")]
    Negative {
        /// Name of the invalid input.
        field: &'static str,
        /// Invalid value.
        value: f64,
    },
    /// A bounded input fell outside its accepted interval.
    #[error("{field} must be between {minimum} and {maximum}, got {value}")]
    OutOfRange {
        /// Name of the invalid input.
        field: &'static str,
        /// Inclusive lower bound.
        minimum: f64,
        /// Inclusive upper bound.
        maximum: f64,
        /// Invalid value.
        value: f64,
    },
    /// A month outside 1 through 12 was supplied.
    #[error("month must be between 1 and 12, got {month}")]
    InvalidMonth {
        /// Invalid month number.
        month: u8,
    },
}

pub(crate) fn validate_finite(field: &'static str, value: f64) -> Result<(), FwiError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(FwiError::NonFinite { field })
    }
}

pub(crate) fn validate_non_negative(field: &'static str, value: f64) -> Result<(), FwiError> {
    validate_finite(field, value)?;
    if value >= 0.0 {
        Ok(())
    } else {
        Err(FwiError::Negative { field, value })
    }
}

pub(crate) fn validate_percentage(field: &'static str, value: f64) -> Result<(), FwiError> {
    validate_range(field, value, 0.0, 100.0)
}

pub(crate) fn validate_range(
    field: &'static str,
    value: f64,
    minimum: f64,
    maximum: f64,
) -> Result<(), FwiError> {
    validate_finite(field, value)?;
    if (minimum..=maximum).contains(&value) {
        Ok(())
    } else {
        Err(FwiError::OutOfRange {
            field,
            minimum,
            maximum,
            value,
        })
    }
}

pub(crate) const fn validate_month(month: u8) -> Result<(), FwiError> {
    if month >= 1 && month <= 12 {
        Ok(())
    } else {
        Err(FwiError::InvalidMonth { month })
    }
}
