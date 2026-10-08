/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Owned temporary directories for unit tests.

use std::path::Path;

/// An owned temporary directory for unit tests.
///
/// Cleanup is delegated to [`tempfile::TempDir`]; there is no custom destructor.
pub(crate) struct TempSandbox {
    inner: tempfile::TempDir,
}

impl TempSandbox {
    /// Creates a unique `scarab-test-` directory in the system temporary location.
    ///
    /// On Unix, requests mode 0700 (subject to umask). Panics if creation fails.
    pub(crate) fn new() -> Self {
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
    pub(crate) fn path(&self) -> &Path {
        self.inner.path()
    }
}

impl Default for TempSandbox {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../tests/test_support/sandbox.rs"]
mod tests;
