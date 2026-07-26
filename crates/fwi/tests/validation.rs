use fwi::{FwiError, FwiState, Weather, calculate_daily};

#[test]
fn standard_initial_state_is_documented_values() {
    assert_eq!(
        FwiState::default(),
        FwiState {
            ffmc: 85.0,
            dmc: 6.0,
            dc: 15.0,
        }
    );
}

#[test]
fn rejects_invalid_weather() {
    let error = calculate_daily(
        Weather {
            temperature_c: 20.0,
            relative_humidity_pct: 101.0,
            wind_speed_kmh: 10.0,
            precipitation_mm: 0.0,
            month: 7,
        },
        FwiState::default(),
    )
    .expect_err("humidity above 100 must be rejected");

    assert!(matches!(
        error,
        FwiError::OutOfRange {
            field: "relative_humidity_pct",
            ..
        }
    ));
}

#[test]
fn accepts_saturated_air_without_non_finite_output() {
    let output = calculate_daily(
        Weather {
            temperature_c: 5.0,
            relative_humidity_pct: 100.0,
            wind_speed_kmh: 0.0,
            precipitation_mm: 20.0,
            month: 1,
        },
        FwiState::default(),
    )
    .expect("boundary weather should be valid");

    assert!(output.ffmc.is_finite());
    assert!(output.dmc.is_finite());
    assert!(output.dc.is_finite());
    assert!(output.isi.is_finite());
    assert!(output.bui.is_finite());
    assert!(output.fwi.is_finite());
}
