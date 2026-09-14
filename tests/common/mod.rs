/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Shared test support: temporary owned test sandboxes, disposable fixture-tree
//! copies, and source-file and source-tree snapshots.
//!
//! Each integration test links this module into its binary and uses
//! only the helpers it needs, so unused items are expected and allowed here.

// Shared across test binaries that each use only part of this module, which
// also leaves parts of this façade's re-exports unused per binary.
#![allow(dead_code, unused_imports)]

mod fixtures;
mod snapshots;
mod temp_sandbox;

pub use fixtures::{copy_fixture_library, copy_fixture_tree};
pub use snapshots::{SourceFileSnapshot, SourceTreeSnapshot};
pub use temp_sandbox::TempSandbox;

use std::fs;
use std::path::{Path, PathBuf};

/// Panics unless `path` exists as an ordinary directory.
fn ensure_ordinary_dir(path: &Path) {
    // Match discovery's root check: POSIX resolution follows a terminal
    // symlink when the spelling ends in a separator or a `.` component, so
    // inspect a probe spelling with those stripped. Callers retain the
    // original spelling for traversal and reporting.
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
