/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Source-tree snapshots for unit-test mutation checks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// A captured entry category.
enum TreeEntry {
    /// A directory, including an empty one.
    Directory,
    /// A regular file and its bytes.
    File(Vec<u8>),
    /// A symlink or other special entry.
    NonOrdinary,
}

impl TreeEntry {
    /// Returns the category name used in diagnostics.
    fn kind(&self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File(_) => "ordinary file",
            Self::NonOrdinary => "non-ordinary entry",
        }
    }
}

/// Captures entries beneath a directory by relative path. Compares regular
/// files by bytes, directories by presence, and all other entries as one
/// category. Symlinks are recorded, not followed.
///
/// Ignores metadata and changes reverted before comparison. Concurrent changes
/// can be missed or disrupt the walk. The root must remain an ordinary
/// directory, not a symlink; it is checked but not captured.
pub(crate) struct SourceTreeSnapshot {
    root_path: PathBuf,
    entries: BTreeMap<PathBuf, TreeEntry>,
}

impl SourceTreeSnapshot {
    /// Captures a tree snapshot; panics if the root is invalid or unreadable.
    pub(crate) fn capture(root_path: impl Into<PathBuf>) -> Self {
        let root: PathBuf = root_path.into();
        ensure_ordinary_dir(&root);
        let entries = capture_tree(&root);
        Self {
            root_path: root,
            entries,
        }
    }

    /// Returns the original root path.
    pub(crate) fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Panics if the root is invalid, entries differ, or a read fails.
    pub(crate) fn assert_unchanged(&self) {
        ensure_ordinary_dir(&self.root_path);
        let current = capture_tree(&self.root_path);

        let deleted: Vec<&Path> = self
            .entries
            .keys()
            .filter(|path| !current.contains_key(*path))
            .map(PathBuf::as_path)
            .collect();
        let added: Vec<&Path> = current
            .keys()
            .filter(|path| !self.entries.contains_key(*path))
            .map(PathBuf::as_path)
            .collect();
        let modified: Vec<String> = self
            .entries
            .iter()
            .filter_map(|(path, before)| {
                let after = current.get(path)?;
                if same_entry(before, after) {
                    None
                } else {
                    Some(describe_change(path, before, after))
                }
            })
            .collect();

        assert!(
            deleted.is_empty() && added.is_empty() && modified.is_empty(),
            "source tree {} changed: deleted entries {deleted:?}, added entries {added:?}, modified entries {modified:?}",
            self.root_path.display(),
        );
    }
}

/// Compares two captures of the same path.
fn same_entry(before: &TreeEntry, after: &TreeEntry) -> bool {
    match (before, after) {
        (TreeEntry::Directory, TreeEntry::Directory) => true,
        (TreeEntry::File(before), TreeEntry::File(after)) => before == after,
        (TreeEntry::NonOrdinary, TreeEntry::NonOrdinary) => true,
        _ => false,
    }
}

/// Formats a change for diagnostics.
fn describe_change(path: &Path, before: &TreeEntry, after: &TreeEntry) -> String {
    match (before, after) {
        (TreeEntry::File(_), TreeEntry::File(_)) => {
            format!("{} (ordinary file bytes changed)", path.display())
        }
        _ => format!("{} ({} -> {})", path.display(), before.kind(), after.kind()),
    }
}

/// Captures the tree into a path-sorted map.
fn capture_tree(tree_root: &Path) -> BTreeMap<PathBuf, TreeEntry> {
    let mut entries = BTreeMap::new();
    capture_dir(tree_root, tree_root, &mut entries);
    entries
}

/// Recursively records entries beneath `dir`, relative to `tree_root`.
fn capture_dir(tree_root: &Path, dir: &Path, entries: &mut BTreeMap<PathBuf, TreeEntry>) {
    for entry in fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", dir.display()))
    {
        let entry = entry
            .unwrap_or_else(|error| panic!("failed to read entry in {}: {error}", dir.display()));
        let path = entry.path();
        let relative = path
            .strip_prefix(tree_root)
            .expect("traversal escaped tree root")
            .to_path_buf();
        // `file_type` reports the entry itself without following a symlink.
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!("failed to determine type of {}: {error}", path.display())
        });
        if file_type.is_dir() {
            entries.insert(relative, TreeEntry::Directory);
            capture_dir(tree_root, &path, entries);
        } else if file_type.is_file() {
            let contents = fs::read(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            entries.insert(relative, TreeEntry::File(contents));
        } else {
            entries.insert(relative, TreeEntry::NonOrdinary);
        }
    }
}

/// Panics unless `path` exists as an ordinary directory.
fn ensure_ordinary_dir(path: &Path) {
    // Strip trailing separators and `.` before checking; POSIX otherwise
    // follows a terminal symlink. Keep the original path for traversal.
    let probe: PathBuf = path.components().collect();
    let metadata = fs::symlink_metadata(&probe)
        .unwrap_or_else(|error| panic!("failed to inspect {}: {error}", path.display()));
    assert!(
        !metadata.is_symlink(),
        "{} is a symlink, expected an ordinary directory",
        path.display()
    );
    assert!(metadata.is_dir(), "{} is not a directory", path.display());
}

#[cfg(test)]
#[path = "../tests/test_support/snapshots.rs"]
mod tests;
