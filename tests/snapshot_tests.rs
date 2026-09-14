/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Hostile tests proving the snapshot oracles detect the source changes they
//! claim to detect, plus root-symlink rejection cases.
//!
//! Every tree here is synthetic and lives inside a [`TempSandbox`], so no
//! committed fixture is ever mutated.

use std::fs;

use common::{SourceFileSnapshot, SourceTreeSnapshot, TempSandbox};

mod common;

#[test]
#[should_panic(expected = "changed")]
fn source_file_snapshot_detects_byte_change() {
    let sandbox = TempSandbox::new();
    let file = sandbox.path().join("track.flac");
    fs::write(&file, b"original bytes").expect("write file");

    let snapshot = SourceFileSnapshot::capture(&file);
    fs::write(&file, b"modified bytes").expect("modify file");

    snapshot.assert_unchanged();
}

#[test]
#[should_panic(expected = "changed")]
fn source_tree_snapshot_detects_byte_modification() {
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
#[should_panic(expected = "added files")]
fn source_tree_snapshot_detects_file_addition() {
    let sandbox = TempSandbox::new();
    let root = sandbox.path().join("tree");
    fs::create_dir(&root).expect("create root");

    let snapshot = SourceTreeSnapshot::capture(&root);
    fs::write(root.join("added.flac"), b"added").expect("add file");

    snapshot.assert_unchanged();
}

#[test]
#[should_panic(expected = "deleted files")]
fn source_tree_snapshot_detects_file_deletion() {
    let sandbox = TempSandbox::new();
    let root = sandbox.path().join("tree");
    fs::create_dir(&root).expect("create root");
    let file = root.join("track.flac");
    fs::write(&file, b"original bytes").expect("write file");

    let snapshot = SourceTreeSnapshot::capture(&root);
    fs::remove_file(&file).expect("delete file");

    snapshot.assert_unchanged();
}

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    use super::{SourceTreeSnapshot, TempSandbox};

    #[test]
    #[should_panic(expected = "deleted files")]
    fn source_tree_snapshot_detects_file_replaced_by_byte_equivalent_symlink() {
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
        assert!(
            fs::symlink_metadata(&replaced)
                .expect("inspect replacement")
                .is_symlink(),
            "replacement must be a symlink"
        );

        snapshot.assert_unchanged();
    }

    #[test]
    #[should_panic(expected = "is a symlink")]
    fn tree_snapshot_capture_rejects_symlink_root_with_trailing_separator() {
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
    fn tree_snapshot_capture_rejects_symlink_root_with_terminal_dot() {
        let sandbox = TempSandbox::new();
        let real = sandbox.path().join("real");
        fs::create_dir(&real).expect("create real directory");
        let link = sandbox.path().join("link");
        symlink(&real, &link).expect("create root symlink");

        let mut spelling = link.as_os_str().to_os_string();
        spelling.push("/.");
        SourceTreeSnapshot::capture(PathBuf::from(spelling));
    }
}
