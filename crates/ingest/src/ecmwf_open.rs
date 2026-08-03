//! Direct ECMWF IFS open-data operational forecast connector.

use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    process::Command,
};

use chrono::{DateTime, Days, Duration, Timelike as _, Utc};
use grid::{BoundingBox, LatLng};
use reqwest::{StatusCode, header::RANGE};
use serde::Deserialize;

use crate::open_meteo::{ForecastLocation, ForecastSample};

const BASE_URL: &str = "https://data.ecmwf.int/forecasts";
const GRID_SCALE: f64 = 4.0;
const RUN_HOURS: [u32; 4] = [0, 6, 12, 18];
const MAX_STEP: i64 = 144;

/// Direct, unauthenticated ECMWF IFS 0.25-degree open-data source.
#[derive(Clone, Debug)]
pub struct EcmwfOpenDataForecastSource {
    cache_dir: PathBuf,
    aoi: BoundingBox,
}

impl EcmwfOpenDataForecastSource {
    pub const ID: &'static str = "ecmwf_ifs025_direct";

    #[must_use]
    pub fn new(cache_dir: impl Into<PathBuf>, aoi: BoundingBox) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            aoi,
        }
    }

    /// Downloads byte ranges for only the required IFS fields and samples them at `locations`.
    ///
    /// # Errors
    ///
    /// Returns an error if no retained run covers the complete period, a byte-range request fails,
    /// GRIB decoding fails, or a requested location is outside the decoded grid.
    pub async fn fetch(
        &self,
        client: &reqwest::Client,
        locations: &[LatLng],
    ) -> Result<Vec<ForecastLocation>, EcmwfOpenError> {
        if locations.is_empty() {
            return Err(EcmwfOpenError::EmptyLocations);
        }
        let now = Utc::now();
        let base_valid_at = now
            .with_minute(0)
            .and_then(|value| value.with_second(0))
            .and_then(|value| value.with_nanosecond(0))
            .map(|value| value - Duration::hours(i64::from(value.hour() % 3)))
            .ok_or(EcmwfOpenError::InvalidTime)?;
        let sample_times = required_sample_times(base_valid_at)?;
        let first = *sample_times.first().ok_or(EcmwfOpenError::InvalidTime)?;
        let last = *sample_times.last().ok_or(EcmwfOpenError::InvalidTime)?;
        let run_at = self.select_run(client, first, last).await?;
        let mut forecasts = locations
            .iter()
            .copied()
            .map(|location| ForecastLocation {
                location,
                samples: Vec::with_capacity(sample_times.len()),
            })
            .collect::<Vec<_>>();

        for valid_at in sample_times {
            let step = valid_at.signed_duration_since(run_at).num_hours();
            let previous_step = step - 24;
            if previous_step < 0 || step > MAX_STEP || step % 3 != 0 {
                return Err(EcmwfOpenError::UnsupportedStep(step));
            }
            let temperature = self
                .field(client, run_at, step, Variable::Temperature)
                .await?;
            let dewpoint = self.field(client, run_at, step, Variable::Dewpoint).await?;
            let wind_u = self.field(client, run_at, step, Variable::WindU).await?;
            let wind_v = self.field(client, run_at, step, Variable::WindV).await?;
            let precipitation_end = self
                .field(client, run_at, step, Variable::Precipitation)
                .await?;
            let precipitation_start = self
                .field(client, run_at, previous_step, Variable::Precipitation)
                .await?;
            for forecast in &mut forecasts {
                let temperature_c = temperature.sample(forecast.location)? - 273.15;
                let dewpoint_c = dewpoint.sample(forecast.location)? - 273.15;
                let u_ms = wind_u.sample(forecast.location)?;
                let v_ms = wind_v.sample(forecast.location)?;
                let precipitation_m = precipitation_end.sample(forecast.location)?
                    - precipitation_start.sample(forecast.location)?;
                forecasts_sample_push(
                    forecast,
                    ForecastSample {
                        valid_at,
                        temperature_c,
                        relative_humidity_pct: relative_humidity(temperature_c, dewpoint_c),
                        wind_speed_kmh: u_ms.hypot(v_ms) * 3.6,
                        precipitation_24h_mm: (precipitation_m * 1000.0).max(0.0),
                    },
                );
            }
        }
        Ok(forecasts)
    }

    async fn select_run(
        &self,
        client: &reqwest::Client,
        first_valid_at: DateTime<Utc>,
        last_valid_at: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, EcmwfOpenError> {
        let latest_allowed = first_valid_at - Duration::hours(24);
        let mut candidates = Vec::new();
        for days_back in 0..=4 {
            let date = latest_allowed
                .date_naive()
                .checked_sub_days(Days::new(days_back))
                .ok_or(EcmwfOpenError::InvalidTime)?;
            for hour in RUN_HOURS {
                let candidate = date
                    .and_hms_opt(hour, 0, 0)
                    .ok_or(EcmwfOpenError::InvalidTime)?
                    .and_utc();
                let final_step = last_valid_at.signed_duration_since(candidate).num_hours();
                if candidate <= latest_allowed && final_step <= MAX_STEP && final_step % 3 == 0 {
                    candidates.push(candidate);
                }
            }
        }
        candidates.sort_unstable_by(|left, right| right.cmp(left));
        for candidate in candidates {
            let step = last_valid_at.signed_duration_since(candidate).num_hours();
            let url = data_url(candidate, step);
            let response = client.head(&url).send().await?;
            if response.status().is_success() {
                tracing::info!(run_at = %candidate, last_step = step, "selected direct ECMWF run");
                return Ok(candidate);
            }
            if response.status() != StatusCode::NOT_FOUND {
                return Err(EcmwfOpenError::HttpStatus {
                    status: response.status(),
                    url,
                });
            }
        }
        Err(EcmwfOpenError::NoAvailableRun)
    }

    async fn field(
        &self,
        client: &reqwest::Client,
        run_at: DateTime<Utc>,
        step: i64,
        variable: Variable,
    ) -> Result<GridField, EcmwfOpenError> {
        let run_dir = self
            .cache_dir
            .join(run_at.format("%Y%m%d%H").to_string())
            .join(format!(
                "{:.3}_{:.3}_{:.3}_{:.3}",
                self.aoi.west, self.aoi.south, self.aoi.east, self.aoi.north
            ));
        tokio::fs::create_dir_all(&run_dir)
            .await
            .map_err(|source| EcmwfOpenError::Io {
                path: run_dir.clone(),
                source,
            })?;
        let field_name = format!("{}-{step:03}", variable.parameter());
        let xyz_path = run_dir.join(format!("{field_name}.xyz"));
        if !xyz_path.is_file() {
            self.materialize_field(
                client,
                run_at,
                step,
                variable,
                &run_dir,
                &field_name,
                &xyz_path,
            )
            .await?;
        }
        GridField::read(&xyz_path).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn materialize_field(
        &self,
        client: &reqwest::Client,
        run_at: DateTime<Utc>,
        step: i64,
        variable: Variable,
        run_dir: &Path,
        field_name: &str,
        xyz_path: &Path,
    ) -> Result<(), EcmwfOpenError> {
        let data_url = data_url(run_at, step);
        let index_url = index_url(run_at, step);
        let index = client
            .get(&index_url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        let entry = index
            .lines()
            .map(serde_json::from_str::<IndexEntry>)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .find(|entry| entry.parameter == variable.parameter())
            .ok_or_else(|| EcmwfOpenError::MissingParameter {
                parameter: variable.parameter(),
                index_url: index_url.clone(),
            })?;
        let end = entry
            .offset
            .checked_add(entry.length)
            .and_then(|value| value.checked_sub(1))
            .ok_or(EcmwfOpenError::InvalidRange)?;
        let grib = client
            .get(&data_url)
            .header(RANGE, format!("bytes={}-{}", entry.offset, end))
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        if grib.len() != entry.length {
            return Err(EcmwfOpenError::RangeLength {
                expected: entry.length,
                actual: grib.len(),
            });
        }
        let grib_path = run_dir.join(format!("{field_name}.grib2"));
        let netcdf_path = run_dir.join(format!("{field_name}.nc"));
        let pending_xyz = run_dir.join(format!("{field_name}.xyz.part"));
        tokio::fs::write(&grib_path, grib)
            .await
            .map_err(|source| EcmwfOpenError::Io {
                path: grib_path.clone(),
                source,
            })?;
        let aoi = self.aoi;
        let grib_for_task = grib_path.clone();
        let netcdf_for_task = netcdf_path.clone();
        let pending_for_task = pending_xyz.clone();
        tokio::task::spawn_blocking(move || {
            run_command(
                "grib_to_netcdf",
                &[
                    "-o",
                    path_text(&netcdf_for_task)?,
                    path_text(&grib_for_task)?,
                ],
            )?;
            run_command(
                "gdal_translate",
                &[
                    "-q",
                    "-unscale",
                    "-of",
                    "XYZ",
                    "-projwin",
                    &aoi.west.to_string(),
                    &aoi.north.to_string(),
                    &aoi.east.to_string(),
                    &aoi.south.to_string(),
                    path_text(&netcdf_for_task)?,
                    path_text(&pending_for_task)?,
                ],
            )?;
            Ok::<(), EcmwfOpenError>(())
        })
        .await
        .map_err(|error| EcmwfOpenError::Task(error.to_string()))??;
        tokio::fs::rename(&pending_xyz, xyz_path)
            .await
            .map_err(|source| EcmwfOpenError::Io {
                path: xyz_path.to_path_buf(),
                source,
            })?;
        for path in [&grib_path, &netcdf_path] {
            if let Err(error) = tokio::fs::remove_file(path).await
                && error.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(path = %path.display(), %error, "failed to remove intermediate ECMWF file");
            }
        }
        Ok(())
    }
}

fn forecasts_sample_push(forecast: &mut ForecastLocation, sample: ForecastSample) {
    forecast.samples.push(sample);
}

fn required_sample_times(base: DateTime<Utc>) -> Result<Vec<DateTime<Utc>>, EcmwfOpenError> {
    let mut times = BTreeSet::from([
        base,
        base + Duration::hours(6),
        base + Duration::hours(24),
        base + Duration::hours(48),
    ]);
    let first_noon = latest_noon(base)?;
    let last_noon = latest_noon(base + Duration::hours(48))?;
    let mut noon = first_noon;
    loop {
        times.insert(noon);
        if noon == last_noon {
            break;
        }
        noon = noon
            .checked_add_days(Days::new(1))
            .ok_or(EcmwfOpenError::InvalidTime)?;
    }
    Ok(times.into_iter().collect())
}

fn latest_noon(valid_at: DateTime<Utc>) -> Result<DateTime<Utc>, EcmwfOpenError> {
    let date = valid_at.date_naive();
    let noon = date
        .and_hms_opt(12, 0, 0)
        .ok_or(EcmwfOpenError::InvalidTime)?
        .and_utc();
    if noon <= valid_at {
        Ok(noon)
    } else {
        Ok(date
            .checked_sub_days(Days::new(1))
            .ok_or(EcmwfOpenError::InvalidTime)?
            .and_hms_opt(12, 0, 0)
            .ok_or(EcmwfOpenError::InvalidTime)?
            .and_utc())
    }
}

fn relative_humidity(temperature_c: f64, dewpoint_c: f64) -> f64 {
    let saturation = (17.625 * temperature_c / (243.04 + temperature_c)).exp();
    let actual = (17.625 * dewpoint_c / (243.04 + dewpoint_c)).exp();
    (100.0 * actual / saturation).clamp(0.0, 100.0)
}

fn data_url(run_at: DateTime<Utc>, step: i64) -> String {
    let date = run_at.format("%Y%m%d");
    let cycle = run_at.format("%H");
    let timestamp = run_at.format("%Y%m%d%H0000");
    format!("{BASE_URL}/{date}/{cycle}z/ifs/0p25/oper/{timestamp}-{step}h-oper-fc.grib2")
}

fn index_url(run_at: DateTime<Utc>, step: i64) -> String {
    data_url(run_at, step).replace(".grib2", ".index")
}

#[derive(Clone, Copy, Debug)]
enum Variable {
    Temperature,
    Dewpoint,
    WindU,
    WindV,
    Precipitation,
}

impl Variable {
    const fn parameter(self) -> &'static str {
        match self {
            Self::Temperature => "2t",
            Self::Dewpoint => "2d",
            Self::WindU => "10u",
            Self::WindV => "10v",
            Self::Precipitation => "tp",
        }
    }
}

