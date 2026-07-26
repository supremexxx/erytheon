//! Historical Météo-France SYNOP archive loader.

use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use chrono::{Datelike as _, Months, NaiveDate, NaiveDateTime, NaiveTime};
use flate2::read::GzDecoder;
use serde::{Deserialize, Deserializer};

use crate::{SourceError, meteo_france::WeatherStationObservation};

const SYNOP_NOON_UTC: NaiveTime = NaiveTime::from_hms_opt(12, 0, 0).expect("valid time");
const KELVIN_OFFSET: f64 = 273.15;
const METRES_PER_SECOND_TO_KILOMETRES_PER_HOUR: f64 = 3.6;

/// Fixed metadata for one station retained by the historical backtest.
#[derive(Clone, Copy, Debug)]
pub struct ArchiveStation {
    /// WMO station identifier.
    pub id: &'static str,
    /// Human-readable station name.
    pub name: &'static str,
    /// WGS84 latitude.
    pub latitude: f64,
    /// WGS84 longitude.
    pub longitude: f64,
}

/// Stable station catalog surrounding the default Aude AOI.
pub const ARCHIVE_STATIONS: [ArchiveStation; 4] = [
    ArchiveStation {
        id: "07558",
        name: "MILLAU",
        latitude: 44.118_5,
        longitude: 3.019_5,
    },
    ArchiveStation {
        id: "07627",
        name: "ST-GIRONS",
        latitude: 43.005_333,
        longitude: 1.106_833,
    },
    ArchiveStation {
        id: "07630",
        name: "TOULOUSE-BLAGNAC",
        latitude: 43.621,
        longitude: 1.378_833,
    },
    ArchiveStation {
        id: "07747",
        name: "PERPIGNAN",
        latitude: 42.737_167,
        longitude: 2.872_833,
    },
];

/// Official SYNOP archive organized as monthly `synop.YYYYMM.csv.gz` files.
#[derive(Clone, Debug)]
pub struct MeteoArchive {
    path: PathBuf,
}

