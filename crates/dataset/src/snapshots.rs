//! Pure logic for selecting the correct feature snapshot for a given
//! query date. A snapshot must never be selected for a date before it was
//! actually available — this is the core anti-leakage rule for feature
//! snapshots (mission section 3: "quelle information était réellement
//! disponible à la date de cette observation ?").

use chrono::{DateTime, Utc};

/// The subset of `features.feature_snapshots` fields needed to decide
/// which snapshot applies at a given date.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapshotWindow {
    pub id: String,
    pub family: String,
    pub available_from: DateTime<Utc>,
    pub available_until: Option<DateTime<Utc>>,
}

/// Selects the snapshot that was actually available at `as_of`, among
/// `candidates` restricted to one `family`. Never returns a snapshot whose
/// `available_from` is after `as_of` (no future information). When
/// several snapshots are eligible, the most recently available one wins.
#[must_use]
pub fn select_snapshot_for_date(
    candidates: &[SnapshotWindow],
    as_of: DateTime<Utc>,
) -> Option<&SnapshotWindow> {
    candidates
        .iter()
        .filter(|candidate| candidate.available_from <= as_of)
        .filter(|candidate| candidate.available_until.is_none_or(|until| until > as_of))
        .max_by_key(|candidate| candidate.available_from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(
        id: &str,
        available_from: (i32, u32, u32),
        available_until: Option<(i32, u32, u32)>,
    ) -> SnapshotWindow {
        SnapshotWindow {
            id: id.to_owned(),
            family: "cell_static_bundle".to_owned(),
            available_from: Utc
                .with_ymd_and_hms(
                    available_from.0,
                    available_from.1,
                    available_from.2,
                    0,
                    0,
                    0,
                )
                .unwrap(),
            available_until: available_until
                .map(|(y, m, d)| Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap()),
        }
    }

    use chrono::TimeZone;

    #[test]
    fn selects_the_most_recently_available_eligible_snapshot() {
        let candidates = vec![
            window("older", (2024, 1, 1), None),
            window("newer", (2025, 1, 1), None),
        ];
        let as_of = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let selected = select_snapshot_for_date(&candidates, as_of).unwrap();
        assert_eq!(selected.id, "newer");
    }

    #[test]
    fn never_selects_a_snapshot_from_the_future() {
        let candidates = vec![
            window("past", (2020, 1, 1), None),
            window("future", (2030, 1, 1), None),
        ];
        let as_of = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        let selected = select_snapshot_for_date(&candidates, as_of).unwrap();
        assert_eq!(selected.id, "past");
    }

    #[test]
    fn returns_none_when_only_future_snapshots_exist() {
        let candidates = vec![window("future", (2030, 1, 1), None)];
        let as_of = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
        assert!(select_snapshot_for_date(&candidates, as_of).is_none());
    }

    #[test]
    fn respects_available_until_expiry() {
        let candidates = vec![window("expired", (2020, 1, 1), Some((2021, 1, 1)))];
        let as_of = Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap();
        assert!(select_snapshot_for_date(&candidates, as_of).is_none());
    }

    #[test]
    fn returns_none_for_empty_candidates() {
        assert!(select_snapshot_for_date(&[], Utc::now()).is_none());
    }
}
