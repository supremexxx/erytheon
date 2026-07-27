//! Pilot-only deterministic negative candidate selection, used solely to
//! exercise the dataset architecture end to end. This is NOT the final
//! scientific negative strategy (mission section 21): the real strategy —
//! stratification, exclusion windows validated by sensitivity analysis,
//! ratio selection — is a separate, later mission.

use chrono::{Datelike, NaiveDate};

pub const PILOT_STRATEGY_ID: &str = "pilot_only_deterministic_hash_v1";

/// One combustible, event-free cell-day eligible to become a pilot negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EligibleCellDay {
    pub h3: i64,
    pub date: NaiveDate,
}

/// Deterministically selects up to `count` pilot negatives from `eligible`
/// candidates using `seed`. Ordering and selection depend only on
/// `(h3, date, seed)`, never on database row order.
#[must_use]
pub fn select_pilot_negatives(
    eligible: &[EligibleCellDay],
    seed: u64,
    count: usize,
) -> Vec<EligibleCellDay> {
    let mut scored: Vec<(u64, EligibleCellDay)> = eligible
        .iter()
        .map(|candidate| {
            (
                mix64(hash_cell_day(candidate.h3, candidate.date) ^ seed),
                *candidate,
            )
        })
        .collect();
    scored.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.h3.cmp(&right.1.h3))
            .then_with(|| left.1.date.cmp(&right.1.date))
    });
    scored
        .into_iter()
        .take(count)
        .map(|(_, candidate)| candidate)
        .collect()
}

fn hash_cell_day(h3: i64, date: NaiveDate) -> u64 {
    mix64(h3.cast_unsigned()) ^ mix64(u64::from(date.num_days_from_ce().unsigned_abs()))
}

/// `splitmix64`-style deterministic mixing, stable across platforms and runs.
const fn mix64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^= x >> 33;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(h3: i64, ymd: (i32, u32, u32)) -> EligibleCellDay {
        EligibleCellDay {
            h3,
            date: NaiveDate::from_ymd_opt(ymd.0, ymd.1, ymd.2).unwrap(),
        }
    }

    #[test]
    fn selection_is_deterministic_for_a_given_seed() {
        let eligible = vec![
            candidate(1, (2023, 6, 1)),
            candidate(2, (2023, 6, 2)),
            candidate(3, (2023, 6, 3)),
            candidate(4, (2023, 6, 4)),
        ];
        let first = select_pilot_negatives(&eligible, 42, 2);
        let second = select_pilot_negatives(&eligible, 42, 2);
        assert_eq!(first, second);
        assert_eq!(first.len(), 2);
    }

    #[test]
    fn different_seeds_can_change_selection() {
        let eligible = vec![
            candidate(1, (2023, 6, 1)),
            candidate(2, (2023, 6, 2)),
            candidate(3, (2023, 6, 3)),
            candidate(4, (2023, 6, 4)),
        ];
        let a = select_pilot_negatives(&eligible, 1, 2);
        let b = select_pilot_negatives(&eligible, 2, 2);
        assert_ne!(a, b, "different seeds should very likely reorder the pool");
    }

    #[test]
    fn selection_is_independent_of_input_order() {
        let eligible = vec![
            candidate(1, (2023, 6, 1)),
            candidate(2, (2023, 6, 2)),
            candidate(3, (2023, 6, 3)),
        ];
        let mut reversed = eligible.clone();
        reversed.reverse();
        let a = select_pilot_negatives(&eligible, 7, 2);
        let b = select_pilot_negatives(&reversed, 7, 2);
        assert_eq!(a, b);
    }
}