#[derive(Debug, Deserialize)]
struct IndexEntry {
    #[serde(rename = "param")]
    parameter: String,
    #[serde(rename = "_offset")]
    offset: usize,
    #[serde(rename = "_length")]
    length: usize,
}

#[derive(Clone, Debug)]
struct GridField {
    path: PathBuf,
    values: HashMap<(i32, i32), f64>,
}

impl GridField {
    async fn read(path: &Path) -> Result<Self, EcmwfOpenError> {
        let document =
            tokio::fs::read_to_string(path)
                .await
                .map_err(|source| EcmwfOpenError::Io {
                    path: path.to_path_buf(),
                    source,
                })?;
        let mut values = HashMap::new();
        for line in document.lines() {
            let mut fields = line.split_whitespace();
            let longitude = parse_number(fields.next(), line)?;
            let latitude = parse_number(fields.next(), line)?;
            let value = parse_number(fields.next(), line)?;
            values.insert(grid_key(latitude, longitude), value);
        }
        if values.is_empty() {
            return Err(EcmwfOpenError::EmptyGrid(path.to_path_buf()));
        }
        Ok(Self {
            path: path.to_path_buf(),
            values,
        })
    }

    fn sample(&self, location: LatLng) -> Result<f64, EcmwfOpenError> {
        let center = grid_key(location.lat(), location.lng());
        let mut nearest = None;
        for latitude_offset in -1..=1 {
            for longitude_offset in -1..=1 {
                let key = (center.0 + latitude_offset, center.1 + longitude_offset);
                if let Some(value) = self.values.get(&key) {
                    let latitude = f64::from(key.0) / GRID_SCALE;
                    let longitude = f64::from(key.1) / GRID_SCALE;
                    let distance = (latitude - location.lat()).mul_add(
                        latitude - location.lat(),
                        (longitude - location.lng()).powi(2),
                    );
                    if nearest.is_none_or(|(best_distance, _)| distance < best_distance) {
                        nearest = Some((distance, *value));
                    }
                }
            }
        }
        nearest
            .map(|(_, value)| value)
            .ok_or(EcmwfOpenError::MissingGridPoint {
                latitude: location.lat(),
                longitude: location.lng(),
                path: self.path.clone(),
            })
    }
}

