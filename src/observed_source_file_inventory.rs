/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Inventory of ordinary files observed under required filesystem scopes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::collection_scope::{CollectionScopePreparation, prepare_collection_scopes};
use crate::config::LibraryBuildSpec;
use crate::required_discovery::{RequiredDiscovery, discover_required_files};

mod diagnostics;

pub use diagnostics::{
    ConfiguredSelectorFailure, DefaultSourceRootFailure, DefaultSourceRootFailureKind,
    ObservedSourceFileInventoryFailure, RedundantConfiguredSelectors,
    RequiredScopeDiscoveryFailure,
};
use diagnostics::{
    configured_failure_from_private, default_failure_from_private, discovery_failures_from_private,
    warnings_from_private,
};

/// Builds a complete inventory of ordinary source-side files observed under
/// the required scopes.
///
/// Preparation must succeed before discovery starts. Any scan failure returns
/// diagnostics without a partial inventory.
///
/// Path observations equal under native `Path` equality are represented by one
/// [`ObservedSourceFile`]. It retains one observed pathname spelling and every
/// distinct scope that reported an equal path. Iteration order for files,
/// scopes, and reporting associations is unspecified. Ordering within scope
/// descriptions and diagnostics is preserved.
///
/// Successful inventories include every required scope, even empty ones.
/// Coverage records completed scans, not ongoing filesystem validity.
/// A reporting-scope association does not by itself establish collection membership.
///
/// This inventory does not classify or probe files, establish media validity,
/// interpret metadata, determine collection membership, assign logical-track
/// identity, or select or plan output. Other configuration does not filter
/// observed files.
///
/// Building the inventory does not alter file contents or directory entries.
// Owned diagnostics make this error larger than Clippy's default threshold.
#[allow(clippy::result_large_err)]
pub fn build_observed_source_file_inventory(
    spec: &LibraryBuildSpec,
    source_root: &Path,
) -> Result<ObservedSourceFileInventorySuccess, ObservedSourceFileInventoryFailure> {
    let preparation: CollectionScopePreparation = prepare_collection_scopes(spec, source_root);
    let (scopes, warnings) = match preparation {
        CollectionScopePreparation::Prepared { scopes, warnings } => (scopes, warnings),
        CollectionScopePreparation::Failed {
            configured_failures,
            default_source_root_failure,
            warnings,
        } => {
            return Err(ObservedSourceFileInventoryFailure::Preparation {
                configured_failures: configured_failures
                    .into_iter()
                    .map(configured_failure_from_private)
                    .collect(),
                default_source_root_failure: default_source_root_failure
                    .map(default_failure_from_private),
                warnings: warnings_from_private(warnings),
            });
        }
    };

    match discover_required_files(scopes, warnings) {
        RequiredDiscovery::Completed { coverage, warnings } => {
            let inventory = aggregate_coverage(coverage);
            Ok(ObservedSourceFileInventorySuccess {
                inventory,
                warnings: warnings_from_private(warnings),
            })
        }
        RequiredDiscovery::Failed { failures, warnings } => {
            Err(ObservedSourceFileInventoryFailure::Discovery {
                failures: discovery_failures_from_private(failures),
                warnings: warnings_from_private(warnings),
            })
        }
    }
}

/// Inventory with warnings from preparation.
#[derive(Debug)]
pub struct ObservedSourceFileInventorySuccess {
    inventory: ObservedSourceFileInventory,
    warnings: Vec<RedundantConfiguredSelectors>,
}

impl ObservedSourceFileInventorySuccess {
    /// The inventory.
    pub fn inventory(&self) -> &ObservedSourceFileInventory {
        &self.inventory
    }

    /// Warnings for redundant configured selectors.
    pub fn warnings(&self) -> &[RedundantConfiguredSelectors] {
        &self.warnings
    }
}

/// Complete required-scope coverage, with distinct observed source files and
/// their reporting-scope associations.
#[derive(Debug)]
pub struct ObservedSourceFileInventory {
    scopes: Vec<RequiredScope>,
    entries: HashMap<PathBuf, Vec<usize>>,
}

impl ObservedSourceFileInventory {
    /// Every required scope in this completed inventory, including empty ones.
    ///
    /// Order is unspecified.
    pub fn covered_scopes(&self) -> &[RequiredScope] {
        &self.scopes
    }

