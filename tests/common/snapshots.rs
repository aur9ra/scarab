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

/// How one entry under a captured tree root was observed.
enum TreeEntry {
    /// An ordinary directory, including empty ones.
    Directory,
    /// An ordinary file and its captured bytes.
    File(Vec<u8>),
    /// "Catch-all" for other entry kinds, recorded by presence in one shared category.
    NonOrdinary,
}

impl TreeEntry {
    /// Diagnostic label for this category.
    fn kind(&self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File(_) => "ordinary file",
            Self::NonOrdinary => "non-ordinary entry",
        }
    }
}

/// Captures entries beneath a directory, keyed by relative path. Ordinary
/// files are compared by bytes, directories by presence, and all other entry
/// kinds as one category. In a stable tree, symlinks are recorded but not
/// followed.
///
/// Detects additions, removals, file byte-content changes, and category changes.
///
/// It ignores metadata, replacements that preserve the captured data, and
/// changes reachable only through symlinks. Changes restored before comparison
/// are also invisible. The walk isn't atomic, so concurrent changes may be
/// missed or redirect later reads or traversal. The root must be an ordinary
/// directory at capture and comparison; it is checked but not included.
pub struct SourceTreeSnapshot {
    root_path: PathBuf,
    entries: BTreeMap<PathBuf, TreeEntry>,
}

impl SourceTreeSnapshot {
    /// Recursively captures entries under an ordinary directory. Panics if
    /// the root is invalid or traversal or file reads fail.
    pub fn capture(root_path: impl Into<PathBuf>) -> Self {
        let root: PathBuf = root_path.into();
        super::ensure_ordinary_dir(&root);
        let entries = capture_tree(&root);
        Self {
            root_path: root,
            entries,
        }
    }

    /// The path of the tree root the snapshot was captured from.
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Asserts that the root remains an ordinary directory and all entries
    /// match the capture.
    pub fn assert_unchanged(&self) {
        super::ensure_ordinary_dir(&self.root_path);
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

/// Whether two captures of the same pathname observed an equal entry.
fn same_entry(before: &TreeEntry, after: &TreeEntry) -> bool {
    match (before, after) {
        (TreeEntry::Directory, TreeEntry::Directory) => true,
        (TreeEntry::File(before), TreeEntry::File(after)) => before == after,
        (TreeEntry::NonOrdinary, TreeEntry::NonOrdinary) => true,
        _ => false,
    }
}

/// Describes why two captures of the same pathname differ.
fn describe_change(path: &Path, before: &TreeEntry, after: &TreeEntry) -> String {
    match (before, after) {
        (TreeEntry::File(_), TreeEntry::File(_)) => {
            format!("{} (ordinary file bytes changed)", path.display())
        }
        _ => format!("{} ({} -> {})", path.display(), before.kind(), after.kind()),
    }
}

/// Captures every entry under `tree_root` into a fresh map.
fn capture_tree(tree_root: &Path) -> BTreeMap<PathBuf, TreeEntry> {
    let mut entries = BTreeMap::new();
    capture_dir(tree_root, tree_root, &mut entries);
    entries
}

/// Captures entries beneath `dir`, keyed relative to `tree_root`. It descends
/// into ordinary directories, reads ordinary files, and records other kinds by
/// presence.
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
