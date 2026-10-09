/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::*;

#[test]
fn empty_format_object_is_a_valid_empty_response() {
    let response = serde_json::from_str::<FfprobeResponse>(r#"{"format": {}}"#).unwrap();

    assert_eq!(response.format.duration, None);
}

#[test]
fn missing_format_object_is_an_invalid_response() {
    assert!(serde_json::from_str::<FfprobeResponse>("{}").is_err());
}

#[test]
fn null_format_object_is_an_invalid_response() {
    assert!(serde_json::from_str::<FfprobeResponse>(r#"{"format": null}"#).is_err());
}

#[test]
fn duration_field_parses_decimal_seconds() {
    let response =
        serde_json::from_str::<FfprobeResponse>(r#"{"format": {"duration": "19.000000"}}"#)
            .unwrap();

    assert_eq!(
        response.format.duration,
        Some(Duration::from_nanos(19_000_000_000))
    );
}

#[test]
fn null_duration_yields_no_duration() {
    let response =
        serde_json::from_str::<FfprobeResponse>(r#"{"format": {"duration": null}}"#).unwrap();

    assert_eq!(response.format.duration, None);
}

#[test]
fn non_string_duration_is_an_invalid_response() {
    assert!(serde_json::from_str::<FfprobeResponse>(r#"{"format": {"duration": 19.0}}"#).is_err());
}

#[test]
fn malformed_duration_is_an_invalid_response() {
    assert!(
        serde_json::from_str::<FfprobeResponse>(r#"{"format": {"duration": "not-a-number"}}"#)
            .is_err()
    );
}

#[test]
fn out_of_range_duration_is_an_invalid_response() {
    assert!(
        serde_json::from_str::<FfprobeResponse>(r#"{"format": {"duration": "-1.5"}}"#).is_err()
    );
    assert!(serde_json::from_str::<FfprobeResponse>(r#"{"format": {"duration": "inf"}}"#).is_err());
}
