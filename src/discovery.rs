/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Discovery of ordinary (non-linked) source files under one root directory.
//!
//! [`discover_source_files`] recursively walks one supplied source root and
//! returns every file it contains. The walk is configuration-,
//! album-, metadata-, and probe-blind. It looks only at filesystem names and
//! entry types, never at file extensions or contents.
//!
//! Symlinks are never followed. Symlinked files are skipped, symlinked
//! directories are not traversed, and a symlink root is rejected. Paths are
//! returned exactly as constructed from the supplied root prefix. They are
//! never canonicalized, absolutized, or otherwise rewritten.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Recursively discovers every file under one source root.
///
/// The returned paths retain the supplied root prefix, e.g.
/// `music/Artist/Album/01 - Track`, and are sorted deterministically. Only
/// ordinary directories are traversed and only ordinary files are collected.
/// Symlinks and all other entry kinds are ignored.
///
/// The root must exist, be an ordinary directory, and not itself be a symlink.
/// A terminal symlink is rejected even when the supplied spelling ends in
/// separators or `.` components. Violations are reported as [`DiscoveryError`].
pub fn discover_source_files(root: &Path) -> Result<Vec<PathBuf>, DiscoveryError> {
    // Validate root directory.
    // POSIX resolution follows a terminal symlink when the spelling ends in a
    // separator or a `.` component, so inspecting `root` as spelled would miss
    // `link/` and `link/.`. We inspect a probe spelling with trailing separators
    // and `.` components stripped instead. Traversal and returned paths keep
    // the caller's original spelling.
    let probe: PathBuf = root.components().collect();
    let metadata = fs::symlink_metadata(&probe).map_err(|source| DiscoveryError::Root {
        path: root.to_path_buf(),
        source,
    })?;

    if metadata.is_symlink() {
        return Err(DiscoveryError::RootSymlink {
            path: root.to_path_buf(),
        });
    }
    if !metadata.is_dir() {
        return Err(DiscoveryError::NotADirectory {
            path: root.to_path_buf(),
        });
    }

    // collect files
    let mut files = Vec::new();
    walk(root, &mut files)?;
    files.sort();
    Ok(files)
}

/// Depth-first collection of file paths under `dir`, which must be
/// an ordinary directory (never a symlink). Any traversal I/O failure is
/// propagated. Partial results are never returned.
fn walk(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), DiscoveryError> {
    let mut entries = fs::read_dir(dir).map_err(|source| DiscoveryError::OpenDir {
        path: dir.to_path_buf(),
        source,
    })?;

    loop {
        let entry = match entries.next() {
            // catch errors from the OS's directory iterator
            Some(entry) => entry.map_err(|source| DiscoveryError::ReadEntries {
                path: dir.to_path_buf(),
                source,
            })?,
            None => return Ok(()),
        };

        let path = entry.path();
        // DirEntry::file_type does not traverse symlinks, so symlinked files
        // and directories report neither is_file nor is_dir here.
        let file_type = entry
            .file_type()
            .map_err(|source| DiscoveryError::FileType {
                path: path.clone(),
                source,
            })?;

        if file_type.is_dir() {
            walk(&path, files)?;
        } else if file_type.is_file() {
            files.push(path);
        }
    }
}

/// A failure to discover source files under a source root.
#[derive(Debug)]
pub enum DiscoveryError {
    /// The root path could not be inspected.
    Root {
        /// The root that failed inspection.
        path: PathBuf,
        source: io::Error,
    },
    /// The root path is a symlink. Symlink roots are rejected.
    RootSymlink {
        /// The symlinked root.
        path: PathBuf,
    },
    /// The root path exists but is not a directory.
    NotADirectory {
        /// The non-directory root.
        path: PathBuf,
    },
    /// A directory could not be opened for reading.
    OpenDir {
        /// The directory being opened.
        path: PathBuf,
        source: io::Error,
    },
    /// Iterating a directory's entries failed.
    ReadEntries {
        /// The directory being iterated.
        path: PathBuf,
        source: io::Error,
    },
    /// A directory entry's file type could not be determined.
    FileType {
        /// The entry whose file type could not be determined.
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscoveryError::Root { path, source } => {
                write!(f, "could not inspect root {}: {source}", path.display())
            }
            DiscoveryError::RootSymlink { path } => {
                write!(f, "root {} is a symlink", path.display())
            }
            DiscoveryError::NotADirectory { path } => {
                write!(f, "root {} is not a directory", path.display())
            }
            DiscoveryError::OpenDir { path, source } => {
                write!(f, "could not open directory {}: {source}", path.display())
            }
            DiscoveryError::ReadEntries { path, source } => {
                write!(
                    f,
                    "could not read entries of directory {}: {source}",
                    path.display()
                )
            }
            DiscoveryError::FileType { path, source } => {
                write!(
                    f,
                    "could not determine file type of {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for DiscoveryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            DiscoveryError::Root { source, .. }
            | DiscoveryError::OpenDir { source, .. }
            | DiscoveryError::ReadEntries { source, .. }
            | DiscoveryError::FileType { source, .. } => Some(source),
            DiscoveryError::RootSymlink { .. } | DiscoveryError::NotADirectory { .. } => None,
        }
    }
}
