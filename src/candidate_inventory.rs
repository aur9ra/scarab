/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Inventory of ordinary files observed under required filesystem scopes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::album_scope::{AlbumScopePreparation, prepare_album_scopes};
use crate::config::LibraryBuildSpec;
use crate::required_discovery::{RequiredDiscovery, discover_required_files};

mod diagnostics;

pub use diagnostics::{
    CandidateInventoryFailure, ConfiguredSelectorFailure, DefaultSourceRootFailure,
    DefaultSourceRootFailureKind, RedundantConfiguredSelectors, RequiredScopeDiscoveryFailure,
};
use diagnostics::{
    configured_failure_from_private, default_failure_from_private, discovery_failures_from_private,
    warnings_from_private,
};

/// Builds an inventory of ordinary files under the required scopes.
///
/// Preparation must succeed before discovery starts. Any scan failure returns
/// diagnostics without a partial inventory.
///
/// Equal paths under native `Path` equality share a candidate. Each retains
/// one actually observed pathname and every distinct scope that reported it.
/// Candidate, scope, and association iteration order is unspecified.
/// Ordering within scope descriptions and diagnostics is preserved.
///
/// Successful inventories include every required scope, even empty ones.
/// Coverage records completed scans, not ongoing filesystem validity, and a
/// scope association does not establish album membership.
///
/// This inventory does not classify or probe files, establish media validity,
/// interpret metadata, assign album membership or logical-track identity, or
/// select or plan output. Other configuration does not filter candidates.
/// Building the inventory does not alter file contents or directory entries.
// Owned diagnostics make this error larger than Clippy's default threshold.
#[allow(clippy::result_large_err)]
pub fn build_candidate_inventory(
    spec: &LibraryBuildSpec,
    source_root: &Path,
) -> Result<CandidateInventorySuccess, CandidateInventoryFailure> {
    let preparation: AlbumScopePreparation = prepare_album_scopes(spec, source_root);
    let (scopes, warnings) = match preparation {
        AlbumScopePreparation::Prepared { scopes, warnings } => (scopes, warnings),
        AlbumScopePreparation::Failed {
            configured_failures,
            default_source_root_failure,
            warnings,
        } => {
            return Err(CandidateInventoryFailure::Preparation {
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
            Ok(CandidateInventorySuccess {
                inventory,
                warnings: warnings_from_private(warnings),
            })
        }
        RequiredDiscovery::Failed { failures, warnings } => {
            Err(CandidateInventoryFailure::Discovery {
                failures: discovery_failures_from_private(failures),
                warnings: warnings_from_private(warnings),
            })
        }
    }
}

/// Inventory with warnings from preparation.
#[derive(Debug)]
pub struct CandidateInventorySuccess {
    inventory: CandidateInventory,
    warnings: Vec<RedundantConfiguredSelectors>,
}

impl CandidateInventorySuccess {
    /// The inventory.
    pub fn inventory(&self) -> &CandidateInventory {
        &self.inventory
    }

    /// Warnings for redundant configured selectors.
    pub fn warnings(&self) -> &[RedundantConfiguredSelectors] {
        &self.warnings
    }
}

/// Distinct paths and the required scopes that reported them.
#[derive(Debug)]
pub struct CandidateInventory {
    scopes: Vec<RequiredScope>,
    entries: HashMap<PathBuf, Vec<usize>>,
}

impl CandidateInventory {
    /// Every required scope in this completed inventory, including empty ones.
    ///
    /// Order is unspecified.
    pub fn covered_scopes(&self) -> &[RequiredScope] {
        &self.scopes
    }

    /// Iterates over distinct paths. Order is unspecified.
    pub fn candidates(&self) -> impl Iterator<Item = Candidate<'_>> + '_ {
        self.entries.iter().map(|(path, scope_indices)| Candidate {
            path: path.as_path(),
            scope_indices: scope_indices.as_slice(),
            scopes: self.scopes.as_slice(),
        })
    }
}

/// One distinct ordinary pathname observed by required-scope discovery and
/// its reporting scopes.
#[derive(Debug)]
pub struct Candidate<'inventory> {
    path: &'inventory Path,
    scope_indices: &'inventory [usize],
    scopes: &'inventory [RequiredScope],
}

impl<'inventory> Candidate<'inventory> {
    /// An observed spelling for this candidate.
    ///
    /// Selection among equal observations is unspecified.
    pub fn path(&self) -> &'inventory Path {
        self.path
    }

    /// Scopes that reported an equal path under native `Path` equality.
    ///
    /// This does not establish album membership. Order is unspecified.
    pub fn scopes(&self) -> impl Iterator<Item = &'inventory RequiredScope> + '_ {
        self.scope_indices.iter().map(|&index| &self.scopes[index])
    }
}

/// Description of a required scan scope.
///
/// It appears in `covered_scopes` only after all required scans succeed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequiredScope {
    /// One configured directory-selector group required by an album declaration.
    ConfiguredDirectory {
        /// Album declaration for which this scope is required.
        album_handle: String,
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
        /// Album declarations without directory selectors that require this
        /// scope, in declaration order.
        dependent_album_handles: Vec<String>,
    },
}

