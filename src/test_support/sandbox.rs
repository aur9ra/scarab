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
mod tests {
    use super::*;
    use std::fs;
    use std::io;
    use std::path::Path;
    use std::sync::OnceLock;

    /// Panics if `path` exists or cannot be inspected.
    fn assert_absent(path: &Path) {
        match fs::symlink_metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => panic!("failed to inspect {}: {error}", path.display()),
            Ok(_) => panic!("expected {} to be absent, but it exists", path.display()),
        }
    }

    #[test]
    fn sandbox_is_created_in_the_selected_temporary_location_with_its_prefix() {
        let sandbox = TempSandbox::new();
        let temporary_location = std::env::temp_dir();

        assert!(
            sandbox.path().starts_with(&temporary_location),
            "sandbox {} must be created under {}",
            sandbox.path().display(),
            temporary_location.display()
        );
        let name = sandbox
            .path()
            .file_name()
            .expect("sandbox path must have a file name");
        assert!(
            name.to_string_lossy().starts_with("scarab-test-"),
            "sandbox name {name:?} must start with the scarab-test- prefix"
        );
        assert!(
            sandbox.path().is_dir(),
            "sandbox {} must exist as a directory",
            sandbox.path().display()
        );
    }

    #[test]
    fn simultaneously_live_sandboxes_are_distinct() {
        let first = TempSandbox::default();
        let second = TempSandbox::new();

        assert_ne!(first.path(), second.path());
        assert!(first.path().is_dir());
        assert!(second.path().is_dir());
    }

    #[test]
    fn dropping_the_owner_removes_nested_files_and_directories() {
        let path = {
            let sandbox = TempSandbox::new();
            let nested = sandbox.path().join("album/nested");
            fs::create_dir_all(&nested).expect("create nested directories");
            fs::write(nested.join("track.flac"), b"track bytes").expect("write nested file");
            sandbox.path().to_path_buf()
        };

        assert_absent(&path);
    }

    #[test]
    fn dropping_the_owner_during_unwinding_removes_nested_contents() {
        let path_slot = OnceLock::new();
        let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let sandbox = TempSandbox::new();
            path_slot
                .set(sandbox.path().to_path_buf())
                .expect("record the sandbox path");
            fs::create_dir(sandbox.path().join("nested")).expect("create nested directory");
            panic!("unwind while the sandbox owner is live");
        }));

        assert!(unwound.is_err(), "the scope must unwind");
        let path = path_slot
            .get()
            .expect("the unwinding scope must record the sandbox path");
        assert_absent(path);
    }

    #[test]
    fn dropping_one_owner_preserves_the_other_owners_sentinel() {
        let first = TempSandbox::new();
        let second = TempSandbox::new();
        let first_path = first.path().to_path_buf();
        let sentinel = second.path().join("sentinel.txt");
        fs::write(&sentinel, b"sentinel bytes").expect("write sentinel");

        drop(first);

        assert_absent(&first_path);
        assert_eq!(
            fs::read(&sentinel).expect("read the surviving sentinel"),
            b"sentinel bytes"
        );
    }

    #[cfg(unix)]
    #[test]
    fn sandbox_permissions_exclude_group_and_other_access() {
        use std::os::unix::fs::PermissionsExt;

        let sandbox = TempSandbox::new();
        let mode = fs::metadata(sandbox.path())
            .expect("inspect the sandbox root")
            .permissions()
            .mode();

        assert_eq!(
            mode & 0o077,
            0,
            "sandbox mode {mode:o} must exclude group and other access"
        );
    }
}
