/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Warnings and failures for observed source file inventory construction.

use std::fmt;
use std::io;
use std::path::PathBuf;

use crate::discovery::DiscoveryError;

use super::{
    RequiredScope, required_scope_from_configured_directory,
    required_scope_from_default_source_root,
};

/// Configured directory selectors in one collection declaration that resolve to
/// the same directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedundantConfiguredSelectors {
    /// Handle of the collection declaration containing redundant selectors.
    pub collection_handle: String,
    /// Shared resolved path.
    pub resolved_directory: PathBuf,
    /// Selector spellings in declaration order, including duplicates.
    pub contributing_selectors: Vec<PathBuf>,
}

/// Observed source file inventory failure.
#[derive(Debug)]
pub enum ObservedSourceFileInventoryFailure {
    /// Preparation failed; discovery did not run.
    Preparation {
        /// Directory-selector failures in declaration and selector order.
        configured_failures: Vec<ConfiguredSelectorFailure>,
        /// Failure of the shared default root, if needed.
        default_source_root_failure: Option<DefaultSourceRootFailure>,
        /// Warnings from successful configured groups.
        warnings: Vec<RedundantConfiguredSelectors>,
    },
    /// Discovery failed; no partial inventory is returned.
    Discovery {
        /// Failed scopes and their descriptions.
        failures: Vec<RequiredScopeDiscoveryFailure>,
        /// Preparation warnings.
        warnings: Vec<RedundantConfiguredSelectors>,
    },
}

impl ObservedSourceFileInventoryFailure {
    /// Preparation warnings retained by the failure.
    pub fn warnings(&self) -> &[RedundantConfiguredSelectors] {
        match self {
            ObservedSourceFileInventoryFailure::Preparation { warnings, .. }
            | ObservedSourceFileInventoryFailure::Discovery { warnings, .. } => warnings,
        }
    }
}

impl fmt::Display for ObservedSourceFileInventoryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObservedSourceFileInventoryFailure::Preparation {
                configured_failures,
                default_source_root_failure,
                ..
            } => {
                write!(
                    f,
                    "observed source file inventory preparation failed with {} configured failure(s)",
                    configured_failures.len()
                )?;
                if default_source_root_failure.is_some() {
                    write!(f, " and a default source root failure")?;
                }
                Ok(())
            }
            ObservedSourceFileInventoryFailure::Discovery { failures, .. } => {
                write!(
                    f,
                    "observed source file inventory discovery failed with {} scope failure(s)",
                    failures.len()
                )
            }
        }
    }
}

impl std::error::Error for ObservedSourceFileInventoryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

/// A configured directory selector that failed to resolve.
#[derive(Debug)]
pub struct ConfiguredSelectorFailure {
    /// Handle of the collection declaration containing the failed selector.
    pub collection_handle: String,
    /// Directory-selector spelling from the configuration.
    pub configured_selector: PathBuf,
    /// Error returned by the resolver.
    pub error: io::Error,
}

/// Failure to prepare the shared default source root.
#[derive(Debug)]
pub struct DefaultSourceRootFailure {
    /// Source-root spelling supplied by the caller.
    pub original_source_root: PathBuf,
    /// Handles of collection declarations without directory selectors that require this scope, in
    /// declaration order.
    pub dependent_collection_handles: Vec<String>,
    /// Reason preparation failed.
    pub kind: DefaultSourceRootFailureKind,
}

/// Reason default source root preparation failed.
#[derive(Debug)]
#[non_exhaustive]
pub enum DefaultSourceRootFailureKind {
    /// The supplied root is empty.
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

/// A required scope whose scan failed.
#[derive(Debug)]
pub struct RequiredScopeDiscoveryFailure {
    /// Complete affected scope description.
    pub scope: RequiredScope,
    /// Original discovery error.
    pub error: DiscoveryError,
}

pub(super) fn warnings_from_private(
    warnings: Vec<crate::collection_scope::RedundancyWarning>,
) -> Vec<RedundantConfiguredSelectors> {
    let mut converted = Vec::with_capacity(warnings.len());
    for warning in warnings {
        converted.push(RedundantConfiguredSelectors {
            collection_handle: warning.collection_handle,
            resolved_directory: warning.resolved_directory,
            contributing_selectors: warning.contributing_selectors,
        });
    }
    converted
}

pub(super) fn configured_failure_from_private(
    failure: crate::collection_scope::ConfiguredSelectorFailure,
) -> ConfiguredSelectorFailure {
    ConfiguredSelectorFailure {
        collection_handle: failure.collection_handle,
        configured_selector: failure.configured_selector,
        error: failure.error,
    }
}

pub(super) fn default_failure_from_private(
    failure: crate::collection_scope::DefaultSourceRootFailure,
) -> DefaultSourceRootFailure {
    let kind = match failure.kind {
        crate::collection_scope::DefaultSourceRootFailureKind::EmptyInput => {
            DefaultSourceRootFailureKind::EmptyInput
        }
        crate::collection_scope::DefaultSourceRootFailureKind::UnsupportedPathForm => {
            DefaultSourceRootFailureKind::UnsupportedPathForm
        }
        crate::collection_scope::DefaultSourceRootFailureKind::ResolutionFailed { error } => {
            DefaultSourceRootFailureKind::ResolutionFailed { error }
        }
        crate::collection_scope::DefaultSourceRootFailureKind::ResolvedPathInspectionFailed {
            resolved_path,
            error,
        } => DefaultSourceRootFailureKind::ResolvedPathInspectionFailed {
            resolved_path,
            error,
        },
        crate::collection_scope::DefaultSourceRootFailureKind::ResolvedTargetNotDirectory {
            resolved_path,
        } => DefaultSourceRootFailureKind::ResolvedTargetNotDirectory { resolved_path },
    };
    DefaultSourceRootFailure {
        original_source_root: failure.original_source_root,
        dependent_collection_handles: failure.dependent_collection_handles,
        kind,
    }
}

pub(super) fn discovery_failures_from_private(
    failures: crate::required_discovery::RequiredFailures,
) -> Vec<RequiredScopeDiscoveryFailure> {
    let mut converted =
        Vec::with_capacity(failures.configured.len() + failures.default.iter().len());
    for failure in failures.configured {
        converted.push(RequiredScopeDiscoveryFailure {
            scope: required_scope_from_configured_directory(failure.scope),
            error: failure.error,
        });
    }
    if let Some(failure) = failures.default {
        converted.push(RequiredScopeDiscoveryFailure {
            scope: required_scope_from_default_source_root(failure.scope),
            error: failure.error,
        });
    }
    converted
}

#[cfg(test)]
#[path = "../tests/observed_source_file_inventory/diagnostics.rs"]
mod tests;
