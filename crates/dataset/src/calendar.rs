//! Deterministic French calendar rules: public holidays (fixed by law and
//! therefore correctly computable for any past year), meteorological
//! seasons, and the seasonal sine/cosine encoding already used by
//! [`risk::human_feature_vector`]. School holidays are NOT computed here:
//! they are zone-specific administrative decisions with no reliable
//! historical source for 2020-2024 in this environment, so callers must
//! supply them from a verified source and leave them absent otherwise.

use std::f64::consts::TAU;

use chrono::{Datelike, NaiveDate, Weekday};

/// One computed calendar day, before any school-holiday information is merged in.
// Each flag is an independent, well-named calendar fact (not a state
// machine over mutually exclusive states), so keeping them as plain
// booleans stays clearer than an enum here.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq)]
pub struct CalendarDay {
    pub date: NaiveDate,
    pub year: i32,
    pub month: u32,
    /// ISO weekday as 0 (Monday) through 6 (Sunday).
    pub day_of_week: u32,
    pub is_weekend: bool,
    pub public_holiday: bool,
    pub public_holiday_label: Option<String>,
    pub is_day_before_public_holiday: bool,
    pub is_day_after_public_holiday: bool,
    /// Meteorological season: 0 winter, 1 spring, 2 summer, 3 autumn.
    pub season: u8,
    pub season_sine: f64,
    pub season_cosine: f64,
}

#[must_use]
pub fn day_of_week_iso(date: NaiveDate) -> u32 {
    date.weekday().num_days_from_monday()
}

#[must_use]
pub fn is_weekend(date: NaiveDate) -> bool {
    matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

#[must_use]
pub fn season(date: NaiveDate) -> u8 {
    match date.month() {
        12 | 1 | 2 => 0,
        3..=5 => 1,
        6..=8 => 2,
        _ => 3,
    }
}

#[must_use]
pub fn season_sine_cosine(date: NaiveDate) -> (f64, f64) {
    let angle = TAU * (f64::from(date.ordinal0()) / 365.25);
    (angle.sin(), angle.cos())
}

/// Easter Sunday (Gregorian calendar) via the Meeus/Jones/Butcher algorithm.
///
/// # Panics
///
/// Never panics for any `year` the Gregorian calendar can represent: the
/// algorithm always yields a month/day pair that forms a valid date.
#[must_use]
#[allow(clippy::many_single_char_names)] // canonical Meeus/Jones/Butcher variable names
pub fn easter_sunday(year: i32) -> NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
    .expect("Easter computation always yields a valid calendar date")
}

/// French public holidays for one civil year: fixed dates plus Easter Monday,
/// Ascension, and Whit (Pentecost) Monday. Fixed by law; correctly
/// computable retroactively for any year, unlike school holidays.
///
/// # Panics
///
/// Never panics for any `year` the Gregorian calendar can represent: every
/// fixed date below (day, month) is valid for every year.
#[must_use]
pub fn public_holidays(year: i32) -> Vec<(NaiveDate, &'static str)> {
    let easter = easter_sunday(year);
    let mut holidays = vec![
        (NaiveDate::from_ymd_opt(year, 1, 1).unwrap(), "Jour de l'An"),
        (easter + chrono::Duration::days(1), "Lundi de Pâques"),
        (
            NaiveDate::from_ymd_opt(year, 5, 1).unwrap(),
            "Fête du Travail",
        ),
        (
            NaiveDate::from_ymd_opt(year, 5, 8).unwrap(),
            "Victoire 1945",
        ),
        (easter + chrono::Duration::days(39), "Ascension"),
        (easter + chrono::Duration::days(50), "Lundi de Pentecôte"),
        (
            NaiveDate::from_ymd_opt(year, 7, 14).unwrap(),
            "Fête Nationale",
        ),
        (NaiveDate::from_ymd_opt(year, 8, 15).unwrap(), "Assomption"),
        (NaiveDate::from_ymd_opt(year, 11, 1).unwrap(), "Toussaint"),
        (
            NaiveDate::from_ymd_opt(year, 11, 11).unwrap(),
            "Armistice 1918",
        ),
        (NaiveDate::from_ymd_opt(year, 12, 25).unwrap(), "Noël"),
    ];
    holidays.sort_by_key(|(date, _)| *date);
    holidays
}

