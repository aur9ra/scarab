/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Integration tests for source-file discovery.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use scarab::{DiscoveryError, discover_source_files};

mod common;

use common::{SourceTreeSnapshot, TestSandbox};

/// Library fixture root, relative to the crate root that cargo test runs in.
const LIBRARY: &str = "tests/fixtures/library";

/// Every ordinary file under LIBRARY.
const LIBRARY_FILES: &[&str] = &[
    "README.md",
    "album-one/01-flamenco-road.flac",
    "album-one/02-blackbird.flac",
    "album-one/03-minuet.flac",
    "album-two/01-hans-im-schnokeloch.flac",
    "album-two/02-22sq.flac",
];

/// Runs discovery over `root` and asserts the source tree is untouched
/// before inspecting the result. Returns `result` as a sorted [`Vec<PathBuf>`].
fn discover_checked(root: &Path) -> Result<Vec<PathBuf>, DiscoveryError> {
    let snapshot = SourceTreeSnapshot::capture(root);
    let result = discover_source_files(root);
    snapshot.assert_unchanged();
    result
}

/// Builds the expected discovery result: every fixture joined
/// under `root`. Sorting is handled by [`PathBuf`]'s [`Ord`]
/// implementation, rather than predetermination.
fn expected_library_files_under_root(root: &Path) -> Vec<PathBuf> {
    let mut expected: Vec<PathBuf> = LIBRARY_FILES
        .iter()
        .map(|fixture| root.join(fixture))
        .collect();
    expected.sort();
    expected
}

/// The expected discovery result for the fixture library root.
fn expected_library_files() -> Vec<PathBuf> {
    expected_library_files_under_root(Path::new(LIBRARY))
}

#[test]
fn finds_every_fixture_file_recursively_with_root_prefix() {
    let discovered: Vec<PathBuf> =
        discover_checked(Path::new(LIBRARY)).expect("discovery over fixtures must succeed");

    assert_eq!(discovered, expected_library_files());
}

#[test]
fn supplied_root_spelling_is_preserved_and_honored() {
    // A redundant internal `./` component in the supplied root must be
    // retained verbatim in every discovered path, never normalized away.
    let root = Path::new("tests/fixtures/./library");

    let discovered = discover_checked(root).expect("discovery must succeed");

    // OS-string comparison pins the exact spelling, which plain
    // component-wise path equality would not distinguish from a normalized
    // form. Exact vector equality also requires the existing sorted order.
    let discovered_spelling: Vec<OsString> = discovered
        .iter()
        .map(|path| path.as_os_str().to_os_string())
        .collect();
    let expected_spelling: Vec<OsString> = expected_library_files_under_root(root)
        .into_iter()
        .map(|path| path.into_os_string())
        .collect();
    assert_eq!(discovered_spelling, expected_spelling);
}

#[test]
fn trailing_separator_and_dot_root_spelling_is_preserved() {
    // Root validation inspects a probe spelling with trailing separators and
    // `.` components stripped, but traversal and returned paths must keep the
    // caller's spelling verbatim.
    for spelling in ["tests/fixtures/library/.", "tests/fixtures/library/"] {
        let root = Path::new(spelling);

        let discovered = discover_checked(root).expect("discovery must succeed");

        let discovered_spelling: Vec<OsString> = discovered
            .iter()
            .map(|path| path.as_os_str().to_os_string())
            .collect();
        let expected_spelling: Vec<OsString> = expected_library_files_under_root(root)
            .into_iter()
            .map(|path| path.into_os_string())
            .collect();
        assert_eq!(discovered_spelling, expected_spelling, "root {spelling}");
    }
}

#[test]
fn results_are_deterministically_sorted() {
    let first = discover_checked(Path::new(LIBRARY)).expect("first discovery must succeed");
    let second = discover_checked(Path::new(LIBRARY)).expect("second discovery must succeed");
    assert_eq!(first, second);
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(first, sorted);
}

#[test]
fn ordinary_files_are_collected_regardless_of_name_or_extension() {
    // Discovery is extension-blind. Dotfiles, mixed-case names, compound
    // extensions, and extensionless files are all ordinary files.
    let sandbox = TestSandbox::new();
    let root = sandbox.path().join("ordinary-files");
    fs::create_dir(&root).expect("create root");
    let names = [
        ".hidden",
        "UPPER.FLAC",
        "archive.tar.gz",
        "lower.flac",
        "mixed.FlaC",
        "no-extension",
        "notes.txt",
    ];
    for name in names {
        fs::write(root.join(name), name.as_bytes()).expect("write ordinary file");
    }

    let discovered = discover_checked(&root).expect("discovery must succeed");
    let mut expected: Vec<PathBuf> = names.iter().map(|name| root.join(name)).collect();
    expected.sort();
    assert_eq!(discovered, expected);
}

#[test]
fn nested_directories_beyond_album_layout_are_traversed() {
    // A tree deeper and more irregular than artist/album must still be walked.
    let sandbox = TestSandbox::new();
    let root = sandbox.path().join("nested");
    let deep = root.join("a/b/c");
    fs::create_dir_all(&deep).expect("create deep directories");
    let deep_file = deep.join("song.bin");
    fs::write(&deep_file, b"deep").expect("write deep file");
    let shallow_file = root.join("top.dat");
    fs::write(&shallow_file, b"top").expect("write top file");

    let discovered = discover_checked(&root).expect("discovery must succeed");
    let mut expected = vec![root.join("a/b/c/song.bin"), root.join("top.dat")];
    expected.sort();
    assert_eq!(discovered, expected);
}

