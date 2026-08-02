use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, Utc};
use grid::LatLng;
use reqwest::{StatusCode, header::RETRY_AFTER};
use serde::Deserialize;

const HOURLY_VARIABLES: &str = "temperature_2m,relative_humidity_2m,precipitation,wind_speed_10m";
const PAST_HOURS: &str = "24";
const FORECAST_HOURS: &str = "49";
const RATE_LIMIT_RETRIES: usize = 1;
const RATE_LIMIT_DELAY: Duration = Duration::from_secs(5);

/// Operational weather models available through the normalized forecast API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForecastModel {
    /// Météo-France AROME/ARPEGE seamless forecast.
    MeteoFrance,
    /// ECMWF IFS 0.25° forecast used as an independent fallback model.
    Ecmwf,
}

impl ForecastModel {
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::MeteoFrance => "open_meteo_arome",
            Self::Ecmwf => "open_meteo_ecmwf_ifs025",
        }
    }

    const fn api_url(self) -> &'static str {
        match self {
            Self::MeteoFrance => "https://api.open-meteo.com/v1/meteofrance",
            Self::Ecmwf => "https://api.open-meteo.com/v1/forecast",
        }
    }

    const fn model(self) -> &'static str {
        match self {
            Self::MeteoFrance => "meteofrance_seamless",
            Self::Ecmwf => "ecmwf_ifs025",
        }
    }
}

/// One AROME/ARPEGE weather value valid at an exact UTC hour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ForecastSample {
    pub valid_at: DateTime<Utc>,
    pub temperature_c: f64,
    pub relative_humidity_pct: f64,
    pub wind_speed_kmh: f64,
    pub precipitation_24h_mm: f64,
}

/// Forecast time series for one requested interpolation anchor.
#[derive(Clone, Debug, PartialEq)]
pub struct ForecastLocation {
    pub location: LatLng,
    pub samples: Vec<ForecastSample>,
}

impl ForecastLocation {
    /// Returns the sample for one exact valid hour.
    #[must_use]
    pub fn sample_at(&self, valid_at: DateTime<Utc>) -> Option<ForecastSample> {
        self.samples
            .iter()
            .copied()
            .find(|sample| sample.valid_at == valid_at)
    }
}

/// Client for a weather model exposed through Open-Meteo's normalized API.
#[derive(Clone, Copy, Debug)]
pub struct OpenMeteoForecastSource {
    model: ForecastModel,
}

impl OpenMeteoForecastSource {
    #[must_use]
    pub const fn new(model: ForecastModel) -> Self {
        Self { model }
    }

    #[must_use]
    pub const fn id(self) -> &'static str {
        self.model.id()
    }

    /// Fetches 24 past hours and 49 forecast hours for all anchors in one request.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint, JSON shape, units, or values are invalid.
    pub async fn fetch(
        &self,
        client: &reqwest::Client,
        locations: &[LatLng],
    ) -> Result<Vec<ForecastLocation>, OpenMeteoError> {
        if locations.is_empty() {
            return Err(OpenMeteoError::EmptyLocations);
        }
        let latitudes = locations
            .iter()
            .map(|location| location.lat().to_string())
            .collect::<Vec<_>>()
            .join(",");
        let longitudes = locations
            .iter()
            .map(|location| location.lng().to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut retry_count = 0;
        let response = loop {
            let response = client
                .get(self.model.api_url())
                .query(&[
                    ("latitude", latitudes.as_str()),
                    ("longitude", longitudes.as_str()),
                    ("hourly", HOURLY_VARIABLES),
                    ("models", self.model.model()),
                    ("past_hours", PAST_HOURS),
                    ("forecast_hours", FORECAST_HOURS),
                    ("timezone", "UTC"),
                ])
                .send()
                .await?;
            if response.status() != StatusCode::TOO_MANY_REQUESTS {
                break response.error_for_status()?;
            }
            if retry_count == RATE_LIMIT_RETRIES {
                return Err(response
                    .error_for_status()
                    .expect_err("HTTP 429 must produce a reqwest error")
                    .into());
            }
            retry_count += 1;
            let delay = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map_or(RATE_LIMIT_DELAY, Duration::from_secs)
                .max(RATE_LIMIT_DELAY);
            tracing::warn!(
                retry_count,
                delay_seconds = delay.as_secs(),
                locations = locations.len(),
                source = self.id(),
                "weather source rate limit reached; retrying"
            );
            tokio::time::sleep(delay).await;
        }
        .text()
        .await?;
        let parsed = parse_response(&response)?;
        if parsed.len() != locations.len() {
            return Err(OpenMeteoError::LocationCount {
                expected: locations.len(),
                actual: parsed.len(),
            });
        }
        Ok(parsed)
    }
}

fn parse_response(response: &str) -> Result<Vec<ForecastLocation>, OpenMeteoError> {
    let envelope = serde_json::from_str::<ApiEnvelope>(response)?;
    let locations = match envelope {
        ApiEnvelope::One(location) => vec![*location],
        ApiEnvelope::Many(locations) => locations,
    };
    locations.into_iter().map(convert_location).collect()
}

