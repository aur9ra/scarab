/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Temporary owned test sandboxes.

use std::path::Path;

/// An owned temporary directory for integration tests, created beneath
/// Cargo's test scratch directory and removed, best-effort, on normal drop.
///
/// Callers derive every test path beneath [`TempSandbox::path`]. There is no
/// cleanup guarantee after a crash or abort.
pub struct TempSandbox {
    inner: tempfile::TempDir,
}

impl TempSandbox {
    /// Creates a uniquely named, empty sandbox directory.
    ///
    /// Panics if the directory cannot be created, so a test fails loudly
    /// instead of reporting phantom filesystem errors.
    pub fn new() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("scarab-test-")
            .tempdir_in(env!("CARGO_TARGET_TMPDIR"))
            .unwrap_or_else(|error| {
                panic!(
                    "failed to create sandbox directory in {}: {error}",
                    env!("CARGO_TARGET_TMPDIR")
                )
            });
        Self { inner: dir }
    }

    /// The owned sandbox directory, which exists for the sandbox's lifetime.
    pub fn path(&self) -> &Path {
        self.inner.path()
    }
}

impl Default for TempSandbox {
    fn default() -> Self {
        Self::new()
    }
}
