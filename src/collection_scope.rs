/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Prepares filesystem scopes on behalf of collection declarations.
//!
//! Relative directory selectors use the supplied source root. Absolute
//! selectors ignore it. Selectors are grouped by resolved path within each
//! collection declaration. Multiple selectors for one path produce one warning per
//! group. Declarations without directory selectors (such as those with
//! metadata selectors) share one prepared source-root scope.
//!
//! Relative selectors always use the original source root, not the prepared
//! default root. Configured scopes from separate declarations remain separate,
//! as do configured scopes and the default scope. Preparation returns scopes
//! only if every required resolution succeeds, otherwise it returns failures
//! and any warnings from successful groups.
//!
//! This module only inspects paths. It does not modify the source tree, find
//! files, or decide collection membership.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::config::LibraryBuildSpec;
use crate::directory_selector::resolve_directory_selector;

/// Result of preparing filesystem scopes for collection declarations.
#[derive(Debug)]
pub(crate) enum CollectionScopePreparation {
    /// Every required scope was prepared.
    Prepared {
        /// The prepared scopes.
        scopes: PreparedCollectionScopes,
        /// Redundancy warnings from successful configured groups.
        warnings: Vec<RedundancyWarning>,
    },
    /// One or more required scopes failed to prepare.
    Failed {
        /// Selector failures in declaration and selector order; duplicates stay separate.
        configured_failures: Vec<ConfiguredSelectorFailure>,
        /// Failure to prepare the shared default root, if needed.
        default_source_root_failure: Option<DefaultSourceRootFailure>,
        /// Redundancy warnings from successful configured groups.
        warnings: Vec<RedundancyWarning>,
    },
}

/// Scopes from a successful collection-scope preparation.
#[derive(Debug)]
pub(crate) struct PreparedCollectionScopes {
    /// Configured scopes in declaration and first-selector order.
    pub(crate) configured_directory_scopes: Vec<ConfiguredDirectoryScope>,
    /// Shared by declarations without configured directory selectors, if any.
    pub(crate) default_source_root_scope: Option<DefaultSourceRootScope>,
}

/// One group of directory selectors in a collection declaration that resolved to
/// the same path.
#[derive(Debug)]
pub(crate) struct ConfiguredDirectoryScope {
    /// Handle of the collection declaration that requires this scope.
    pub(crate) collection_handle: String,
    /// Resolved path shared by this group.
    pub(crate) resolved_directory: PathBuf,
    /// Selector spellings in declaration order, including duplicates.
    pub(crate) contributing_selectors: Vec<PathBuf>,
}

/// One configured directory-selector occurrence that failed to resolve.
#[derive(Debug)]
pub(crate) struct ConfiguredSelectorFailure {
    /// Handle of the collection declaration containing the failed selector.
    pub(crate) collection_handle: String,
    /// Directory-selector spelling from the configuration.
    pub(crate) configured_selector: PathBuf,
    /// Error returned by the resolver.
    pub(crate) error: io::Error,
}

/// One source-root scope shared by declarations without directory selectors.
#[derive(Debug)]
pub(crate) struct DefaultSourceRootScope {
    /// Source-root spelling supplied by the caller.
    pub(crate) original_source_root: PathBuf,
    /// Resolved path for later filesystem work.
    pub(crate) resolved_traversal_root: PathBuf,
    /// Handles of collection declarations without directory selectors that require this scope, in
    /// declaration order.
    pub(crate) dependent_collection_handles: Vec<String>,
}

/// Failure to prepare the shared default source root.
#[derive(Debug)]
pub(crate) struct DefaultSourceRootFailure {
    /// Source-root spelling supplied by the caller.
    pub(crate) original_source_root: PathBuf,
    /// Handles of collection declarations without directory selectors that require this scope, in
    /// declaration order.
    pub(crate) dependent_collection_handles: Vec<String>,
    /// Reason preparation failed.
    pub(crate) kind: DefaultSourceRootFailureKind,
}

