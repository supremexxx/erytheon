use fwi::{FwiOutput, FwiState, Weather, calculate_daily};

const REFERENCE: &str = include_str!("../../../testdata/fwi_reference.csv");

#[test]
fn reproduces_standard_cffdrs_sequence() {
    let mut state = FwiState::default();

    for (row_index, line) in REFERENCE.lines().skip(1).enumerate() {
        let columns: Vec<&str> = line.split(',').collect();
        assert_eq!(columns.len(), 13, "invalid fixture row {}", row_index + 2);

        let weather = Weather {
            month: parse(columns[1]),
            temperature_c: parse(columns[3]),
            relative_humidity_pct: parse(columns[4]),
            wind_speed_kmh: parse(columns[5]),
            precipitation_mm: parse(columns[6]),
        };
        let output = calculate_daily(weather, state).expect("reference input should be valid");

        assert_output_matches(row_index + 2, output, &columns[7..13]);
        state = output.state();
    }
}

fn assert_output_matches(row: usize, actual: FwiOutput, expected: &[&str]) {
    for (name, actual_value, expected_token) in [
        ("ffmc", actual.ffmc, expected[0]),
        ("dmc", actual.dmc, expected[1]),
        ("dc", actual.dc, expected[2]),
        ("isi", actual.isi, expected[3]),
        ("bui", actual.bui, expected[4]),
        ("fwi", actual.fwi, expected[5]),
    ] {
        let expected_value: f64 = parse(expected_token);
        let tolerance = source_rounding_tolerance(expected_token) + 1.0e-10;
        let difference = (actual_value - expected_value).abs();
        assert!(
            difference <= tolerance,
            "row {row} {name}: expected {expected_value} ± {tolerance}, got {actual_value}"
        );
    }
}

fn source_rounding_tolerance(token: &str) -> f64 {
    let (mantissa, exponent) =
        token
            .split_once(['e', 'E'])
            .map_or((token, 0), |(mantissa, exponent)| {
                (
                    mantissa,
                    exponent.parse::<i32>().expect("valid scientific exponent"),
                )
            });
    let decimal_places = i32::try_from(
        mantissa
            .split_once('.')
            .map_or(0, |(_, decimals)| decimals.len()),
    )
    .expect("fixture precision should fit in i32");
    0.5 * 10.0_f64.powi(exponent - decimal_places)
}

fn parse<T>(token: &str) -> T
where
    T: std::str::FromStr,
    T::Err: std::fmt::Debug,
{
    token.parse().expect("fixture value should be valid")
}