fn convert_location(location: ApiLocation) -> Result<ForecastLocation, OpenMeteoError> {
    let units = location.hourly_units;
    if units.temperature_2m != "°C"
        || units.relative_humidity_2m != "%"
        || units.precipitation != "mm"
        || units.wind_speed_10m != "km/h"
    {
        return Err(OpenMeteoError::UnexpectedUnits);
    }
    let hourly = location.hourly;
    let length = hourly.time.len();
    for (field, actual) in [
        ("temperature_2m", hourly.temperature_2m.len()),
        ("relative_humidity_2m", hourly.relative_humidity_2m.len()),
        ("precipitation", hourly.precipitation.len()),
        ("wind_speed_10m", hourly.wind_speed_10m.len()),
    ] {
        if actual != length {
            return Err(OpenMeteoError::LengthMismatch {
                field,
                expected: length,
                actual,
            });
        }
    }
    let mut precipitation_window = Vec::with_capacity(length);
    for index in 0..length {
        let start = index.saturating_sub(23);
        let total = hourly.precipitation[start..=index]
            .iter()
            .map(|value| value.unwrap_or_default().max(0.0))
            .sum();
        precipitation_window.push(total);
    }
    let samples = hourly
        .time
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            let timestamp = NaiveDateTime::parse_from_str(&value, "%Y-%m-%dT%H:%M")?;
            Ok(ForecastSample {
                valid_at: timestamp.and_utc(),
                temperature_c: required(hourly.temperature_2m[index], "temperature_2m", index)?,
                relative_humidity_pct: required(
                    hourly.relative_humidity_2m[index],
                    "relative_humidity_2m",
                    index,
                )?,
                wind_speed_kmh: required(hourly.wind_speed_10m[index], "wind_speed_10m", index)?,
                precipitation_24h_mm: precipitation_window[index],
            })
        })
        .collect::<Result<Vec<_>, OpenMeteoError>>()?;
    Ok(ForecastLocation {
        location: LatLng::new(location.latitude, location.longitude)
            .map_err(|error| OpenMeteoError::Coordinate(error.to_string()))?,
        samples,
    })
}

fn required(value: Option<f64>, field: &'static str, index: usize) -> Result<f64, OpenMeteoError> {
    value.ok_or(OpenMeteoError::MissingValue { field, index })
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ApiEnvelope {
    One(Box<ApiLocation>),
    Many(Vec<ApiLocation>),
}

#[derive(Debug, Deserialize)]
struct ApiLocation {
    latitude: f64,
    longitude: f64,
    hourly_units: ApiUnits,
    hourly: ApiHourly,
}

#[derive(Debug, Deserialize)]
struct ApiUnits {
    temperature_2m: String,
    relative_humidity_2m: String,
    precipitation: String,
    wind_speed_10m: String,
}

#[derive(Debug, Deserialize)]
struct ApiHourly {
    time: Vec<String>,
    temperature_2m: Vec<Option<f64>>,
    relative_humidity_2m: Vec<Option<f64>>,
    precipitation: Vec<Option<f64>>,
    wind_speed_10m: Vec<Option<f64>>,
}

/// Open-Meteo forecast acquisition failures.
#[derive(Debug, thiserror::Error)]
pub enum OpenMeteoError {
    #[error("at least one forecast location is required")]
    EmptyLocations,
    #[error("Open-Meteo request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid Open-Meteo JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid Open-Meteo timestamp: {0}")]
    Time(#[from] chrono::ParseError),
    #[error("invalid Open-Meteo coordinate: {0}")]
    Coordinate(String),
    #[error("Open-Meteo returned {actual} locations; expected {expected}")]
    LocationCount { expected: usize, actual: usize },
    #[error("Open-Meteo returned unexpected weather units")]
    UnexpectedUnits,
    #[error("Open-Meteo field {field} has {actual} values; expected {expected}")]
    LengthMismatch {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
    #[error("Open-Meteo field {field} is missing at index {index}")]
    MissingValue { field: &'static str, index: usize },
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone as _, Utc};

    use super::{ForecastModel, OpenMeteoForecastSource, parse_response};

    #[test]
    fn weather_models_have_distinct_provenance_ids() {
        let meteo_france = OpenMeteoForecastSource::new(ForecastModel::MeteoFrance);
        let ecmwf = OpenMeteoForecastSource::new(ForecastModel::Ecmwf);

        assert_eq!(meteo_france.id(), "open_meteo_arome");
        assert_eq!(ecmwf.id(), "open_meteo_ecmwf_ifs025");
        assert_ne!(meteo_france.id(), ecmwf.id());
    }

    #[test]
    fn parses_hourly_forecast_and_accumulates_precipitation() {
        let payload = r#"{
          "latitude":43.2,"longitude":2.3,
          "hourly_units":{"temperature_2m":"°C","relative_humidity_2m":"%","precipitation":"mm","wind_speed_10m":"km/h"},
          "hourly":{"time":["2026-07-18T12:00","2026-07-18T13:00"],"temperature_2m":[30.0,31.0],"relative_humidity_2m":[25.0,24.0],"precipitation":[0.2,0.3],"wind_speed_10m":[18.0,20.0]}
        }"#;
        let locations = parse_response(payload).expect("valid forecast");
        assert_eq!(locations.len(), 1);
        assert!((locations[0].samples[1].precipitation_24h_mm - 0.5).abs() < f64::EPSILON);
        assert_eq!(
            locations[0].samples[1].valid_at,
            Utc.with_ymd_and_hms(2026, 7, 18, 13, 0, 0).unwrap()
        );
    }
}