#[allow(clippy::cast_possible_truncation)]
fn grid_key(latitude: f64, longitude: f64) -> (i32, i32) {
    (
        (latitude * GRID_SCALE).round() as i32,
        (longitude * GRID_SCALE).round() as i32,
    )
}

fn parse_number(value: Option<&str>, line: &str) -> Result<f64, EcmwfOpenError> {
    value
        .ok_or_else(|| EcmwfOpenError::InvalidGridLine(line.to_owned()))?
        .parse()
        .map_err(|_| EcmwfOpenError::InvalidGridLine(line.to_owned()))
}

fn path_text(path: &Path) -> Result<&str, EcmwfOpenError> {
    path.to_str()
        .ok_or_else(|| EcmwfOpenError::InvalidPath(path.to_path_buf()))
}

fn run_command(program: &'static str, arguments: &[&str]) -> Result<(), EcmwfOpenError> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|source| EcmwfOpenError::CommandStart { program, source })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(EcmwfOpenError::CommandFailed {
            program,
            message: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

/// Failures from direct ECMWF open-data acquisition and decoding.
#[derive(Debug, thiserror::Error)]
pub enum EcmwfOpenError {
    #[error("at least one ECMWF forecast location is required")]
    EmptyLocations,
    #[error("no retained ECMWF run covers the required period")]
    NoAvailableRun,
    #[error("invalid ECMWF forecast time")]
    InvalidTime,
    #[error("ECMWF forecast step {0} is unavailable")]
    UnsupportedStep(i64),
    #[error("ECMWF request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid ECMWF index JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("ECMWF returned HTTP {status} for {url}")]
    HttpStatus { status: StatusCode, url: String },
    #[error("ECMWF parameter {parameter} is absent from {index_url}")]
    MissingParameter {
        parameter: &'static str,
        index_url: String,
    },
    #[error("invalid ECMWF byte range")]
    InvalidRange,
    #[error("ECMWF byte range returned {actual} bytes; expected {expected}")]
    RangeLength { expected: usize, actual: usize },
    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to start {program}: {source}")]
    CommandStart {
        program: &'static str,
        source: std::io::Error,
    },
    #[error("{program} failed: {message}")]
    CommandFailed {
        program: &'static str,
        message: String,
    },
    #[error("invalid weather cache path {0}")]
    InvalidPath(PathBuf),
    #[error("invalid ECMWF grid line: {0}")]
    InvalidGridLine(String),
    #[error("decoded ECMWF grid is empty: {0}")]
    EmptyGrid(PathBuf),
    #[error("ECMWF grid {path} does not cover {latitude},{longitude}")]
    MissingGridPoint {
        latitude: f64,
        longitude: f64,
        path: PathBuf,
    },
    #[error("ECMWF decode task failed: {0}")]
    Task(String),
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{TimeZone as _, Timelike as _, Utc};
    use grid::{BoundingBox, LatLng};

    use super::{EcmwfOpenDataForecastSource, data_url, relative_humidity, required_sample_times};

    #[test]
    fn urls_match_the_open_data_convention() {
        let run = Utc.with_ymd_and_hms(2026, 8, 3, 12, 0, 0).unwrap();
        assert_eq!(
            data_url(run, 9),
            "https://data.ecmwf.int/forecasts/20260803/12z/ifs/0p25/oper/20260803120000-9h-oper-fc.grib2"
        );
    }

    #[test]
    fn humidity_is_bounded_and_saturated_at_equal_temperature() {
        assert!((relative_humidity(20.0, 20.0) - 100.0).abs() < f64::EPSILON);
        assert!((0.0..=100.0).contains(&relative_humidity(30.0, 10.0)));
    }

    #[test]
    fn required_times_are_three_hour_aligned() {
        let base = Utc.with_ymd_and_hms(2026, 8, 3, 21, 0, 0).unwrap();
        assert!(
            required_sample_times(base)
                .expect("valid times")
                .iter()
                .all(|time| time.hour() % 3 == 0)
        );
    }

    #[tokio::test]
    #[ignore = "requires live ECMWF data and external GRIB tools"]
    async fn fetches_and_decodes_a_live_forecast() {
        let configured_cache = std::env::var_os("ECMWF_TEST_CACHE_DIR").map(PathBuf::from);
        let cache_dir = configured_cache.clone().unwrap_or_else(|| {
            std::env::temp_dir().join(format!("erytheon-ecmwf-test-{}", std::process::id()))
        });
        let aoi = BoundingBox::new(-5.5, 41.0, 10.0, 51.5).expect("valid France test AOI");
        let source = EcmwfOpenDataForecastSource::new(&cache_dir, aoi);
        let locations = [
            LatLng::new(48.8566, 2.3522).expect("valid Paris coordinate"),
            LatLng::new(48.3904, -4.4861).expect("valid Brest coordinate"),
            LatLng::new(48.5734, 7.7521).expect("valid Strasbourg coordinate"),
            LatLng::new(42.6887, 2.8948).expect("valid Perpignan coordinate"),
            LatLng::new(41.9192, 8.7386).expect("valid Ajaccio coordinate"),
        ];
        let result = source
            .fetch(&reqwest::Client::new(), &locations)
            .await
            .expect("live ECMWF forecast should decode");
        assert_eq!(result.len(), locations.len());
        assert!(result.iter().all(|forecast| {
            forecast.samples.len() >= 4
                && forecast.samples.iter().all(|sample| {
                    (-80.0..=60.0).contains(&sample.temperature_c)
                        && (0.0..=100.0).contains(&sample.relative_humidity_pct)
                        && (0.0..=300.0).contains(&sample.wind_speed_kmh)
                        && (0.0..=1_000.0).contains(&sample.precipitation_24h_mm)
                })
        }));
        if configured_cache.is_none() {
            let _cleanup = std::fs::remove_dir_all(cache_dir);
        }
    }
}
