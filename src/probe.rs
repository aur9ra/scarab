/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Probing source files with ffprobe.
//!
//! [`probe_track`] invokes `ffprobe` on a supplied path and
//! returns its format metadata. Whether ffprobe can read the
//! input is ffprobe's own decision - Scarab forwards the path untouched.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

/// The ffprobe executable Scarab invokes. Looked up on `PATH` by name.
const FFPROBE: &str = "ffprobe";

/// Probes one source file with ffprobe and returns its metadata.
///
/// The supplied path is passed to ffprobe exactly as given and is copied
/// unchanged into the result: it is never canonicalized, absolutized, or
/// otherwise rewritten. A path ffprobe cannot open is reported as
/// [`ProbeError::Exit`] together with ffprobe's stderr diagnostics.
pub fn probe_track(path: &Path) -> Result<ProbedTrack, ProbeError> {
    let output = Command::new(FFPROBE)
        .args([
            "-v",
            "error",
            "-of",
            "json",
            "-show_entries",
            "format=duration:format_tags",
        ])
        .arg(path)
        // ffprobe writes report files when FFREPORT is inherited
        .env_remove("FFREPORT")
        .output()
        .map_err(ProbeError::Spawn)?;

    if !output.status.success() {
        return Err(ProbeError::Exit {
            status: output.status,
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let response =
        serde_json::from_slice::<FfprobeResponse>(&output.stdout).map_err(ProbeError::Response)?;

    Ok(ProbedTrack {
        path: path.to_path_buf(),
        tags: response.format.tags,
        duration: response.format.duration,
    })
}

/// ffprobe's view of one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedTrack {
    /// The unmodified path passed to [`probe_track`].
    pub path: PathBuf,
    /// The format tags ffprobe reported, with keys and values exactly as
    /// reported. A missing tags field yields an empty map.
    pub tags: BTreeMap<String, String>,
    /// The format duration ffprobe reported, if any.
    pub duration: Option<Duration>,
}

/// A failure to probe a source file with ffprobe.
#[derive(Debug)]
pub enum ProbeError {
    /// Error starting ffprobe.
    Spawn(std::io::Error),
    /// ffprobe started but exited with a non-success status.
    Exit {
        /// The process's exit status.
        status: std::process::ExitStatus,
        /// The process's stderr diagnostics.
        stderr: String,
    },
    /// ffprobe succeeded (status code 0) but its JSON
    /// response shape could not be deserialized.
    Response(serde_json::Error),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeError::Spawn(error) => write!(f, "ffprobe could not be started: {error}"),
            ProbeError::Exit { status, stderr } => {
                write!(f, "ffprobe exited with {status}: {}", stderr.trim())
            }
            ProbeError::Response(error) => {
                write!(f, "ffprobe returned an invalid response: {error}")
            }
        }
    }
}

impl std::error::Error for ProbeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProbeError::Spawn(error) => Some(error),
            ProbeError::Exit { .. } => None,
            ProbeError::Response(error) => Some(error),
        }
    }
}

/// An ffprobe response. The format object is required, only fields
/// inside it are optional: tags defaultly initializes to an empty [`BTreeMap`].
#[derive(Deserialize)]
struct FfprobeResponse {
    format: FfprobeFormatEntry,
}

/// The selected `format` entry of an ffprobe response.
#[derive(Deserialize)]
struct FfprobeFormatEntry {
    #[serde(default, deserialize_with = "duration_from_seconds")]
    duration: Option<Duration>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
}

/// Deserializes an ffprobe duration reported as decimal seconds. A missing or
/// null field means no duration. Any other unparseable value is a
/// deserialization error.
fn duration_from_seconds<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let reported = Option::<String>::deserialize(deserializer)?;
    let Some(reported) = reported else {
        return Ok(None);
    };

    let seconds = reported.parse::<f64>().map_err(|_| {
        serde::de::Error::custom(format!("ffprobe duration `{reported}` is not a number"))
    })?;

    // 20 imaginary bucks to the first person who hits this in real usage
    // "yes, i'd like a track 10^300 times longer than the age of the universe"
    // "please compress this"
    Duration::try_from_secs_f64(seconds).map(Some).map_err(|_| {
        serde::de::Error::custom(format!("ffprobe duration `{reported}` is out of range"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_format_object_is_a_valid_empty_response() {
        let response = serde_json::from_str::<FfprobeResponse>(r#"{"format": {}}"#).unwrap();

        assert!(response.format.tags.is_empty());
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
    fn missing_tags_field_yields_an_empty_map() {
        let response =
            serde_json::from_str::<FfprobeResponse>(r#"{"format": {"duration": "19.000000"}}"#)
                .unwrap();

        assert!(response.format.tags.is_empty());
        assert_eq!(
            response.format.duration,
            Some(Duration::from_nanos(19_000_000_000))
        );
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
        assert!(
            serde_json::from_str::<FfprobeResponse>(r#"{"format": {"duration": "inf"}}"#).is_err()
        );
    }
}