/// Why preparing the default source root failed.
#[derive(Debug)]
pub(crate) enum DefaultSourceRootFailureKind {
    /// The supplied root is empty. Whitespace is not empty.
    EmptyInput,
    /// The path form is not allowed on this host.
    UnsupportedPathForm,
    /// Resolving the supplied root failed.
    ResolutionFailed {
        /// Filesystem error from resolution.
        error: io::Error,
    },
    /// Inspecting the resolved path failed.
    ResolvedPathInspectionFailed {
        /// Path that could not be inspected.
        resolved_path: PathBuf,
        /// Filesystem error from inspection.
        error: io::Error,
    },
    /// The resolved path is not a directory.
    ResolvedTargetNotDirectory {
        /// Path that is not a directory.
        resolved_path: PathBuf,
    },
}

/// Warning that several directory selectors in a collection declaration resolved
/// to the same path.
#[derive(Debug)]
pub(crate) struct RedundancyWarning {
    /// Handle of the collection declaration containing redundant selectors.
    pub(crate) collection_handle: String,
    /// Shared resolved path.
    pub(crate) resolved_directory: PathBuf,
    /// Selector spellings in declaration order, including duplicates.
    pub(crate) contributing_selectors: Vec<PathBuf>,
}

/// Prepares filesystem scopes on behalf of collection declarations in `spec`.
///
/// Each configured directory selector is resolved independently, in declaration and
/// selector order. Relative selectors use the supplied root, absolute selectors
/// ignore it. Selectors that resolve to the same path are grouped within their
/// declaration. Groups with multiple selectors produce one warning. Failures
/// are collected and do not stop preparation.
///
/// If any collection declaration has no configured directory selector, the root is
/// resolved once and shared by those declarations.
///
/// Configured directory selectors and the default root are checked even if one fails.
/// Scopes are returned only if all required resolutions succeed, otherwise the
/// result contains the failures and any warnings from successful groups.
pub(crate) fn prepare_collection_scopes(
    spec: &LibraryBuildSpec,
    source_root: &Path,
) -> CollectionScopePreparation {
    let mut configured_directory_scopes: Vec<ConfiguredDirectoryScope> = Vec::new();
    let mut configured_failures: Vec<ConfiguredSelectorFailure> = Vec::new();
    let mut warnings: Vec<RedundancyWarning> = Vec::new();

    // Groups belong to one declaration; equal paths in other declarations stay separate.
    for (collection_handle, declaration) in spec.collection_declarations() {
        let Some(selectors) = declaration.directories.as_deref() else {
            continue;
        };

        let mut declaration_scopes: Vec<ConfiguredDirectoryScope> = Vec::new();
        for selector in selectors {
            // Resolve every occurrence independently, including duplicates.
            match resolve_directory_selector(source_root, selector) {
                Ok(resolved_directory) => {
                    // group this selector with earlier selectors in this declaration that resolved to the same directory
                    match declaration_scopes
                        .iter_mut()
                        .find(|scope| scope.resolved_directory == resolved_directory)
                    {
                        Some(scope) => scope.contributing_selectors.push(selector.clone()),
                        None => declaration_scopes.push(ConfiguredDirectoryScope {
                            collection_handle: collection_handle.to_owned(),
                            resolved_directory,
                            contributing_selectors: vec![selector.clone()],
                        }),
                    }
                }
                // Accumulate errors and continue
                Err(error) => configured_failures.push(ConfiguredSelectorFailure {
                    collection_handle: collection_handle.to_owned(),
                    configured_selector: selector.clone(),
                    error,
                }),
            }
        }

        for scope in &declaration_scopes {
            if scope.contributing_selectors.len() >= 2 {
                warnings.push(RedundancyWarning {
                    collection_handle: scope.collection_handle.clone(),
                    resolved_directory: scope.resolved_directory.clone(),
                    contributing_selectors: scope.contributing_selectors.clone(),
                });
            }
        }
        configured_directory_scopes.extend(declaration_scopes);
    }

    let dependent_collection_handles: Vec<String> = spec
        .collection_declarations()
        .filter(|(_, declaration)| declaration.directories.is_none())
        .map(|(collection_handle, _)| collection_handle.to_owned())
        .collect();

    // Only prepare a default root when at least one declaration needs it.
    let mut default_source_root_scope = None;
    let mut default_source_root_failure = None;
    if !dependent_collection_handles.is_empty() {
        match prepare_default_source_root(source_root, &dependent_collection_handles) {
            Ok(scope) => default_source_root_scope = Some(scope),
            Err(kind) => {
                default_source_root_failure = Some(DefaultSourceRootFailure {
                    original_source_root: source_root.to_path_buf(),
                    dependent_collection_handles,
                    kind,
                });
            }
        }
    }

    if configured_failures.is_empty() && default_source_root_failure.is_none() {
        CollectionScopePreparation::Prepared {
            scopes: PreparedCollectionScopes {
                configured_directory_scopes,
                default_source_root_scope,
            },
            warnings,
        }
    } else {
        CollectionScopePreparation::Failed {
            configured_failures,
            default_source_root_failure,
            warnings,
        }
    }
}