fn required_scope_from_configured_directory(
    scope: crate::album_scope::ConfiguredDirectoryScope,
) -> RequiredScope {
    RequiredScope::ConfiguredDirectory {
        album_handle: scope.album_handle,
        resolved_directory: scope.resolved_directory,
        contributing_selectors: scope.contributing_selectors,
    }
}

fn required_scope_from_default_source_root(
    scope: crate::album_scope::DefaultSourceRootScope,
) -> RequiredScope {
    RequiredScope::DefaultSourceRoot {
        original_source_root: scope.original_source_root,
        resolved_traversal_root: scope.resolved_traversal_root,
        dependent_album_handles: scope.dependent_album_handles,
    }
}

fn aggregate_coverage(coverage: crate::required_discovery::RequiredCoverage) -> CandidateInventory {
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
) -> CandidateInventory {
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
    CandidateInventory { scopes, entries }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    fn configured_scope(
        album_handle: &str,
        resolved_directory: &str,
        contributing_selectors: &[&str],
    ) -> RequiredScope {
        RequiredScope::ConfiguredDirectory {
            album_handle: album_handle.to_owned(),
            resolved_directory: PathBuf::from(resolved_directory),
            contributing_selectors: contributing_selectors.iter().map(PathBuf::from).collect(),
        }
    }

    fn find_entry<'inventory>(
        inventory: &'inventory CandidateInventory,
        path: &Path,
    ) -> Option<Candidate<'inventory>> {
        inventory
            .candidates()
            .find(|candidate| candidate.path() == path)
    }

    #[test]
    fn equal_path_observations_with_different_spellings_share_one_candidate() {
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
        let mut candidates: Vec<Candidate<'_>> = inventory.candidates().collect();
        assert_eq!(candidates.len(), 1, "equal observations must deduplicate");
        let candidate = candidates.pop().expect("one candidate");
        let representative = candidate.path();
        assert!(
            representative == first.as_path() || representative == second.as_path(),
            "representative must be one reported pathname"
        );
        assert!(
            representative.as_os_str() == first.as_os_str()
                || representative.as_os_str() == second.as_os_str(),
            "representative spelling must match one observation"
        );
        let mut handles: Vec<&str> = candidate
            .scopes()
            .map(|scope| match scope {
                RequiredScope::ConfiguredDirectory { album_handle, .. } => album_handle.as_str(),
                RequiredScope::DefaultSourceRoot { .. } => {
                    panic!("expected configured scope")
                }
            })
            .collect();
        handles.sort();
        assert_eq!(handles, ["one", "two"]);
        assert_eq!(candidate.scopes().count(), 2);
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

        assert_eq!(inventory.candidates().count(), 2);
        assert!(find_entry(&inventory, Path::new("/music/album/a.dat")).is_some());
        assert!(find_entry(&inventory, Path::new("/music/album/b.dat")).is_some());
    }

    #[test]
    fn empty_scopes_stay_covered_without_candidates() {
        let scopes = vec![
            configured_scope("one", "/music/one", &["one"]),
            configured_scope("two", "/music/two", &["two"]),
        ];
        let inventory = aggregate_observations(
            scopes,
            vec![vec![PathBuf::from("/music/one/track.dat")], Vec::new()],
        );

        assert_eq!(inventory.covered_scopes().len(), 2);
        assert_eq!(inventory.candidates().count(), 1);
        let candidate = find_entry(&inventory, Path::new("/music/one/track.dat"))
            .expect("populated scope candidate must exist");
        assert_eq!(candidate.scopes().count(), 1);
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

        assert_eq!(inventory.candidates().count(), 1);
        let candidate = find_entry(&inventory, Path::new("/music/album/track.dat"))
            .expect("candidate must exist");
        assert_eq!(
            candidate.scopes().count(),
            1,
            "one scope yields one association even with duplicate equal observations"
        );
    }

    #[test]
    fn required_scope_translation_preserves_owned_fields_and_spellings() {
        let configured = required_scope_from_configured_directory(
            crate::album_scope::ConfiguredDirectoryScope {
                album_handle: "deadwing".to_owned(),
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
                album_handle,
                resolved_directory,
                contributing_selectors,
            } => {
                assert_eq!(album_handle, "deadwing");
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

        let default =
            required_scope_from_default_source_root(crate::album_scope::DefaultSourceRootScope {
                original_source_root: PathBuf::from("./source"),
                resolved_traversal_root: PathBuf::from("/music/source/./"),
                dependent_album_handles: vec!["z".to_owned(), "a".to_owned()],
            });
        match default {
            RequiredScope::DefaultSourceRoot {
                original_source_root,
                resolved_traversal_root,
                dependent_album_handles,
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
                    dependent_album_handles.len(),
                    2,
                    "length of dependent album handles must survive"
                );
                assert_eq!(dependent_album_handles, ["z", "a"]);
            }
            other => panic!("default translation must stay default, got {other:?}"),
        }
    }
}
