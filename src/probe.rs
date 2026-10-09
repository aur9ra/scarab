/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Obtaining source-file format duration with `ffprobe`.
//!
//! [`probe_source_file`] requests format duration for a supplied path
//! using `ffprobe`. No prior source-audio classification is required.

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

/// The ffprobe executable Scarab invokes. Looked up on `PATH` by name.
const FFPROBE: &str = "ffprobe";

/// Requests format duration from ffprobe for the supplied path.
///
/// The path is passed unchanged and stored unchanged. It is never
/// canonicalized, absolutized, or otherwise rewritten. Success means ffprobe
/// exited successfully and returned a valid JSON format response. The format
/// duration may be absent.
///
/// It does not classify the path, establish collection membership, identify or finalize
/// a logical track, or determine output inclusion. No prior classification is
/// required.
pub fn probe_source_file(path: &Path) -> Result<ProbedSourceFile, ProbeError> {
    let output = Command::new(FFPROBE)
        .args([
            "-v",
            "error",
            "-of",
            "json",
            "-show_entries",
            "format=duration",
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

    Ok(ProbedSourceFile {
        path: path.to_path_buf(),
        duration: response.format.duration,
    })
}

/// The requested ffprobe format duration for a supplied path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbedSourceFile {
    /// The exact path passed to [`probe_source_file`].
    pub path: PathBuf,
    /// Format duration parsed from ffprobe's decimal-seconds value, if present.
    pub duration: Option<Duration>,
}

/// A failure to probe a source file with ffprobe.
#[derive(Debug)]
#[non_exhaustive]
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
    /// ffprobe exited successfully, but its JSON response did not match the
    /// required format representation.
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

/// An ffprobe response with a required `format` object. The duration inside it
/// is optional.
#[derive(Deserialize)]
struct FfprobeResponse {
    format: FfprobeFormatEntry,
}

/// The selected `format` entry of an ffprobe response.
#[derive(Deserialize)]
struct FfprobeFormatEntry {
    #[serde(default, deserialize_with = "duration_from_seconds")]
    duration: Option<Duration>,
}

/// Deserializes an ffprobe duration reported as decimal seconds. A missing or
/// null field means no duration. A non-string, unparseable, or out-of-range
/// value is a deserialization error.
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
#[path = "tests/probe.rs"]
mod tests;
