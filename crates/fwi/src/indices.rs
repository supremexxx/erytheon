use crate::{FwiError, validate_non_negative, validate_range};

const FFMC_COEFFICIENT: f64 = 250.0 * 59.5 / 101.0;

/// Calculates the Initial Spread Index (`ISI`).
///
/// # Errors
///
/// Returns [`FwiError`] for an `FFMC` outside 0 through 101, negative wind, or
/// non-finite input.
pub fn initial_spread_index(ffmc: f64, wind_speed_kmh: f64) -> Result<f64, FwiError> {
    validate_range("ffmc", ffmc, 0.0, 101.0)?;
    validate_non_negative("wind_speed_kmh", wind_speed_kmh)?;

    let moisture = FFMC_COEFFICIENT * (101.0 - ffmc) / (59.5 + ffmc);
    let wind_effect = (0.05039 * wind_speed_kmh).exp();
    let fuel_effect =
        91.9 * (-0.1386 * moisture).exp() * (1.0 + moisture.powf(5.31) / 49_300_000.0);
    Ok(0.208 * wind_effect * fuel_effect)
}

/// Calculates the Buildup Index (`BUI`).
///
/// # Errors
///
/// Returns [`FwiError`] for a negative or non-finite moisture code.
pub fn buildup_index(dmc: f64, dc: f64) -> Result<f64, FwiError> {
    validate_non_negative("dmc", dmc)?;
    validate_non_negative("dc", dc)?;

    if dmc == 0.0 && dc == 0.0 {
        return Ok(0.0);
    }

    let mut bui = 0.8 * dc * dmc / (dmc + 0.4 * dc);
    if bui < dmc {
        let proportion = if dmc == 0.0 { 0.0 } else { (dmc - bui) / dmc };
        let coefficient = 0.92 + (0.0114 * dmc).powf(1.7);
        bui = (dmc - coefficient * proportion).max(0.0);
    }
    Ok(bui)
}

/// Calculates the Fire Weather Index (`FWI`).
///
/// # Errors
///
/// Returns [`FwiError`] for a negative or non-finite index.
pub fn fire_weather_index(isi: f64, bui: f64) -> Result<f64, FwiError> {
    validate_non_negative("isi", isi)?;
    validate_non_negative("bui", bui)?;

    let intermediate = if bui > 80.0 {
        0.1 * isi * (1000.0 / (25.0 + 108.64 / (0.023 * bui).exp()))
    } else {
        0.1 * isi * (0.626 * bui.powf(0.809) + 2.0)
    };
    if intermediate <= 1.0 {
        Ok(intermediate)
    } else {
        Ok((2.72 * (0.434 * intermediate.ln()).powf(0.647)).exp())
    }
}
