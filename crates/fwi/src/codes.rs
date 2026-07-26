use crate::{FwiError, validate_month, validate_non_negative, validate_percentage, validate_range};

const FFMC_COEFFICIENT: f64 = 250.0 * 59.5 / 101.0;
const DMC_DAY_LENGTH_FACTORS: [f64; 12] = [
    6.5, 7.5, 9.0, 12.8, 13.9, 13.9, 12.4, 10.9, 9.4, 8.0, 7.0, 6.0,
];
const DC_DAY_LENGTH_FACTORS: [f64; 12] = [
    -1.6, -1.6, -1.6, 0.9, 3.8, 5.8, 6.4, 5.0, 2.4, 0.4, -1.6, -1.6,
];

/// Calculates the Fine Fuel Moisture Code (`FFMC`).
///
/// # Errors
///
/// Returns [`FwiError`] for a previous `FFMC` outside 0 through 101, relative
/// humidity outside 0 through 100, negative wind or rain, or non-finite input.
pub fn fine_fuel_moisture_code(
    previous_ffmc: f64,
    temperature_c: f64,
    relative_humidity_pct: f64,
    wind_speed_kmh: f64,
    precipitation_mm: f64,
) -> Result<f64, FwiError> {
    validate_range("previous_ffmc", previous_ffmc, 0.0, 101.0)?;
    crate::validate_finite("temperature_c", temperature_c)?;
    validate_percentage("relative_humidity_pct", relative_humidity_pct)?;
    validate_non_negative("wind_speed_kmh", wind_speed_kmh)?;
    validate_non_negative("precipitation_mm", precipitation_mm)?;

    let humidity = relative_humidity_pct.min(99.9999);
    let mut moisture = FFMC_COEFFICIENT * (101.0 - previous_ffmc) / (59.5 + previous_ffmc);

    if precipitation_mm > 0.5 {
        let net_rain = precipitation_mm - 0.5;
        let rain_absorption = 42.5
            * net_rain
            * (-100.0 / (251.0 - moisture)).exp()
            * (1.0 - (-6.93 / net_rain).exp());
        let high_moisture_correction = if moisture > 150.0 {
            0.0015 * (moisture - 150.0).powi(2) * net_rain.sqrt()
        } else {
            0.0
        };
        moisture += rain_absorption + high_moisture_correction;
        moisture = moisture.min(250.0);
    }

    let drying_equilibrium = 0.942 * humidity.powf(0.679)
        + 11.0 * ((humidity - 100.0) / 10.0).exp()
        + 0.18 * (21.1 - temperature_c) * (1.0 - (-0.115 * humidity).exp());
    let wetting_equilibrium = 0.618 * humidity.powf(0.753)
        + 10.0 * ((humidity - 100.0) / 10.0).exp()
        + 0.18 * (21.1 - temperature_c) * (1.0 - (-0.115 * humidity).exp());

    let final_moisture = if moisture < drying_equilibrium && moisture < wetting_equilibrium {
        let rate = 0.424 * (1.0 - ((100.0 - humidity) / 100.0).powf(1.7))
            + 0.0694 * wind_speed_kmh.sqrt() * (1.0 - ((100.0 - humidity) / 100.0).powi(8));
        let temperature_rate = rate * 0.581 * (0.0365 * temperature_c).exp();
        wetting_equilibrium - (wetting_equilibrium - moisture) / 10.0_f64.powf(temperature_rate)
    } else if moisture > drying_equilibrium {
        let rate = 0.424 * (1.0 - (humidity / 100.0).powf(1.7))
            + 0.0694 * wind_speed_kmh.sqrt() * (1.0 - (humidity / 100.0).powi(8));
        let temperature_rate = rate * 0.581 * (0.0365 * temperature_c).exp();
        drying_equilibrium + (moisture - drying_equilibrium) / 10.0_f64.powf(temperature_rate)
    } else {
        moisture
    };

    let ffmc = 59.5 * (250.0 - final_moisture) / (FFMC_COEFFICIENT + final_moisture);
    Ok(ffmc.clamp(0.0, 101.0))
}

/// Calculates the Duff Moisture Code (`DMC`).
///
/// # Errors
///
/// Returns [`FwiError`] for a negative previous code or rain, relative
/// humidity outside 0 through 100, an invalid month, or non-finite input.
pub fn duff_moisture_code(
    previous_dmc: f64,
    temperature_c: f64,
    relative_humidity_pct: f64,
    precipitation_mm: f64,
    month: u8,
) -> Result<f64, FwiError> {
    validate_non_negative("previous_dmc", previous_dmc)?;
    crate::validate_finite("temperature_c", temperature_c)?;
    validate_percentage("relative_humidity_pct", relative_humidity_pct)?;
    validate_non_negative("precipitation_mm", precipitation_mm)?;
    validate_month(month)?;

    let temperature = temperature_c.max(-1.1);
    let day_length = DMC_DAY_LENGTH_FACTORS[usize::from(month - 1)];
    let drying_rate =
        1.894 * (temperature + 1.1) * (100.0 - relative_humidity_pct) * day_length * 1.0e-4;

    let rain_adjusted = if precipitation_mm > 1.5 {
        let effective_rain = 0.92 * precipitation_mm - 1.27;
        let initial_moisture = 20.0 + 280.0 / (0.023 * previous_dmc).exp();
        let rain_coefficient = if previous_dmc <= 33.0 {
            100.0 / (0.5 + 0.3 * previous_dmc)
        } else if previous_dmc <= 65.0 {
            14.0 - 1.3 * previous_dmc.ln()
        } else {
            6.2 * previous_dmc.ln() - 17.2
        };
        let moisture_after_rain = initial_moisture
            + 1000.0 * effective_rain / (48.77 + rain_coefficient * effective_rain);
        (43.43 * (5.6348 - (moisture_after_rain - 20.0).ln())).max(0.0)
    } else {
        previous_dmc
    };

    Ok((rain_adjusted + drying_rate).max(0.0))
}

/// Calculates the Drought Code (`DC`).
///
/// # Errors
///
/// Returns [`FwiError`] for a negative previous code or rain, an invalid
/// month, or non-finite input.
pub fn drought_code(
    previous_dc: f64,
    temperature_c: f64,
    precipitation_mm: f64,
    month: u8,
) -> Result<f64, FwiError> {
    validate_non_negative("previous_dc", previous_dc)?;
    crate::validate_finite("temperature_c", temperature_c)?;
    validate_non_negative("precipitation_mm", precipitation_mm)?;
    validate_month(month)?;

    let temperature = temperature_c.max(-2.8);
    let day_length = DC_DAY_LENGTH_FACTORS[usize::from(month - 1)];
    let potential_evapotranspiration = (0.5 * (0.36 * (temperature + 2.8) + day_length)).max(0.0);

    let rain_adjusted = if precipitation_mm > 2.8 {
        let effective_rain = 0.83 * precipitation_mm - 1.27;
        let initial_moisture = 800.0 * (-previous_dc / 400.0).exp();
        (previous_dc - 400.0 * (1.0 + 3.937 * effective_rain / initial_moisture).ln()).max(0.0)
    } else {
        previous_dc
    };

    Ok((rain_adjusted + potential_evapotranspiration).max(0.0))
}
