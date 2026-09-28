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
mod tests {
    use super::*;
    use crate::test_support::TempSandbox;

    #[test]
    fn capture_accepts_an_empty_ordinary_tree() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir(&root).expect("create root");

        let snapshot = SourceTreeSnapshot::capture(&root);

        snapshot.assert_unchanged();
    }

    #[test]
    fn capture_accepts_nested_directories_files_and_empty_directories() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir_all(root.join("album/empty-subdir")).expect("create nested directories");
        fs::write(root.join("album/track.flac"), b"track bytes").expect("write track");
        fs::write(root.join("top.txt"), b"top").expect("write top-level file");

        let snapshot = SourceTreeSnapshot::capture(&root);

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "ordinary file bytes changed")]
    fn snapshot_detects_byte_modification() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir(&root).expect("create root");
        let file = root.join("track.flac");
        fs::write(&file, b"original bytes").expect("write file");

        let snapshot = SourceTreeSnapshot::capture(&root);
        fs::write(&file, b"modified bytes").expect("modify file");

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "added entries")]
    fn snapshot_detects_file_addition() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir(&root).expect("create root");

        let snapshot = SourceTreeSnapshot::capture(&root);
        fs::write(root.join("added.flac"), b"added").expect("add file");

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "deleted entries")]
    fn snapshot_detects_file_deletion() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir(&root).expect("create root");
        let file = root.join("track.flac");
        fs::write(&file, b"original bytes").expect("write file");

        let snapshot = SourceTreeSnapshot::capture(&root);
        fs::remove_file(&file).expect("delete file");

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "added entries")]
    fn snapshot_detects_empty_directory_addition() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir(&root).expect("create root");

        let snapshot = SourceTreeSnapshot::capture(&root);
        fs::create_dir(root.join("empty-album")).expect("add empty directory");

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "deleted entries")]
    fn snapshot_detects_empty_directory_deletion() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir(&root).expect("create root");
        fs::create_dir(root.join("empty-album")).expect("create empty directory");

        let snapshot = SourceTreeSnapshot::capture(&root);
        fs::remove_dir(root.join("empty-album")).expect("delete empty directory");

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "directory -> ordinary file")]
    fn snapshot_detects_directory_replaced_by_file() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir(&root).expect("create root");
        let replaced = root.join("album");
        fs::create_dir(&replaced).expect("create directory");

        let snapshot = SourceTreeSnapshot::capture(&root);
        fs::remove_dir(&replaced).expect("remove directory");
        fs::write(&replaced, b"not a directory").expect("write replacement file");

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "ordinary file -> directory")]
    fn snapshot_detects_file_replaced_by_directory() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir(&root).expect("create root");
        let replaced = root.join("track.flac");
        fs::write(&replaced, b"not a directory").expect("write file");

        let snapshot = SourceTreeSnapshot::capture(&root);
        fs::remove_file(&replaced).expect("remove file");
        fs::create_dir(&replaced).expect("create replacement directory");

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "ordinary file bytes changed")]
    fn snapshot_tracks_changes_using_trailing_separator_root_spelling() {
        let sandbox = TempSandbox::new();
        let real = sandbox.path().join("real");
        fs::create_dir(&real).expect("create real directory");
        let file = real.join("track.flac");
        fs::write(&file, b"original bytes").expect("write file");

        let mut spelling = real.as_os_str().to_os_string();
        spelling.push("/");
        let root = PathBuf::from(spelling);
        let snapshot = SourceTreeSnapshot::capture(root.clone());
        assert_eq!(
            snapshot.root_path().as_os_str(),
            root.as_os_str(),
            "supplied spelling must be retained exactly"
        );
        fs::write(&file, b"modified bytes").expect("modify file");

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "ordinary file bytes changed")]
    fn snapshot_tracks_changes_using_terminal_dot_root_spelling() {
        let sandbox = TempSandbox::new();
        let real = sandbox.path().join("real");
        fs::create_dir(&real).expect("create real directory");
        let file = real.join("track.flac");
        fs::write(&file, b"original bytes").expect("write file");

        let mut spelling = real.as_os_str().to_os_string();
        spelling.push("/.");
        let root = PathBuf::from(spelling);
        let snapshot = SourceTreeSnapshot::capture(root.clone());
        assert_eq!(
            snapshot.root_path().as_os_str(),
            root.as_os_str(),
            "supplied spelling must be retained exactly"
        );
        fs::write(&file, b"modified bytes").expect("modify file");

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "is not a directory")]
    fn capture_rejects_ordinary_file_root() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::write(&root, b"not a directory").expect("write file");

        SourceTreeSnapshot::capture(&root);
    }

    #[test]
    #[should_panic(expected = "failed to inspect")]
    fn capture_rejects_missing_root() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("missing");

        SourceTreeSnapshot::capture(&root);
    }

    #[test]
    #[should_panic(expected = "is not a directory")]
    fn comparison_rejects_root_replaced_by_ordinary_file() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir(&root).expect("create root");

        let snapshot = SourceTreeSnapshot::capture(&root);
        fs::remove_dir(&root).expect("remove root");
        fs::write(&root, b"not a directory").expect("write replacement file");

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "failed to inspect")]
    fn comparison_rejects_removed_root() {
        let sandbox = TempSandbox::new();
        let root = sandbox.path().join("tree");
        fs::create_dir(&root).expect("create root");

        let snapshot = SourceTreeSnapshot::capture(&root);
        fs::remove_dir(&root).expect("remove root");

        snapshot.assert_unchanged();
    }

    #[cfg(unix)]
    mod unix {
        use std::fs;
        use std::os::unix::fs::symlink;
        use std::path::PathBuf;

        use crate::test_support::{SourceTreeSnapshot, TempSandbox};

        #[test]
        #[should_panic(expected = "ordinary file -> non-ordinary entry")]
        fn snapshot_detects_file_replaced_by_byte_equivalent_symlink() {
            let sandbox = TempSandbox::new();
            let root = sandbox.path().join("tree");
            fs::create_dir(&root).expect("create root");
            let replaced = root.join("replaced.flac");
            let equivalent = root.join("equivalent.flac");
            fs::write(&replaced, b"identical bytes").expect("write replaced file");
            fs::write(&equivalent, b"identical bytes").expect("write equivalent file");

            let snapshot = SourceTreeSnapshot::capture(&root);

            fs::remove_file(&replaced).expect("remove replaced file");
            symlink(&equivalent, &replaced).expect("replace file with symlink");

            snapshot.assert_unchanged();
        }

        #[test]
        #[should_panic(expected = "directory -> non-ordinary entry")]
        fn snapshot_detects_directory_replaced_by_symlink() {
            let sandbox = TempSandbox::new();
            let root = sandbox.path().join("tree");
            fs::create_dir(&root).expect("create root");
            let replaced = root.join("album");
            let target = root.join("target");
            fs::create_dir(&replaced).expect("create replaced directory");
            fs::create_dir(&target).expect("create target directory");

            let snapshot = SourceTreeSnapshot::capture(&root);

            fs::remove_dir(&replaced).expect("remove replaced directory");
            symlink(&target, &replaced).expect("replace directory with symlink");

            snapshot.assert_unchanged();
        }

        #[test]
        #[should_panic(expected = "added entries")]
        fn snapshot_detects_symlink_addition() {
            let sandbox = TempSandbox::new();
            let root = sandbox.path().join("tree");
            fs::create_dir(&root).expect("create root");
            let target = root.join("target.flac");
            fs::write(&target, b"bytes").expect("write target");

            let snapshot = SourceTreeSnapshot::capture(&root);
            symlink(&target, root.join("added-link")).expect("add symlink");

            snapshot.assert_unchanged();
        }

        #[test]
        #[should_panic(expected = "deleted entries")]
        fn snapshot_detects_symlink_removal() {
            let sandbox = TempSandbox::new();
            let root = sandbox.path().join("tree");
            fs::create_dir(&root).expect("create root");
            let target = root.join("target.flac");
            fs::write(&target, b"bytes").expect("write target");
            let link = root.join("link");
            symlink(&target, &link).expect("add symlink");

            let snapshot = SourceTreeSnapshot::capture(&root);
            fs::remove_file(&link).expect("remove symlink");

            snapshot.assert_unchanged();
        }

        #[test]
        fn snapshot_ignores_contents_reachable_only_through_symlinks() {
            let sandbox = TempSandbox::new();
            let root = sandbox.path().join("tree");
            fs::create_dir(&root).expect("create root");
            let outside = sandbox.path().join("outside");
            fs::create_dir(&outside).expect("create outside directory");
            let outside_file = outside.join("outside.flac");
            fs::write(&outside_file, b"before").expect("write outside file");
            symlink(&outside_file, root.join("file-link")).expect("link to outside file");
            symlink(&outside, root.join("dir-link")).expect("link to outside directory");

            let snapshot = SourceTreeSnapshot::capture(&root);

            // Changes outside the tree aren't captured through symlinks.
            fs::write(&outside_file, b"after").expect("modify outside file");
            fs::write(outside.join("added.flac"), b"added").expect("add outside file");

            snapshot.assert_unchanged();
        }

        #[test]
        #[should_panic(expected = "ordinary file bytes changed")]
        fn snapshot_still_reads_ordinary_file_that_is_also_a_symlink_target() {
            let sandbox = TempSandbox::new();
            let root = sandbox.path().join("tree");
            fs::create_dir(&root).expect("create root");
            let target = root.join("target.flac");
            fs::write(&target, b"before").expect("write target");
            symlink(&target, root.join("link")).expect("link to target");

            let snapshot = SourceTreeSnapshot::capture(&root);
            fs::write(&target, b"after").expect("modify target");

            snapshot.assert_unchanged();
        }

        #[test]
        #[should_panic(expected = "is a symlink")]
        fn capture_rejects_symlink_root_with_trailing_separator() {
            let sandbox = TempSandbox::new();
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
        fn capture_rejects_symlink_root_with_terminal_dot() {
            let sandbox = TempSandbox::new();
            let real = sandbox.path().join("real");
            fs::create_dir(&real).expect("create real directory");
            let link = sandbox.path().join("link");
            symlink(&real, &link).expect("create root symlink");

            let mut spelling = link.as_os_str().to_os_string();
            spelling.push("/.");
            SourceTreeSnapshot::capture(PathBuf::from(spelling));
        }

        #[test]
        #[should_panic(expected = "is a symlink")]
        fn comparison_rejects_root_replaced_by_symlink() {
            let sandbox = TempSandbox::new();
            let root = sandbox.path().join("tree");
            fs::create_dir(&root).expect("create root");
            // The empty target leaves the root symlink check as the only distinction.
            let target = sandbox.path().join("target");
            fs::create_dir(&target).expect("create target directory");

            let snapshot = SourceTreeSnapshot::capture(&root);
            fs::remove_dir(&root).expect("remove root");
            symlink(&target, &root).expect("replace root with symlink");

            snapshot.assert_unchanged();
        }
    }
}