impl MeteoArchive {
    /// Creates an archive reader for one file or a monthly-file directory.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Loads complete noon observations for every date in an inclusive interval.
    ///
    /// # Errors
    ///
    /// Returns an error when a required archive is unavailable, malformed, or
    /// does not contain at least one complete configured station for each day.
    pub fn load(
        &self,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<BTreeMap<NaiveDate, Vec<WeatherStationObservation>>, SourceError> {
        if from > to {
            return Err(SourceError::InvalidStaticRecord(
                "backtest start date must not follow end date".to_owned(),
            ));
        }
        let paths = if self.path.is_dir() {
            monthly_paths(&self.path, from, to)?
        } else {
            vec![self.path.clone()]
        };
        let mut observations = BTreeMap::<NaiveDate, Vec<WeatherStationObservation>>::new();
        let mut identities = HashSet::<(NaiveDate, String)>::new();
        for path in paths {
            load_file(&path, from, to, &mut observations, &mut identities)?;
        }

        let mut date = from;
        loop {
            if observations.get(&date).is_none_or(Vec::is_empty) {
                return Err(SourceError::InvalidStaticRecord(format!(
                    "SYNOP archive has no complete noon station for {date}"
                )));
            }
            if date == to {
                break;
            }
            date = date
                .succ_opt()
                .ok_or_else(|| SourceError::InvalidTimestamp {
                    value: date.to_string(),
                })?;
        }
        for stations in observations.values_mut() {
            stations.sort_by(|left, right| left.station_id.cmp(&right.station_id));
        }
        Ok(observations)
    }
}

fn monthly_paths(
    directory: &Path,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<PathBuf>, SourceError> {
    let mut month = from
        .with_day(1)
        .ok_or_else(|| SourceError::InvalidTimestamp {
            value: from.to_string(),
        })?;
    let final_month = to
        .with_day(1)
        .ok_or_else(|| SourceError::InvalidTimestamp {
            value: to.to_string(),
        })?;
    let mut paths = Vec::new();
    loop {
        paths.push(directory.join(format!(
            "synop.{:04}{:02}.csv.gz",
            month.year(),
            month.month()
        )));
        if month == final_month {
            break;
        }
        month = month.checked_add_months(Months::new(1)).ok_or_else(|| {
            SourceError::InvalidTimestamp {
                value: month.to_string(),
            }
        })?;
    }
    Ok(paths)
}

fn load_file(
    path: &Path,
    from: NaiveDate,
    to: NaiveDate,
    observations: &mut BTreeMap<NaiveDate, Vec<WeatherStationObservation>>,
    identities: &mut HashSet<(NaiveDate, String)>,
) -> Result<(), SourceError> {
    let file = File::open(path).map_err(|source| SourceError::FixtureRead {
        path: path.to_owned(),
        source,
    })?;
    let input: Box<dyn Read> = if path.extension().is_some_and(|extension| extension == "gz") {
        Box::new(GzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut reader = csv::ReaderBuilder::new().delimiter(b';').from_reader(input);
    for row in reader.deserialize::<ArchiveRecord>() {
        let row = row?;
        let timestamp =
            NaiveDateTime::parse_from_str(&row.timestamp, "%Y%m%d%H%M%S").map_err(|_| {
                SourceError::InvalidTimestamp {
                    value: row.timestamp.clone(),
                }
            })?;
        let date = timestamp.date();
        let Some(station) = ARCHIVE_STATIONS
            .iter()
            .find(|station| station.id == row.station_id)
        else {
            continue;
        };
        if date < from || date > to || timestamp.time() != SYNOP_NOON_UTC {
            continue;
        }
        let (Some(wind), Some(temperature), Some(humidity), Some(precipitation)) =
            (row.wind, row.temperature, row.humidity, row.precipitation)
        else {
            continue;
        };
        if !identities.insert((date, row.station_id.clone())) {
            continue;
        }
        observations
            .entry(date)
            .or_default()
            .push(WeatherStationObservation {
                station_id: row.station_id,
                station_name: station.name.to_owned(),
                latitude: station.latitude,
                longitude: station.longitude,
                temperature_c: temperature - KELVIN_OFFSET,
                relative_humidity_pct: humidity,
                wind_speed_kmh: wind * METRES_PER_SECOND_TO_KILOMETRES_PER_HOUR,
                precipitation_24h_mm: precipitation.max(0.0),
            });
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct ArchiveRecord {
    #[serde(rename = "numer_sta")]
    station_id: String,
    #[serde(rename = "date")]
    timestamp: String,
    #[serde(rename = "ff", deserialize_with = "optional_float")]
    wind: Option<f64>,
    #[serde(rename = "t", deserialize_with = "optional_float")]
    temperature: Option<f64>,
    #[serde(rename = "u", deserialize_with = "optional_float")]
    humidity: Option<f64>,
    #[serde(rename = "rr24", deserialize_with = "optional_float")]
    precipitation: Option<f64>,
}

fn optional_float<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    let value = value.trim();
    if value.is_empty() || value == "mq" {
        Ok(None)
    } else {
        value.parse().map(Some).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use chrono::NaiveDate;

    use super::MeteoArchive;

    #[test]
    fn loads_official_archive_rows_and_skips_missing_values() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/meteo_france_synop_archive.csv");
        let archive = MeteoArchive::new(fixture);
        let from = NaiveDate::from_ymd_opt(2025, 6, 5).expect("valid date");
        let to = NaiveDate::from_ymd_opt(2025, 6, 6).expect("valid date");
        let days = archive.load(from, to).expect("valid archive");

        assert_eq!(days.len(), 2);
        assert_eq!(days[&from].len(), 4);
        assert_eq!(days[&to].len(), 3);
        assert!((days[&from][0].temperature_c - 20.2).abs() < 0.001);
    }
}