#[test]
fn empty_directory_tree_yields_no_files() {
    let sandbox = TestSandbox::new();
    let root = sandbox.path().join("empty");
    fs::create_dir_all(root.join("empty-nested")).expect("create empty tree");

    let discovered = discover_checked(&root).expect("discovery must succeed");
    assert!(discovered.is_empty());
}

#[test]
fn missing_root_returns_typed_error() {
    let sandbox = TestSandbox::new();
    let root = sandbox.path().join("missing");

    // do not create the root: there is no tree to capture
    // thus discovery must fail

    let error = discover_source_files(&root).expect_err("missing root must fail");
    assert!(matches!(error, DiscoveryError::Root { .. }));
    assert!(
        error
            .to_string()
            .contains(&root.to_string_lossy().to_string())
    );
}

#[test]
fn regular_file_root_returns_typed_error() {
    let sandbox = TestSandbox::new();
    let root = sandbox.path().join("root-file");
    fs::write(&root, b"not a directory").expect("write root file");

    let error = discover_source_files(&root).expect_err("file root must fail");
    assert!(matches!(error, DiscoveryError::NotADirectory { .. }));
    assert!(
        error
            .to_string()
            .contains(&root.to_string_lossy().to_string())
    );
}

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    use scarab::{DiscoveryError, discover_source_files};

    use super::{SourceTreeSnapshot, TestSandbox, discover_checked};

    #[test]
    fn symlink_root_is_rejected() {
        let sandbox = TestSandbox::new();
        let real = sandbox.path().join("real");
        fs::create_dir(&real).expect("create real directory");
        fs::write(real.join("track.dat"), b"track").expect("write file");
        let link = sandbox.path().join("link");
        symlink(&real, &link).expect("create root symlink");

        // Root-symlink rejection is an error case with no capturable tree:
        // the snapshot helper refuses to follow the link, matching
        // discovery, so invoke discovery directly.
        let error = discover_source_files(&link).expect_err("symlink root must fail");
        assert!(matches!(error, DiscoveryError::RootSymlink { .. }));
        assert!(
            error
                .to_string()
                .contains(&link.to_string_lossy().to_string())
        );
    }

    #[test]
    fn symlink_root_with_trailing_separator_or_dot_is_rejected() {
        let sandbox = TestSandbox::new();
        let real = sandbox.path().join("real");
        fs::create_dir(&real).expect("create real directory");
        fs::write(real.join("track.dat"), b"track").expect("write file");
        let link = sandbox.path().join("link");
        symlink(&real, &link).expect("create root symlink");

        // POSIX resolution follows a terminal symlink when the spelling ends
        // in a separator or a `.` component, so these spellings must still be
        // rejected as a symlink root.
        for suffix in ["/", "/.", "/./", "///"] {
            let mut spelling = link.as_os_str().to_os_string();
            spelling.push(suffix);
            let spelled_root = PathBuf::from(spelling);

            let error = discover_source_files(&spelled_root).expect_err("symlink root must fail");
            assert!(
                matches!(error, DiscoveryError::RootSymlink { .. }),
                "root spelling {suffix:?} was not rejected as a symlink"
            );
            assert!(
                error
                    .to_string()
                    .contains(&link.to_string_lossy().to_string())
            );
        }
    }

    #[test]
    #[should_panic(expected = "is a symlink")]
    fn tree_snapshot_capture_rejects_symlink_root_with_trailing_separator() {
        let sandbox = TestSandbox::new();
        let real = sandbox.path().join("real");
        fs::create_dir(&real).expect("create real directory");
        let link = sandbox.path().join("link");
        symlink(&real, &link).expect("create root symlink");

        let mut spelling = link.as_os_str().to_os_string();
        spelling.push("/");
        SourceTreeSnapshot::capture(PathBuf::from(spelling));
    }

    #[test]
    #[should_panic(expected = "is a symlink")]
    fn tree_snapshot_capture_rejects_symlink_root_with_terminal_dot() {
        let sandbox = TestSandbox::new();
        let real = sandbox.path().join("real");
        fs::create_dir(&real).expect("create real directory");
        let link = sandbox.path().join("link");
        symlink(&real, &link).expect("create root symlink");

        let mut spelling = link.as_os_str().to_os_string();
        spelling.push("/.");
        SourceTreeSnapshot::capture(PathBuf::from(spelling));
    }

    #[test]
    fn symlinked_ordinary_files_are_skipped() {
        let sandbox = TestSandbox::new();
        let root = sandbox.path().join("files");
        fs::create_dir(&root).expect("create root");
        fs::write(root.join("real.flac"), b"real flac").expect("write real flac");
        fs::write(root.join("real.txt"), b"real txt").expect("write real text");
        symlink(root.join("real.flac"), root.join("linked.flac")).expect("link flac");
        symlink(root.join("real.txt"), root.join("linked.txt")).expect("link txt");

        let discovered = discover_checked(&root).expect("discovery must succeed");
        let mut expected = vec![root.join("real.flac"), root.join("real.txt")];
        expected.sort();
        assert_eq!(discovered, expected);
    }

    #[test]
    fn symlinked_directories_are_not_traversed() {
        let sandbox = TestSandbox::new();
        let root = sandbox.path().join("root");
        let outside = sandbox.path().join("outside");
        fs::create_dir_all(root.join("inner")).expect("create inner directory");
        fs::create_dir(&outside).expect("create outside directory");
        fs::write(root.join("inner/inside.dat"), b"inside").expect("write inside file");
        fs::write(outside.join("outside.dat"), b"outside").expect("write outside file");
        symlink(&outside, root.join("linked-dir")).expect("link directory");

        let discovered = discover_checked(&root).expect("discovery must succeed");
        assert_eq!(discovered, vec![root.join("inner/inside.dat")]);
    }
}