/// Resolves and checks the shared source root for declarations that need it.
///
/// The scope keeps both the caller's spelling and the resolved directory.
fn prepare_default_source_root(
    source_root: &Path,
    dependent_collection_handles: &[String],
) -> Result<DefaultSourceRootScope, DefaultSourceRootFailureKind> {
    if source_root.as_os_str().is_empty() {
        return Err(DefaultSourceRootFailureKind::EmptyInput);
    }
    if !default_root_form_supported(source_root) {
        return Err(DefaultSourceRootFailureKind::UnsupportedPathForm);
    }

    let resolved_traversal_root = fs::canonicalize(source_root)
        .map_err(|error| DefaultSourceRootFailureKind::ResolutionFailed { error })?;

    let metadata = fs::metadata(&resolved_traversal_root).map_err(|error| {
        DefaultSourceRootFailureKind::ResolvedPathInspectionFailed {
            resolved_path: resolved_traversal_root.clone(),
            error,
        }
    })?;
    if !metadata.is_dir() {
        return Err(DefaultSourceRootFailureKind::ResolvedTargetNotDirectory {
            resolved_path: resolved_traversal_root,
        });
    }

    Ok(DefaultSourceRootScope {
        original_source_root: source_root.to_path_buf(),
        resolved_traversal_root,
        dependent_collection_handles: dependent_collection_handles.to_vec(),
    })
}

/// Whether a non-empty default source root has a path form admitted on this host.
///
/// Empty input is handled separately by `prepare_default_source_root` so it
/// can produce `EmptyInput` rather than `UnsupportedPathForm`.
///
/// On non-Windows hosts, Scarab imposes no additional path-form restrictions.
#[cfg(not(windows))]
fn default_root_form_supported(root: &Path) -> bool {
    // unix is so nice. windows - you test me
    debug_assert!(!root.as_os_str().is_empty());
    true
}

/// Whether a non-empty `root` is an allowed Windows default source root.
///
/// Empty input is handled separately by `prepare_default_source_root` so it
/// can produce `EmptyInput` rather than `UnsupportedPathForm`.
///
/// Accepts ordinary relative paths, absolute drive and UNC paths, and rooted
/// verbatim drive and UNC paths. Rejects drive-relative and current-drive-rooted
/// paths, device namespace paths, and generic verbatim paths.
#[cfg(windows)]
fn default_root_form_supported(root: &Path) -> bool {
    use std::path::{Component, Prefix};

    let mut components = root.components();
    match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(_) | Prefix::UNC(..) => root.is_absolute(),
            // Require the root component, reject a bare `\\?\C:`.
            Prefix::VerbatimDisk(_) => matches!(components.next(), Some(Component::RootDir)),
            Prefix::VerbatimUNC(..) => true,
            Prefix::DeviceNS(_) | Prefix::Verbatim(_) => false,
        },
        // Reject paths rooted on the current drive.
        Some(Component::RootDir) => false,
        _ => true,
    }
}

#[cfg(test)]
#[path = "tests/collection_scope.rs"]
mod tests;
