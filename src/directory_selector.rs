/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Resolution of one explicit configured directory selector.
//!
//! [`resolve_directory_selector`] interprets one configured selector against one
//! source root, resolves the effective pathname through the native filesystem,
//! and verifies that a subsequent metadata lookup reports it as a directory.
//!
//! It does not iterate collection declarations, accumulate
//! failures across declarations, prepare default scopes for declarations without directory
//! selectors, associate files with scopes, interpret metadata, or decide collection membership.
//!
//! It reports only that canonicalization produced a
//! pathname and that a subsequent metadata lookup through that pathname
//! reported a directory. Continuing object identity, readability, containment
//! within the source root, and later pathname stability are not established.
//!
//! The configured selector, its effective pathname, and the resolved pathname
//! are distinct. Only the configured input and the resolved output cross the
//! API boundary. An absolute configured selector is used as spelled and
//! ignores the source root. A relative selector is anchored to the source root
//! and resolved through native platform semantics; on Windows, only selectors
//! and anchors with supported structural shapes are admitted.

use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

#[cfg(windows)]
use std::path::{Component, Prefix};

/// Resolves one configured directory selector against one
/// `source_root`.
///
/// A nonempty absolute `configured_directory` is resolved as spelled and never
/// inspects `source_root`.
/// A nonempty relative selector is anchored to `source_root` and resolved through
/// native platform pathname semantics, including symlinks and `..` components,
/// without Scarab-side normalization.
///
/// On Windows, a relative selector must be source-root-relative
/// and `source_root` must be a supported anchor.
///
/// On success, returns the pathname produced by [`fs::canonicalize`] after
/// confirming that [`fs::metadata`] reports it as a directory.
///
/// Returns [`io::Error`] unchanged for canonicalization and metadata errors.
/// A successfully inspected target that is not a directory is reported as
/// [`io::ErrorKind::NotADirectory`]. An empty selector and unsupported Windows
/// structural forms are reported as [`io::ErrorKind::InvalidInput`].
///
/// Successful resolution does not establish containment within `source_root`,
/// continuing object identity, readability, or later pathname stability.
pub fn resolve_directory_selector(
    source_root: &Path,
    configured_directory: &Path,
) -> io::Result<PathBuf> {
    let effective_path = effective_directory_selector_path(source_root, configured_directory)?;
    let resolved_path = fs::canonicalize(&effective_path)?;
    let metadata = fs::metadata(&resolved_path)?;

    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "resolved target is not a directory",
        ));
    }

    Ok(resolved_path)
}

/// Builds the effective pathname for one configured selector without touching
/// the filesystem.
///
/// Absolute configured selectors are used as spelled without inspecting the source root.
/// Relative configured selectors are joined to the source root.
fn effective_directory_selector_path(
    source_root: &Path,
    configured_directory: &Path,
) -> io::Result<PathBuf> {
    if configured_directory.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "directory selector is empty",
        ));
    }

    // An absolute selector (already checked for emptiness) is fully
    // self-describing, and bypasses anchor policy entirely.
    if configured_directory.is_absolute() {
        return Ok(configured_directory.to_path_buf());
    }

    // Windows distinguishes rooted, prefixed, and plain relative forms.
    // Elsewhere every non-absolute selector is source-root-relative and native
    // resolution applies to the joined result.
    #[cfg(windows)]
    {
        if configured_directory.has_root() || component_prefix(configured_directory).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "directory selector is not source-root-relative",
            ));
        }

        if !is_supported_anchor(source_root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "source root is not a supported anchor for a relative directory selector",
            ));
        }
    }

    Ok(source_root.join(configured_directory))
}

#[cfg(windows)]
/// The parsed prefix of `path`'s first component, if it has one.
fn component_prefix(path: &Path) -> Option<Prefix<'_>> {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => Some(prefix.kind()),
        _ => None,
    }
}

#[cfg(windows)]
/// Whether `source_root` can anchor a relative selector.
///
/// A verbatim prefix is never accepted. A non-verbatim prefix is accepted only
/// when the source root is fully absolute. A prefixless path is accepted only
/// when it has no root. An ordinary relative source root remains accepted and
/// therefore is dependent on the process CWD.
fn is_supported_anchor(source_root: &Path) -> bool {
    match component_prefix(source_root) {
        Some(prefix) => !prefix.is_verbatim() && source_root.is_absolute(),
        None => !source_root.has_root(),
    }
}

#[cfg(test)]
#[path = "tests/directory_selector.rs"]
mod tests;
