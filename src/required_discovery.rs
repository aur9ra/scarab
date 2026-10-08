/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Discovers files for prepared collection scopes.
//!
//! Configured scopes and the optional shared default scope remain separate,
//! even when their roots are equal or overlap.
//!
//! The read-only walk collects ordinary files without extension filtering,
//! classification, probing, or collection-membership decisions.

use std::path::PathBuf;

use crate::collection_scope::{PreparedCollectionScopes, RedundancyWarning};
use crate::discovery::{DiscoveryError, discover_source_files};

/// Outcome of discovery across all required scopes.
#[derive(Debug)]
pub(crate) enum RequiredDiscovery {
    /// Every required scan succeeded.
    Completed {
        /// Coverage for every required scope, including empty scopes.
        coverage: RequiredCoverage,
        /// Preparation warnings.
        warnings: Vec<RedundancyWarning>,
    },
    /// One or more scans failed; partial coverage is discarded.
    Failed {
        /// All failures.
        failures: RequiredFailures,
        /// Preparation warnings.
        warnings: Vec<RedundancyWarning>,
    },
}

/// Discovered files grouped by required scope.
#[derive(Debug)]
pub(crate) struct RequiredCoverage {
    /// Configured scopes in preparation order.
    pub(crate) configured: Vec<ConfiguredScopeCoverage>,
    /// Shared default scope, if required.
    pub(crate) default: Option<DefaultScopeCoverage>,
}

/// A configured scope and files found under its resolved root.
#[derive(Debug)]
pub(crate) struct ConfiguredScopeCoverage {
    /// Prepared configured-directory scope, including its contributing selectors.
    pub(crate) scope: crate::collection_scope::ConfiguredDirectoryScope,
    /// Files found under the resolved directory.
    pub(crate) files: Vec<PathBuf>,
}

/// The shared default scope and files found under its resolved root.
#[derive(Debug)]
pub(crate) struct DefaultScopeCoverage {
    /// Prepared default source-root scope, including the handles of declarations that require it.
    pub(crate) scope: crate::collection_scope::DefaultSourceRootScope,
    /// Files found under the resolved traversal root.
    pub(crate) files: Vec<PathBuf>,
}

/// Failures grouped by required scope.
#[derive(Debug)]
pub(crate) struct RequiredFailures {
    /// Configured-scope failures in preparation order.
    pub(crate) configured: Vec<ConfiguredScopeFailure>,
    /// Shared default-scope failure, if any.
    pub(crate) default: Option<DefaultScopeFailure>,
}

/// A configured scope and its discovery error.
#[derive(Debug)]
pub(crate) struct ConfiguredScopeFailure {
    /// Affected configured-directory scope.
    pub(crate) scope: crate::collection_scope::ConfiguredDirectoryScope,
    /// Original discovery error.
    pub(crate) error: DiscoveryError,
}

/// The shared default scope and its discovery error.
#[derive(Debug)]
pub(crate) struct DefaultScopeFailure {
    /// Affected default source-root scope, including the handles of declarations that require it.
    pub(crate) scope: crate::collection_scope::DefaultSourceRootScope,
    /// Original discovery error.
    pub(crate) error: DiscoveryError,
}

/// Scans each required scope once using its resolved root.
///
/// All scopes are attempted. If any scan fails, all errors are returned and
/// successful results are discarded. `warnings` accompany either outcome.
pub(crate) fn discover_required_files(
    scopes: PreparedCollectionScopes,
    warnings: Vec<RedundancyWarning>,
) -> RequiredDiscovery {
    let PreparedCollectionScopes {
        configured_directory_scopes,
        default_source_root_scope,
    } = scopes;

    let mut configured_coverage = Vec::new();
    let mut configured_failures = Vec::new();

    for scope in configured_directory_scopes {
        match discover_source_files(&scope.resolved_directory) {
            Ok(files) => configured_coverage.push(ConfiguredScopeCoverage { scope, files }),
            Err(error) => configured_failures.push(ConfiguredScopeFailure { scope, error }),
        }
    }

    let mut default_coverage = None;
    let mut default_failure = None;
    if let Some(scope) = default_source_root_scope {
        match discover_source_files(&scope.resolved_traversal_root) {
            Ok(files) => default_coverage = Some(DefaultScopeCoverage { scope, files }),
            Err(error) => default_failure = Some(DefaultScopeFailure { scope, error }),
        }
    }

    if configured_failures.is_empty() && default_failure.is_none() {
        RequiredDiscovery::Completed {
            coverage: RequiredCoverage {
                configured: configured_coverage,
                default: default_coverage,
            },
            warnings,
        }
    } else {
        RequiredDiscovery::Failed {
            failures: RequiredFailures {
                configured: configured_failures,
                default: default_failure,
            },
            warnings,
        }
    }
}

#[cfg(test)]
#[path = "tests/required_discovery.rs"]
mod tests;
