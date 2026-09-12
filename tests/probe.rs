/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Real ffprobe integration tests over the provided CC0 FLAC sample fixtures.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use scarab::{ProbeError, probe_track};

mod common;

use common::SourceFileSnapshot;

/// Library fixture root, relative to the crate root that cargo test runs in.
const LIBRARY: &str = "tests/fixtures/library";

/// One fixture row is: the path relative to `LIBRARY`, the exact duration
/// ffprobe reports, and the tag map ffprobe reports.
type FixtureRow = (
    &'static str,
    Duration,
    &'static [(&'static str, &'static str)],
);

/// One row per committed fixture.
const FIXTURES: &[FixtureRow] = &[
    (
        "album-one/01-flamenco-road.flac",
        Duration::from_nanos(46_419_592_000),
        &[
            ("ALBUM", "Fixture Album One"),
            ("album_artist", "Fixture Artist One"),
            ("ARTIST", "Fixture Artist One"),
            ("TITLE", "Flamenco Road"),
            ("track", "1"),
        ],
    ),
    (
        "album-one/02-blackbird.flac",
        Duration::from_nanos(4_239_751_000),
        &[
            ("ALBUM", "Fixture Album One"),
            ("album_artist", "Fixture Artist One"),
            ("ARTIST", "Fixture Artist One"),
            ("TITLE", "Blackbird"),
            ("track", "2"),
        ],
    ),
    (
        "album-one/03-minuet.flac",
        Duration::from_nanos(52_273_542_000),
        &[
            ("ALBUM", "Fixture Album One"),
            ("album_artist", "Fixture Artist One"),
            ("ARTIST", "Fixture Artist One"),
            ("TITLE", "Minuet in G Major"),
            ("track", "3"),
        ],
    ),
    (
        "album-two/01-hans-im-schnokeloch.flac",
        Duration::from_nanos(19_000_000_000),
        &[
            ("ALBUM", "Fixture Album Two"),
            ("album_artist", "Fixture Artist Two"),
            ("ARTIST", "Fixture Artist Two"),
            ("TITLE", "D'r Hans im Schnokeloch"),
            ("track", "1"),
        ],
    ),
    (
        "album-two/02-22sq.flac",
        Duration::from_nanos(38_800_544_000),
        &[
            ("ALBUM", "Fixture Album Two"),
            ("album_artist", "Fixture Artist Two"),
            ("ARTIST", "Fixture Artist Two"),
            ("TITLE", "22SQ"),
            ("track", "2"),
        ],
    ),
];

fn tag_map(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

/// Captures the source file, then returns the probe result unchanged so
/// callers can assert source immutability before inspecting the result.
fn probe(relative: &str) -> (SourceFileSnapshot, Result<scarab::ProbedTrack, ProbeError>) {
    let snapshot = SourceFileSnapshot::capture(relative);
    let result = probe_track(Path::new(relative));
    (snapshot, result)
}

#[test]
fn probes_every_committed_fixture() {
    for (file, expected_duration, expected_tags) in FIXTURES {
        let relative = format!("{LIBRARY}/{file}");
        let (snapshot, result) = probe(&relative);

        snapshot.assert_unchanged();

        let probed = result.unwrap_or_else(|error| panic!("probing {relative} failed: {error}"));

        assert_eq!(probed.path, snapshot.path());
        assert_eq!(probed.duration, Some(*expected_duration), "{relative}");
        assert_eq!(probed.tags, tag_map(expected_tags), "{relative}");
    }
}

#[test]
fn preserves_exact_path() {
    let relative = "tests/fixtures/library/album-two/02-22sq.flac";
    let (snapshot, result) = probe(relative);

    snapshot.assert_unchanged();

    let probed = result.unwrap_or_else(|error| panic!("probing {relative} failed: {error}"));
    assert_eq!(probed.path, PathBuf::from(relative));
    assert!(probed.path.is_relative());

    // a redundant path component must also survive untouched
    let redundant = "tests/fixtures/library/./album-two/02-22sq.flac";
    let (snapshot, result) = probe(redundant);

    snapshot.assert_unchanged();

    let probed = result.unwrap_or_else(|error| panic!("probing {redundant} failed: {error}"));
    assert_eq!(probed.path, PathBuf::from(redundant));
}

#[test]
fn nonexistent_path_reports_failed_probe_process() {
    let missing = "tests/fixtures/library/no-such-file.flac";

    match probe_track(Path::new(missing)) {
        Err(ProbeError::Exit { status, stderr }) => {
            assert!(!status.success());
            assert!(
                !stderr.trim().is_empty(),
                "expected stderr diagnostics, got: {stderr:?}"
            );
        }
        Err(other) => panic!("expected non-success exit, got: {other}"),
        Ok(probed) => panic!("expected probe failure, got: {probed:?}"),
    }
}
