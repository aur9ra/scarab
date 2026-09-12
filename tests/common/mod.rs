/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Shared test support: exact source-file snapshots.

use std::fs;
use std::path::{Path, PathBuf};

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
