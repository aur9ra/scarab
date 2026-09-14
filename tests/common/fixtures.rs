/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Disposable copies of the committed fixture library.

use std::fs;
use std::io;
use std::path::Path;

/// Committed fixture library root, relative to the crate root that
/// cargo test runs in.
const COMMITTED_FIXTURES_LIBRARY: &str = "tests/fixtures/library";

/// Copies the committed fixture library to `destination`.
///
/// `destination` must not exist. Delegates to [`copy_fixture_tree`].
/// The caller chooses and owns the destination and its lifetime.
pub fn copy_fixture_library(destination: &Path) {
    copy_fixture_tree(Path::new(COMMITTED_FIXTURES_LIBRARY), destination);
}

/// Recursively copies an ordinary directory tree to a fresh destination path.
///
/// Exact byte-for-byte file contents are preserved. Filesystem metadata
/// not present within these bytes are not preserved.
///
/// `source` must be an ordinary directory and `destination` must not exist.
/// The destination's existing parent directory must resolve outside the
/// source directory's physical tree.
///
/// Panics on symlinks and other non-ordinary source entries.
pub fn copy_fixture_tree(source: &Path, destination: &Path) {
    super::ensure_ordinary_dir(source);
    ensure_absent(destination);
    ensure_destination_outside_source(source, destination);
    fs::create_dir(destination).unwrap_or_else(|error| {
        panic!(
            "failed to create copy destination {}: {error}",
            destination.display()
        )
    });
    copy_tree(source, destination);
}

/// Panics if creating the non-existent `destination` would place it inside `source`.
///
/// `destination` must not exist, and its immediate parent must already exist.
///
/// The existing parent is canonicalized as to check containment against its
/// physical filesystem location before `destination` is created.
///
/// This is a test-helper precondition, not a filesystem-security boundary.
fn ensure_destination_outside_source(source: &Path, destination: &Path) {
    let source_physical = fs::canonicalize(source).unwrap_or_else(|error| {
        panic!(
            "failed to resolve copy source {}: {error}",
            source.display()
        )
    });

    // see destination's parent, destination doesn't exist
    let destination_parent = match destination.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    let destination_parent_physical =
        fs::canonicalize(destination_parent).unwrap_or_else(|error| {
            panic!(
                "failed to resolve copy destination parent {}: {error}",
                destination_parent.display()
            )
        });

    assert!(
        !destination_parent_physical.starts_with(&source_physical),
        "destination parent {} is inside source {}",
        destination_parent.display(),
        source.display()
    );
}

/// Panics unless `path` does not exist. Uses `symlink_metadata`, so a
/// symlink (even a dangling symlink) also counts as existing.
fn ensure_absent(path: &Path) {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {} // optimal case
        Err(error) => panic!("failed to inspect {}: {error}", path.display()),
        Ok(_) => panic!("{} already exists", path.display()),
    }
}

/// Recursively copies ordinary entries under `source_dir` into
/// an existing `destination_dir`. Child destination directories
/// are created before recursion.
///
/// Symlinks and special entries panic.
fn copy_tree(source_dir: &Path, destination_dir: &Path) {
    for entry in fs::read_dir(source_dir).unwrap_or_else(|error| {
        panic!(
            "failed to read source directory {}: {error}",
            source_dir.display()
        )
    }) {
        let entry = entry.unwrap_or_else(|error| {
            panic!(
                "failed to read entry in source directory {}: {error}",
                source_dir.display()
            )
        });
        let source_path = entry.path();
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!(
                "failed to determine type of entry at {}: {error}",
                source_path.display()
            )
        });
        let destination_path = destination_dir.join(entry.file_name());

        if file_type.is_dir() {
            fs::create_dir(&destination_path).unwrap_or_else(|error| {
                panic!(
                    "failed to create directory {}: {error}",
                    destination_path.display()
                )
            });
            copy_tree(&source_path, &destination_path);
        } else if file_type.is_file() {
            let contents = fs::read(&source_path).unwrap_or_else(|error| {
                panic!("failed to read {}: {error}", source_path.display())
            });
            fs::write(&destination_path, contents).unwrap_or_else(|error| {
                panic!("failed to write {}: {error}", destination_path.display())
            });
        } else {
            panic!(
                "cannot copy non-ordinary entry {}: symlinks and special files are not supported",
                source_path.display()
            );
        }
    }
}
