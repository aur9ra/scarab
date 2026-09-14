/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Source-file and source-tree snapshots used as immutability oracles.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A snapshot of a source file's exact bytes, used by integration tests to
/// assert that Scarab leaves the file unchanged.
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

/// A snapshot of every ordinary file under a directory tree, keyed by
/// root-relative path, used by integration tests to assert that an operation
/// leaves the tree's ordinary-file membership and contents unchanged.
///
/// Comparison covers ordinary-file membership and bytes.
/// It detects ordinary file additions, deletions, and byte modifications,
/// and it detects a captured ordinary file that disappears because it was
/// replaced by a symlink or other non-ordinary entry.
///
/// It does not detect changes that leave every ordinary file path and byte
/// unchanged (such as changes in access time in filesystem metadata)
///
/// Symlinks are skipped.
pub struct SourceTreeSnapshot {
    root_path: PathBuf,
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl SourceTreeSnapshot {
    /// Recursively captures every ordinary file under `root_path` with its exact
    /// bytes, keyed by root-relative path. The root must be an ordinary
    /// directory. Symlinked roots are rejected.
    pub fn capture(root_path: impl Into<PathBuf>) -> Self {
        let root: PathBuf = root_path.into();
        super::ensure_ordinary_dir(&root);
        let mut files = BTreeMap::new();
        capture_tree(&root, &root, &mut files);
        Self {
            root_path: root,
            files,
        }
    }

    /// The path of the tree root the snapshot was captured from.
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Asserts that the current ordinary-file set under the root exactly
    /// matches the capture: no ordinary file was added, deleted,
    /// byte-modified, or made to disappear by replacement with a symlink or
    /// other non-ordinary entry.
    pub fn assert_unchanged(&self) {
        super::ensure_ordinary_dir(&self.root_path);
        let mut current_files = BTreeMap::new();
        capture_tree(&self.root_path, &self.root_path, &mut current_files);

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
            self.root_path.display(),
        );
    }
}

/// Recursively captures ordinary-file bytes under `tree_root`, keyed by their
/// path relative to `tree_root`, skipping symlinks.
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