/// Builds every [`CalendarDay`] for `year`, deterministically.
///
/// # Panics
///
/// Never panics for any `year` the Gregorian calendar can represent:
/// January 1st is always a valid date.
#[must_use]
pub fn build_year(year: i32) -> Vec<CalendarDay> {
    let holidays = public_holidays(year);
    let is_holiday = |date: NaiveDate| {
        holidays
            .iter()
            .find(|(day, _)| *day == date)
            .map(|(_, label)| *label)
    };
    let mut days = Vec::new();
    let mut date = NaiveDate::from_ymd_opt(year, 1, 1).expect("valid year start");
    while date.year() == year {
        let (season_sine, season_cosine) = season_sine_cosine(date);
        let label = is_holiday(date);
        days.push(CalendarDay {
            date,
            year,
            month: date.month(),
            day_of_week: day_of_week_iso(date),
            is_weekend: is_weekend(date),
            public_holiday: label.is_some(),
            public_holiday_label: label.map(ToOwned::to_owned),
            is_day_before_public_holiday: is_holiday(date + chrono::Duration::days(1)).is_some(),
            is_day_after_public_holiday: is_holiday(date - chrono::Duration::days(1)).is_some(),
            season: season(date),
            season_sine,
            season_cosine,
        });
        date += chrono::Duration::days(1);
    }
    days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easter_matches_known_dates() {
        // Published reference dates.
        assert_eq!(
            easter_sunday(2020),
            NaiveDate::from_ymd_opt(2020, 4, 12).unwrap()
        );
        assert_eq!(
            easter_sunday(2023),
            NaiveDate::from_ymd_opt(2023, 4, 9).unwrap()
        );
        assert_eq!(
            easter_sunday(2024),
            NaiveDate::from_ymd_opt(2024, 3, 31).unwrap()
        );
        assert_eq!(
            easter_sunday(2025),
            NaiveDate::from_ymd_opt(2025, 4, 20).unwrap()
        );
        assert_eq!(
            easter_sunday(2026),
            NaiveDate::from_ymd_opt(2026, 4, 5).unwrap()
        );
    }

    #[test]
    fn fixed_holidays_present_every_year() {
        for year in 2020..=2026 {
            let holidays = public_holidays(year);
            assert!(
                holidays
                    .iter()
                    .any(|(d, _)| *d == NaiveDate::from_ymd_opt(year, 1, 1).unwrap())
            );
            assert!(
                holidays
                    .iter()
                    .any(|(d, _)| *d == NaiveDate::from_ymd_opt(year, 7, 14).unwrap())
            );
            assert_eq!(holidays.len(), 11, "France has 11 public holidays per year");
        }
    }

    #[test]
    fn build_year_covers_every_day_deterministically() {
        let days_2024 = build_year(2024);
        assert_eq!(days_2024.len(), 366, "2024 is a leap year");
        let days_2025 = build_year(2025);
        assert_eq!(days_2025.len(), 365);
        // Rebuilding is fully deterministic.
        assert_eq!(build_year(2024), days_2024);
    }

    #[test]
    fn weekend_detection_is_correct() {
        // 2024-01-06 is a Saturday.
        assert!(is_weekend(NaiveDate::from_ymd_opt(2024, 1, 6).unwrap()));
        // 2024-01-08 is a Monday.
        assert!(!is_weekend(NaiveDate::from_ymd_opt(2024, 1, 8).unwrap()));
    }

    #[test]
    fn season_classification_is_meteorological() {
        assert_eq!(season(NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()), 0);
        assert_eq!(season(NaiveDate::from_ymd_opt(2024, 4, 15).unwrap()), 1);
        assert_eq!(season(NaiveDate::from_ymd_opt(2024, 7, 15).unwrap()), 2);
        assert_eq!(season(NaiveDate::from_ymd_opt(2024, 10, 15).unwrap()), 3);
        assert_eq!(season(NaiveDate::from_ymd_opt(2024, 12, 15).unwrap()), 0);
    }

    #[test]
    fn no_future_information_leaks_into_a_past_year() {
        // Building 2020 must not depend on any date outside 2020.
        let days = build_year(2020);
        assert!(days.iter().all(|day| day.date.year() == 2020));
        assert_eq!(days.len(), 366, "2020 is a leap year");
    }
}
