/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Temporary owned test sandboxes.

use std::path::Path;

/// An owned temporary directory for integration tests.
///
/// Created under the system temporary directory with a `scarab-test-` prefix.
/// Drop attempts cleanup, but open handles or abnormal process exit can leave
/// residue.
///
/// Keep test paths beneath [`TempSandbox::path`].
pub struct TempSandbox {
    inner: tempfile::TempDir,
}

impl TempSandbox {
    /// Creates a unique, empty sandbox directory.
    ///
    /// On Unix, requests mode 0700 (subject to umask). Panics if creation fails.
    pub fn new() -> Self {
        let mut builder = tempfile::Builder::new();
        builder.prefix("scarab-test-");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Restrict the sandbox root to the current user.
            // Temporary directories may be shared between users/applications.
            builder.permissions(std::fs::Permissions::from_mode(0o700));
        }
        let inner = builder.tempdir().unwrap_or_else(|error| {
            panic!(
                "failed to create test sandbox in {}: {error}",
                std::env::temp_dir().display()
            )
        });
        Self { inner }
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
