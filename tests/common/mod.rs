/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Shared test support: temporary owned test sandboxes, exact source-file snapshots,
//! and source-tree snapshots.
//!
//! Each integration test links this module into its binary and uses
//! only the helpers it needs, so unused items are expected and allowed here.

// Shared across test binaries that each use only part of this module.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// An owned temporary directory for one test's filesystem trees.
///
/// Each sandbox creates a fresh sub-directory of the shared `$TMPDIR/scarab-tests/`
/// parent, named by process id and a process-local counter, and retries if
/// that exact name is already taken. The sandbox owns only the child it
/// created successfully. Callers derive every test path beneath
/// [`TestSandbox::path`]. Dropping the sandbox recursively removes that
/// child, best-effort, and never removes the shared parent.
pub struct TestSandbox {
    path: PathBuf,
}

impl TestSandbox {
    /// Creates a uniquely named, empty sandbox directory.
    pub fn new() -> Self {
        let parent = std::env::temp_dir().join("scarab-tests");
        fs::create_dir_all(&parent).unwrap_or_else(|error| {
            panic!(
                "failed to create sandbox parent {}: {error}",
                parent.display()
            )
        });

        static SERIAL: AtomicU64 = AtomicU64::new(0);
        loop {
            let serial = SERIAL.fetch_add(1, Ordering::Relaxed);
            let candidate = parent.join(format!("{}-{serial}", std::process::id()));
            match fs::create_dir(&candidate) {
                Ok(()) => return Self { path: candidate },
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("failed to create sandbox {}: {error}", candidate.display()),
            }
        }
    }

    /// The owned sandbox directory, which exists for the sandbox's lifetime.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Default for TestSandbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestSandbox {
    fn drop(&mut self) {
        // Best-effort removal of exactly the child created above. The shared
        // `$TMPDIR/scarab-tests/` parent is never removed.
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// An exact copy of a source file's bytes at capture time, used to assert
/// in all integration test cases that Scarab leaves the original file untouched.
pub struct SourceFileSnapshot {
    path: PathBuf,
    contents: Vec<u8>,
}

impl SourceFileSnapshot {
    /// Reads and retains the exact bytes of `path`.
    pub fn capture(path: impl Into<PathBuf>) -> Self {
        let path: PathBuf = path.into();
        let contents: Vec<u8> = fs::read(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        Self { path, contents }
    }

    /// The exact path the source file is captured from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Rereads the file and asserts its bytes still exactly match the original capture.
    pub fn assert_unchanged(&self) {
        let current = fs::read(&self.path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", self.path.display()));
        assert_eq!(
            current,
            self.contents,
            "source file {} changed",
            self.path.display()
        );
    }
}

/// An exact copy of every ordinary file under a directory tree at capture
/// time, keyed by root-relative path, used to assert in integration tests
/// that a walk leaves the source tree's ordinary files untouched.
///
/// Comparison covers ordinary-file membership and bytes.
/// It detects ordinary file additions, deletions, and byte modifications,
/// and it detects a captured ordinary file that disappears because it was
/// replaced by a symlink or other non-ordinary entry.
///
/// It does not detect changes that leave every ordinary file path and byte
/// unchanged.
///
/// Symlinks are skipped, matching discovery's no-follow behavior.
pub struct SourceTreeSnapshot {
    root: PathBuf,
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl SourceTreeSnapshot {
    /// Recursively captures every ordinary file under `root` with its exact
    /// bytes, keyed by root-relative path. The root must be an ordinary
    /// directory. Symlinked roots are rejected, including a terminal symlink
    /// spelled with trailing separators or `.` components.
    pub fn capture(root: impl Into<PathBuf>) -> Self {
        let root: PathBuf = root.into();
        ensure_ordinary_dir(&root);
        let mut files = BTreeMap::new();
        capture_tree(&root, &root, &mut files);
        Self { root, files }
    }

    /// The tree root the snapshot was captured from.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Asserts that the current ordinary-file set under the root exactly
    /// matches the capture: no ordinary file was added, deleted,
    /// byte-modified, or made to disappear by replacement with a symlink or
    /// other non-ordinary entry.
    pub fn assert_unchanged(&self) {
        ensure_ordinary_dir(&self.root);
        let mut current_files = BTreeMap::new();
        capture_tree(&self.root, &self.root, &mut current_files);

        // A captured ordinary file replaced by a symlink disappears from the
        // current capture and is therefore detected as deleted. Comparing
        // the full maps for equality covers all four change kinds.
        let missing: Vec<&PathBuf> = self
            .files
            .keys()
            .filter(|path| !current_files.contains_key(*path))
            .collect();
        let added: Vec<&PathBuf> = current_files
            .keys()
            .filter(|path| !self.files.contains_key(*path))
            .collect();
        let modified: Vec<&PathBuf> = self
            .files
            .iter()
            .filter_map(|(path, before)| match current_files.get(path) {
                Some(after) if after != before => Some(path),
                _ => None,
            })
            .collect();

        assert!(
            missing.is_empty() && added.is_empty() && modified.is_empty(),
            "source tree {} changed: deleted files {missing:?}, added files {added:?}, modified files {modified:?}",
            self.root.display(),
        );
    }
}

/// Panics unless `root` exists as an ordinary directory. Never follows a
/// symlink, as per discovery's root handling.
fn ensure_ordinary_dir(root: &Path) {
    // Match discovery's root check: POSIX resolution follows a terminal
    // symlink when the spelling ends in a separator or a `.` component, so
    // inspect a probe spelling with those stripped. The original spelling is
    // retained for traversal and reporting.
    let probe: PathBuf = root.components().collect();
    let metadata = fs::symlink_metadata(&probe).unwrap_or_else(|error| {
        panic!(
            "failed to inspect snapshot root {}: {error}",
            root.display()
        )
    });
    assert!(
        !metadata.is_symlink(),
        "snapshot root {} is a symlink",
        root.display()
    );
    assert!(
        metadata.is_dir(),
        "snapshot root {} is not a directory",
        root.display()
    );
}

/// Recursively saves, to memory, for later comparison, the byte contents
/// of all ordinary files under `tree_root`, keyed by their path relative
/// to `tree_root`, skipping symlinks.
fn capture_tree(tree_root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
    for entry in fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", dir.display()))
    {
        let entry = entry
            .unwrap_or_else(|error| panic!("failed to read entry in {}: {error}", dir.display()));
        let path = entry.path();
        // Not symlink_metadata: like discovery, file_type here never
        // traverses symlinks.
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!("failed to determine type of {}: {error}", path.display())
        });
        if file_type.is_dir() {
            capture_tree(tree_root, &path, files);
        } else if file_type.is_file() {
            let relative = path
                .strip_prefix(tree_root)
                .expect("traversal escaped tree root")
                .to_path_buf();
            let contents = fs::read(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            files.insert(relative, contents);
        }
    }
}
