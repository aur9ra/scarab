/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Real ffprobe integration tests over the provided CC0 FLAC sample fixtures.
//!
//! Every probed path lives under a disposable copy of the committed fixture
//! library, so ffprobe is never given a committed fixture path during testing.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use scarab::{ProbeError, ProbedTrack, probe_track};

mod common;

use common::{SourceFileSnapshot, TempSandbox, copy_fixture_library};

/// One fixture row is: the path relative to the committed fixture library,
/// the exact duration ffprobe reports, and the tag map ffprobe reports.
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

/// Captures the source file, probes it, asserts the captured source is
/// unchanged, then returns the probe result unchanged so callers inspect the
/// result only after the immutability check has completed.
fn probe_track_checked(path: &Path) -> Result<ProbedTrack, ProbeError> {
    let snapshot = SourceFileSnapshot::capture(path);
    let result = probe_track(path);
    snapshot.assert_unchanged();
    result
}

#[test]
fn probes_every_committed_fixture() {
    let sandbox = TempSandbox::new();
    let library = sandbox.path().join("library");
    copy_fixture_library(&library);

    for (file_path, expected_duration, expected_tags) in FIXTURES {
        let copied = library.join(file_path);

        let probed = probe_track_checked(&copied)
            .unwrap_or_else(|error| panic!("probing {} failed: {error}", copied.display()));

        assert_eq!(probed.path, copied);
        assert_eq!(probed.duration, Some(*expected_duration), "{file_path}");
        assert_eq!(probed.tags, tag_map(expected_tags), "{file_path}");
    }
}

#[test]
fn preserves_exact_path() {
    let sandbox = TempSandbox::new();
    let library = sandbox.path().join("library");
    copy_fixture_library(&library);
    let copied = library.join("album-two/02-22sq.flac");

    // Derive a relative spelling from the test process CWD to the disposable
    // copy. Incompatible Windows prefixes or volumes cannot have this
    // relation, the redundant `.` spelling below still checks
    // disposable lexical preservation there.
    let relative = relative_spelling(&copied);
    #[cfg(unix)]
    assert!(
        relative.is_some(),
        "a relative spelling to {} must exist on Unix",
        copied.display()
    );

    if let Some(relative) = &relative {
        let probed = probe_track_checked(relative)
            .unwrap_or_else(|error| panic!("probing {} failed: {error}", relative.display()));
        // Raw OS-string comparison: path equality is component-normalized,
        // so it would not distinguish a preserved spelling from a rewritten one.
        assert_eq!(probed.path.as_os_str(), relative.as_os_str());
        assert!(probed.path.is_relative());
    }

    // a redundant path component must also survive untouched
    let base = relative.unwrap_or(copied);
    let redundant = redundant_dot_spelling(&base);

    let probed = probe_track_checked(&redundant)
        .unwrap_or_else(|error| panic!("probing {} failed: {error}", redundant.display()));
    // Raw OS-string comparison: path equality is component-normalized, so the
    // redundant `./` component must be checked as spelled.
    assert_eq!(probed.path.as_os_str(), redundant.as_os_str());
}

#[test]
fn nonexistent_path_reports_failed_probe_process() {
    let sandbox = TempSandbox::new();
    let library = sandbox.path().join("library");
    copy_fixture_library(&library);
    let missing = library.join("no-such-file.flac");
    assert!(
        !missing.exists(),
        "the copied root must not contain {}",
        missing.display()
    );

    // A nonexistent source has nothing to snapshot, so probe_track is
    // invoked directly without the checked wrapper.
    match probe_track(&missing) {
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

/// Returns `path` spelled relative to the test process CWD when possible.
///
/// A relative spelling always exists on Unix: the CWD is absolute and either
/// `path` is already relative, or it shares some root component(s) with the
/// CWD. `None` only occurs when an absolute `path` has a different prefix or
/// volume than the CWD, which is possible on Windows.
fn relative_spelling(path: &Path) -> Option<PathBuf> {
    if path.is_absolute() {
        let cwd = std::env::current_dir().expect("test process working directory");
        relative_from(&cwd, path)
    } else {
        // The path is already relative to the CWD.
        Some(path.to_path_buf())
    }
}

/// Derives a relative path from `from` to `to`. Returns `None` when
/// the paths share no leading component, such as Windows paths on different
/// volumes, where no relative spelling exists.
fn relative_from(from: &Path, to: &Path) -> Option<PathBuf> {
    let from_components: Vec<Component<'_>> = from.components().collect();
    let to_components: Vec<Component<'_>> = to.components().collect();
    let common = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(from, to)| from == to)
        .count();

    if common == 0 {
        return None;
    }

    let mut relative = PathBuf::new();
    for _ in common..from_components.len() {
        relative.push("..");
    }
    for component in &to_components[common..] {
        relative.push(component.as_os_str());
    }
    if relative.as_os_str().is_empty() {
        relative.push(".");
    }
    Some(relative)
}

/// Returns `path` with a redundant `.` component inserted before its final
/// component, i.e. "foo/bar/baz" -> "foo/bar/./baz".
///
/// Every other lexical component is retained.
fn redundant_dot_spelling(path: &Path) -> PathBuf {
    let parent = path.parent().expect("spelling under test has a parent");
    let name = path
        .file_name()
        .expect("spelling under test has a file name");
    let mut redundant = parent.to_path_buf();
    redundant.push(".");
    redundant.push(name);
    redundant
}
