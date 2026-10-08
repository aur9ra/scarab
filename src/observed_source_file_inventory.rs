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
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn configured_scope(
        collection_handle: &str,
        resolved_directory: &str,
        contributing_selectors: &[&str],
    ) -> RequiredScope {
        RequiredScope::ConfiguredDirectory {
            collection_handle: collection_handle.to_owned(),
            resolved_directory: PathBuf::from(resolved_directory),
            contributing_selectors: contributing_selectors.iter().map(PathBuf::from).collect(),
        }
    }

    fn find_entry<'inventory>(
        inventory: &'inventory ObservedSourceFileInventory,
        path: &Path,
    ) -> Option<ObservedSourceFile<'inventory>> {
        inventory
            .files()
            .find(|observed_file| observed_file.path() == path)
    }

    #[test]
    fn equal_path_observations_with_different_spellings_share_one_observed_source_file() {
        let scopes = vec![
            configured_scope("one", "/music/album", &["Album"]),
            configured_scope("two", "/music/album", &["Album"]),
        ];
        let first = PathBuf::from("/music/album/./track.dat");
        let second = PathBuf::from("/music/album/track.dat");
        assert_ne!(
            first.as_os_str(),
            second.as_os_str(),
            "test precondition needs different raw spellings"
        );
        assert_eq!(first, second, "test precondition needs equal native paths");

        let inventory =
            aggregate_observations(scopes, vec![vec![first.clone()], vec![second.clone()]]);

        assert_eq!(inventory.covered_scopes().len(), 2);
        let mut observed_files: Vec<ObservedSourceFile<'_>> = inventory.files().collect();
        assert_eq!(
            observed_files.len(),
            1,
            "equal observations must deduplicate"
        );
        let observed_file = observed_files.pop().expect("one observed source file");
        let representative = observed_file.path();
        assert!(
            representative == first.as_path() || representative == second.as_path(),
            "representative must be one reported pathname"
        );
        assert!(
            representative.as_os_str() == first.as_os_str()
                || representative.as_os_str() == second.as_os_str(),
            "representative spelling must match one observation"
        );
        let mut handles: Vec<&str> = observed_file
            .reporting_scopes()
            .map(|scope| match scope {
                RequiredScope::ConfiguredDirectory {
                    collection_handle, ..
                } => collection_handle.as_str(),
                RequiredScope::DefaultSourceRoot { .. } => {
                    panic!("expected configured scope")
                }
            })
            .collect();
        handles.sort();
        assert_eq!(handles, ["one", "two"]);
        assert_eq!(observed_file.reporting_scopes().count(), 2);
    }

    #[test]
    fn distinct_pathnames_stay_distinct() {
        let scopes = vec![configured_scope("one", "/music/album", &["Album"])];
        let inventory = aggregate_observations(
            scopes,
            vec![vec![
                PathBuf::from("/music/album/a.dat"),
                PathBuf::from("/music/album/b.dat"),
            ]],
        );

        assert_eq!(inventory.files().count(), 2);
        assert!(find_entry(&inventory, Path::new("/music/album/a.dat")).is_some());
        assert!(find_entry(&inventory, Path::new("/music/album/b.dat")).is_some());
    }

    #[test]
    fn metadata_requirements_use_reporting_associations_not_equal_or_overlapping_roots() {
        use crate::metadata_extraction_requirements::required_metadata_paths;
        use crate::test_support::parse_spec;

        let spec = parse_spec(
            "[collections.meta]\ndirectory = \"Album\"\nalbum_name = \"Keep\"\n\
             [collections.dir]\ndirectory = \"Album\"\n",
        );
        let track = PathBuf::from("/music/album/track.flac");
        for metadata_root in ["/music/album", "/music"] {
            let scopes = vec![
                configured_scope("meta", metadata_root, &["Album"]),
                configured_scope("dir", "/music/album", &["Album"]),
            ];
            // coverage can observe different contents at different scan times
            let empty_files_per_scope = vec![vec![], vec![track.clone()]];
            let inventory = aggregate_observations(scopes, empty_files_per_scope);
            assert!(required_metadata_paths(&spec, &inventory).is_empty());
        }

        // a reported pathname qualifies even outside the scope's lexical root
        let inventory = aggregate_observations(
            vec![configured_scope("meta", "/elsewhere", &["Album"])],
            vec![vec![track.clone()]],
        );
        assert_eq!(
            required_metadata_paths(&spec, &inventory),
            vec![track.as_path()]
        );
    }

    #[test]
    fn metadata_requirements_borrow_inventory_retained_spelling() {
        use crate::metadata_extraction_requirements::required_metadata_paths;
        use crate::test_support::parse_spec;

        let spec = parse_spec("[collections.meta]\ndirectory = \"Album\"\nalbum_name = \"Keep\"\n");
        let retained = PathBuf::from("/music/album/./track.FlaC");
        let equal_spelling = PathBuf::from("/music/album/track.FlaC");
        assert_eq!(retained, equal_spelling);
        assert_ne!(retained.as_os_str(), equal_spelling.as_os_str());
        let inventory = aggregate_observations(
            vec![configured_scope("meta", "/music/album", &["Album"])],
            vec![vec![retained.clone(), equal_spelling]],
        );
        let paths = required_metadata_paths(&spec, &inventory);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].as_os_str(), retained.as_os_str());
        assert!(std::ptr::eq(
            paths[0],
            inventory.files().next().unwrap().path()
        ));
    }

    #[test]
    #[should_panic(
        expected = "reporting configured-scope handle must exist in the paired build specification"
    )]
    fn metadata_requirements_reject_unknown_configured_handle() {
        use crate::metadata_extraction_requirements::required_metadata_paths;
        use crate::test_support::parse_spec;

        let spec = parse_spec("[collections.meta]\nalbum_name = \"Keep\"\n");
        let inventory = aggregate_observations(
            vec![configured_scope("unknown", "/music", &["Album"])],
            vec![vec![PathBuf::from("/music/track.flac")]],
        );
        required_metadata_paths(&spec, &inventory);
    }

    #[test]
    fn empty_scopes_stay_covered_without_observed_files() {
        let scopes = vec![
            configured_scope("one", "/music/one", &["one"]),
            configured_scope("two", "/music/two", &["two"]),
        ];
        let inventory = aggregate_observations(
            scopes,
            vec![vec![PathBuf::from("/music/one/track.dat")], Vec::new()],
        );

        assert_eq!(inventory.covered_scopes().len(), 2);
        assert_eq!(inventory.files().count(), 1);
        let observed_file = find_entry(&inventory, Path::new("/music/one/track.dat"))
            .expect("populated scope file must exist");
        assert_eq!(observed_file.reporting_scopes().count(), 1);
    }

    #[test]
    fn same_scope_duplicate_observation_keeps_one_association() {
        let scopes = vec![configured_scope("one", "/music/album", &["Album"])];
        let inventory = aggregate_observations(
            scopes,
            vec![vec![
                PathBuf::from("/music/album/track.dat"),
                PathBuf::from("/music/album/./track.dat"),
            ]],
        );

        assert_eq!(inventory.files().count(), 1);
        let observed_file = find_entry(&inventory, Path::new("/music/album/track.dat"))
            .expect("observed source file must exist");
        assert_eq!(
            observed_file.reporting_scopes().count(),
            1,
            "one scope yields one association even with duplicate equal observations"
        );
    }

    #[test]
    fn required_scope_translation_preserves_owned_fields_and_spellings() {
        let configured = required_scope_from_configured_directory(
            crate::collection_scope::ConfiguredDirectoryScope {
                collection_handle: "deadwing".to_owned(),
                resolved_directory: PathBuf::from("/music/./Deadwing"),
                contributing_selectors: vec![
                    PathBuf::from("Deadwing"),
                    PathBuf::from("./Deadwing"),
                    PathBuf::from("Deadwing"),
                ],
            },
        );
        match configured {
            RequiredScope::ConfiguredDirectory {
                collection_handle,
                resolved_directory,
                contributing_selectors,
            } => {
                assert_eq!(collection_handle, "deadwing");
                assert_eq!(
                    resolved_directory.as_os_str(),
                    OsStr::new("/music/./Deadwing"),
                    "resolved spelling must move unchanged"
                );
                assert_eq!(
                    contributing_selectors.len(),
                    3,
                    "length of contributing selectors must survive"
                );
                assert_eq!(
                    contributing_selectors[0].as_os_str(),
                    OsStr::new("Deadwing")
                );
                assert_eq!(
                    contributing_selectors[1].as_os_str(),
                    OsStr::new("./Deadwing")
                );
                assert_eq!(
                    contributing_selectors[2].as_os_str(),
                    OsStr::new("Deadwing")
                );
            }
            other => panic!("configured translation must stay configured, got {other:?}"),
        }

        let default = required_scope_from_default_source_root(
            crate::collection_scope::DefaultSourceRootScope {
                original_source_root: PathBuf::from("./source"),
                resolved_traversal_root: PathBuf::from("/music/source/./"),
                dependent_collection_handles: vec!["z".to_owned(), "a".to_owned()],
            },
        );
        match default {
            RequiredScope::DefaultSourceRoot {
                original_source_root,
                resolved_traversal_root,
                dependent_collection_handles,
            } => {
                assert_eq!(
                    original_source_root.as_os_str(),
                    OsStr::new("./source"),
                    "caller spelling must move unchanged"
                );
                assert_eq!(
                    resolved_traversal_root.as_os_str(),
                    OsStr::new("/music/source/./"),
                    "resolved spelling must move unchanged"
                );
                assert_eq!(
                    dependent_collection_handles.len(),
                    2,
                    "length of dependent collection handles must survive"
                );
                assert_eq!(dependent_collection_handles, ["z", "a"]);
            }
            other => panic!("default translation must stay default, got {other:?}"),
        }
    }
}