    /// Iterates over the observed source files. Order is unspecified.
    pub fn files(&self) -> impl Iterator<Item = ObservedSourceFile<'_>> + '_ {
        self.entries
            .iter()
            .map(|(path, scope_indices)| ObservedSourceFile {
                path: path.as_path(),
                scope_indices: scope_indices.as_slice(),
                scopes: self.scopes.as_slice(),
            })
    }
}

/// An ordinary source-side file observed by required-scope discovery,
/// represented by one pathname and its reporting scopes.
#[derive(Debug)]
pub struct ObservedSourceFile<'inventory> {
    path: &'inventory Path,
    scope_indices: &'inventory [usize],
    scopes: &'inventory [RequiredScope],
}

impl<'inventory> ObservedSourceFile<'inventory> {
    /// The retained observed pathname spelling for this file.
    ///
    /// Selection among equal observations is unspecified.
    pub fn path(&self) -> &'inventory Path {
        self.path
    }

    /// Scopes that reported an equal path under native `Path` equality.
    ///
    /// These associations do not by themselves establish collection membership.
    /// Order is unspecified.
    pub fn reporting_scopes(&self) -> impl Iterator<Item = &'inventory RequiredScope> + '_ {
        self.scope_indices.iter().map(|&index| &self.scopes[index])
    }
}

/// Description of a required scan scope.
///
/// It appears in `covered_scopes` only after all required scans succeed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequiredScope {
    /// One configured directory-selector group required by a collection declaration.
    ConfiguredDirectory {
        /// Handle of the collection declaration that requires this scope.
        collection_handle: String,
        /// Resolved path for this group.
        resolved_directory: PathBuf,
        /// Selector spellings in declaration order, including duplicates.
        contributing_selectors: Vec<PathBuf>,
    },
    /// Shared root for declarations without configured directory selectors.
    DefaultSourceRoot {
        /// Source-root spelling supplied by the caller.
        original_source_root: PathBuf,
        /// Resolved path used for scanning.
        resolved_traversal_root: PathBuf,
        /// Handles of collection declarations without directory selectors that require this scope,
        /// in declaration order.
        dependent_collection_handles: Vec<String>,
    },
}

fn required_scope_from_configured_directory(
    scope: crate::collection_scope::ConfiguredDirectoryScope,
) -> RequiredScope {
    RequiredScope::ConfiguredDirectory {
        collection_handle: scope.collection_handle,
        resolved_directory: scope.resolved_directory,
        contributing_selectors: scope.contributing_selectors,
    }
}

fn required_scope_from_default_source_root(
    scope: crate::collection_scope::DefaultSourceRootScope,
) -> RequiredScope {
    RequiredScope::DefaultSourceRoot {
        original_source_root: scope.original_source_root,
        resolved_traversal_root: scope.resolved_traversal_root,
        dependent_collection_handles: scope.dependent_collection_handles,
    }
}

fn aggregate_coverage(
    coverage: crate::required_discovery::RequiredCoverage,
) -> ObservedSourceFileInventory {
    let mut scopes: Vec<RequiredScope> =
        Vec::with_capacity(coverage.configured.len() + coverage.default.iter().len());
    let mut files_per_scope: Vec<Vec<PathBuf>> =
        Vec::with_capacity(coverage.configured.len() + coverage.default.iter().len());
    for entry in coverage.configured {
        scopes.push(required_scope_from_configured_directory(entry.scope));
        files_per_scope.push(entry.files);
    }
    if let Some(entry) = coverage.default {
        scopes.push(required_scope_from_default_source_root(entry.scope));
        files_per_scope.push(entry.files);
    }
    aggregate_observations(scopes, files_per_scope)
}

fn aggregate_observations(
    scopes: Vec<RequiredScope>,
    files_per_scope: Vec<Vec<PathBuf>>,
) -> ObservedSourceFileInventory {
    let total: usize = files_per_scope.iter().map(Vec::len).sum();
    let mut entries: HashMap<PathBuf, Vec<usize>> = HashMap::with_capacity(total);
    for (scope_index, files) in files_per_scope.iter().enumerate() {
        for observed in files {
            match entries.get_mut(observed.as_path()) {
                Some(scope_indices) => {
                    if !scope_indices.contains(&scope_index) {
                        scope_indices.push(scope_index);
                    }
                }
                None => {
                    entries.insert(observed.clone(), vec![scope_index]);
                }
            }
        }
    }
    ObservedSourceFileInventory { scopes, entries }
}

#[cfg(test)]
#[path = "tests/observed_source_file_inventory.rs"]
mod tests;
