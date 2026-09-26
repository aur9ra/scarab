/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Inventory of ordinary files observed under required album scopes.

use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::album_scope::{AlbumScopePreparation, prepare_album_scopes};
use crate::config::LibraryBuildSpec;
use crate::discovery::DiscoveryError;
use crate::required_discovery::{RequiredDiscovery, discover_required_files};

/// Builds an inventory of ordinary files under the required scopes.
///
/// Preparation must succeed before discovery starts. Any scan failure returns
/// diagnostics without a partial inventory.
///
/// Equal paths under native `Path` equality share a candidate. Each retains
/// one observed pathname and every distinct scope that reported it. Candidate,
/// scope, and association iteration order is unspecified. Ordering within
/// scope descriptions and diagnostics is preserved.
///
/// Successful inventories include every required scope, even empty ones.
/// Coverage records completed scans, not ongoing filesystem validity, and a
/// scope association does not establish album membership.
///
/// This does not classify or probe files, interpret metadata, determine album
/// membership, or plan output. Other configuration does not filter candidates,
/// and it does not alter file contents or directory entries.
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

/// One distinct observed pathname and its reporting scopes.
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
    /// One configured selector group for an album.
    ConfiguredDirectory {
        /// Album that owns this scope.
        album_handle: String,
        /// Resolved path for this group.
        resolved_directory: PathBuf,
        /// Selector spellings in declaration order, including duplicates.
        contributing_selectors: Vec<PathBuf>,
    },
    /// Shared root for albums without selectors.
    DefaultSourceRoot {
        /// Source-root spelling supplied by the caller.
        original_source_root: PathBuf,
        /// Resolved path used for scanning.
        resolved_traversal_root: PathBuf,
        /// Albums that use this scope, in declaration order.
        dependent_album_handles: Vec<String>,
    },
}

/// Configured selectors in one album that resolve to the same directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedundantConfiguredSelectors {
    /// Album with redundant selectors.
    pub album_handle: String,
    /// Shared resolved path.
    pub resolved_directory: PathBuf,
    /// Selector spellings in declaration order, including duplicates.
    pub contributing_selectors: Vec<PathBuf>,
}

/// Candidate-inventory failure.
#[derive(Debug)]
pub enum CandidateInventoryFailure {
    /// Preparation failed; discovery did not run.
    Preparation {
        /// Selector failures in album and selector order.
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

impl CandidateInventoryFailure {
    /// Preparation warnings retained by the failure.
    pub fn warnings(&self) -> &[RedundantConfiguredSelectors] {
        match self {
            CandidateInventoryFailure::Preparation { warnings, .. }
            | CandidateInventoryFailure::Discovery { warnings, .. } => warnings,
        }
    }
}

impl fmt::Display for CandidateInventoryFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CandidateInventoryFailure::Preparation {
                configured_failures,
                default_source_root_failure,
                ..
            } => {
                write!(
                    f,
                    "candidate inventory preparation failed with {} configured failure(s)",
                    configured_failures.len()
                )?;
                if default_source_root_failure.is_some() {
                    write!(f, " and a default source root failure")?;
                }
                Ok(())
            }
            CandidateInventoryFailure::Discovery { failures, .. } => {
                write!(
                    f,
                    "candidate inventory discovery failed with {} scope failure(s)",
                    failures.len()
                )
            }
        }
    }
}

impl std::error::Error for CandidateInventoryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

/// A configured selector that failed to resolve.
#[derive(Debug)]
pub struct ConfiguredSelectorFailure {
    /// Album that owns the failed selector.
    pub album_handle: String,
    /// Selector spelling from the configuration.
    pub configured_selector: PathBuf,
    /// Error returned by the resolver.
    pub error: io::Error,
}

/// Failure to prepare the shared default source root.
#[derive(Debug)]
pub struct DefaultSourceRootFailure {
    /// Source-root spelling supplied by the caller.
    pub original_source_root: PathBuf,
    /// Albums affected by the failure, in declaration order.
    pub dependent_album_handles: Vec<String>,
    /// Reason preparation failed.
    pub kind: DefaultSourceRootFailureKind,
}

/// Reason default source root preparation failed.
#[derive(Debug)]
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

fn warnings_from_private(
    warnings: Vec<crate::album_scope::RedundancyWarning>,
) -> Vec<RedundantConfiguredSelectors> {
    let mut converted = Vec::with_capacity(warnings.len());
    for warning in warnings {
        converted.push(RedundantConfiguredSelectors {
            album_handle: warning.album_handle,
            resolved_directory: warning.resolved_directory,
            contributing_selectors: warning.contributing_selectors,
        });
    }
    converted
}

fn configured_failure_from_private(
    failure: crate::album_scope::ConfiguredSelectorFailure,
) -> ConfiguredSelectorFailure {
    ConfiguredSelectorFailure {
        album_handle: failure.album_handle,
        configured_selector: failure.configured_selector,
        error: failure.error,
    }
}

fn default_failure_from_private(
    failure: crate::album_scope::DefaultSourceRootFailure,
) -> DefaultSourceRootFailure {
    let kind = match failure.kind {
        crate::album_scope::DefaultSourceRootFailureKind::EmptyInput => {
            DefaultSourceRootFailureKind::EmptyInput
        }
        crate::album_scope::DefaultSourceRootFailureKind::UnsupportedPathForm => {
            DefaultSourceRootFailureKind::UnsupportedPathForm
        }
        crate::album_scope::DefaultSourceRootFailureKind::ResolutionFailed { error } => {
            DefaultSourceRootFailureKind::ResolutionFailed { error }
        }
        crate::album_scope::DefaultSourceRootFailureKind::ResolvedPathInspectionFailed {
            resolved_path,
            error,
        } => DefaultSourceRootFailureKind::ResolvedPathInspectionFailed {
            resolved_path,
            error,
        },
        crate::album_scope::DefaultSourceRootFailureKind::ResolvedTargetNotDirectory {
            resolved_path,
        } => DefaultSourceRootFailureKind::ResolvedTargetNotDirectory { resolved_path },
    };
    DefaultSourceRootFailure {
        original_source_root: failure.original_source_root,
        dependent_album_handles: failure.dependent_album_handles,
        kind,
    }
}

fn discovery_failures_from_private(
    failures: crate::required_discovery::RequiredFailures,
) -> Vec<RequiredScopeDiscoveryFailure> {
    let mut converted =
        Vec::with_capacity(failures.configured.len() + failures.default.iter().len());
    for failure in failures.configured {
        converted.push(RequiredScopeDiscoveryFailure {
            scope: RequiredScope::ConfiguredDirectory {
                album_handle: failure.scope.album_handle,
                resolved_directory: failure.scope.resolved_directory,
                contributing_selectors: failure.scope.contributing_selectors,
            },
            error: failure.error,
        });
    }
    if let Some(failure) = failures.default {
        converted.push(RequiredScopeDiscoveryFailure {
            scope: RequiredScope::DefaultSourceRoot {
                original_source_root: failure.scope.original_source_root,
                resolved_traversal_root: failure.scope.resolved_traversal_root,
                dependent_album_handles: failure.scope.dependent_album_handles,
            },
            error: failure.error,
        });
    }
    converted
}

fn aggregate_coverage(coverage: crate::required_discovery::RequiredCoverage) -> CandidateInventory {
    let mut scopes: Vec<RequiredScope> =
        Vec::with_capacity(coverage.configured.len() + coverage.default.iter().len());
    let mut files_per_scope: Vec<Vec<PathBuf>> =
        Vec::with_capacity(coverage.configured.len() + coverage.default.iter().len());
    for entry in coverage.configured {
        scopes.push(RequiredScope::ConfiguredDirectory {
            album_handle: entry.scope.album_handle,
            resolved_directory: entry.scope.resolved_directory,
            contributing_selectors: entry.scope.contributing_selectors,
        });
        files_per_scope.push(entry.files);
    }
    if let Some(entry) = coverage.default {
        scopes.push(RequiredScope::DefaultSourceRoot {
            original_source_root: entry.scope.original_source_root,
            resolved_traversal_root: entry.scope.resolved_traversal_root,
            dependent_album_handles: entry.scope.dependent_album_handles,
        });
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
    fn discovery_failure_translation_retains_scope_and_error() {
        let scope = crate::album_scope::ConfiguredDirectoryScope {
            album_handle: "deadwing".to_owned(),
            resolved_directory: PathBuf::from("/music/Deadwing"),
            contributing_selectors: vec![PathBuf::from("Deadwing")],
        };
        let error = DiscoveryError::NotADirectory {
            path: PathBuf::from("/music/Deadwing"),
        };
        let failures = crate::required_discovery::RequiredFailures {
            configured: vec![crate::required_discovery::ConfiguredScopeFailure { scope, error }],
            default: Some(crate::required_discovery::DefaultScopeFailure {
                scope: crate::album_scope::DefaultSourceRootScope {
                    original_source_root: PathBuf::from("source"),
                    resolved_traversal_root: PathBuf::from("/music/source"),
                    dependent_album_handles: vec!["in-absentia".to_owned()],
                },
                error: DiscoveryError::Root {
                    path: PathBuf::from("/music/source"),
                    source: io::Error::new(io::ErrorKind::NotFound, "missing"),
                },
            }),
        };

        let converted = discovery_failures_from_private(failures);

        assert_eq!(converted.len(), 2, "both scope failures stay separate");
        match &converted[0].scope {
            RequiredScope::ConfiguredDirectory {
                album_handle,
                resolved_directory,
                contributing_selectors,
            } => {
                assert_eq!(album_handle, "deadwing");
                assert_eq!(
                    resolved_directory.as_os_str(),
                    OsStr::new("/music/Deadwing")
                );
                assert_eq!(contributing_selectors.len(), 1);
                assert_eq!(
                    contributing_selectors[0].as_os_str(),
                    OsStr::new("Deadwing")
                );
            }
            other => panic!("first failure must keep configured scope, got {other:?}"),
        }
        match &converted[0].error {
            DiscoveryError::NotADirectory { path } => {
                assert_eq!(path.as_os_str(), OsStr::new("/music/Deadwing"));
            }
            other => panic!("configured error pathname must stay intact, got {other:?}"),
        }
        match &converted[1].scope {
            RequiredScope::DefaultSourceRoot {
                original_source_root,
                resolved_traversal_root,
                dependent_album_handles,
            } => {
                assert_eq!(original_source_root.as_os_str(), OsStr::new("source"));
                assert_eq!(
                    resolved_traversal_root.as_os_str(),
                    OsStr::new("/music/source")
                );
                assert_eq!(dependent_album_handles, &["in-absentia".to_owned()]);
            }
            other => panic!("second failure must keep default scope, got {other:?}"),
        }
        match &converted[1].error {
            DiscoveryError::Root { path, source } => {
                assert_eq!(path.as_os_str(), OsStr::new("/music/source"));
                assert_eq!(source.kind(), io::ErrorKind::NotFound);
                assert!(
                    source.to_string().contains("missing"),
                    "underlying io payload must stay distinguishable"
                );
            }
            other => panic!("default error pathname and payload must stay intact, got {other:?}"),
        }
    }

    #[test]
    fn default_failure_kind_translation_preserves_all_variants() {
        let original_root = PathBuf::from("source");
        let dependents = vec!["tool".to_owned()];

        let empty = default_failure_from_private(crate::album_scope::DefaultSourceRootFailure {
            original_source_root: original_root.clone(),
            dependent_album_handles: dependents.clone(),
            kind: crate::album_scope::DefaultSourceRootFailureKind::EmptyInput,
        });
        assert!(matches!(
            empty.kind,
            DefaultSourceRootFailureKind::EmptyInput
        ));
        assert_eq!(empty.original_source_root, original_root);
        assert_eq!(empty.dependent_album_handles, dependents);

        let unsupported =
            default_failure_from_private(crate::album_scope::DefaultSourceRootFailure {
                original_source_root: original_root.clone(),
                dependent_album_handles: dependents.clone(),
                kind: crate::album_scope::DefaultSourceRootFailureKind::UnsupportedPathForm,
            });
        assert!(matches!(
            unsupported.kind,
            DefaultSourceRootFailureKind::UnsupportedPathForm
        ));

        let resolved = PathBuf::from("/music/resolved");
        let inspection =
            default_failure_from_private(crate::album_scope::DefaultSourceRootFailure {
                original_source_root: original_root.clone(),
                dependent_album_handles: dependents.clone(),
                kind:
                    crate::album_scope::DefaultSourceRootFailureKind::ResolvedPathInspectionFailed {
                        resolved_path: resolved.clone(),
                        error: io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
                    },
            });
        match inspection.kind {
            DefaultSourceRootFailureKind::ResolvedPathInspectionFailed {
                resolved_path,
                error,
            } => {
                assert_eq!(resolved_path, resolved);
                assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            }
            other => panic!("inspection failure must stay typed, got {other:?}"),
        }

        let not_dir = default_failure_from_private(crate::album_scope::DefaultSourceRootFailure {
            original_source_root: original_root.clone(),
            dependent_album_handles: dependents.clone(),
            kind: crate::album_scope::DefaultSourceRootFailureKind::ResolvedTargetNotDirectory {
                resolved_path: resolved.clone(),
            },
        });
        match not_dir.kind {
            DefaultSourceRootFailureKind::ResolvedTargetNotDirectory { resolved_path } => {
                assert_eq!(resolved_path, resolved);
            }
            other => panic!("not-directory failure must stay typed, got {other:?}"),
        }

        let resolution =
            default_failure_from_private(crate::album_scope::DefaultSourceRootFailure {
                original_source_root: original_root.clone(),
                dependent_album_handles: dependents.clone(),
                kind: crate::album_scope::DefaultSourceRootFailureKind::ResolutionFailed {
                    error: io::Error::new(io::ErrorKind::NotFound, "missing"),
                },
            });
        match resolution.kind {
            DefaultSourceRootFailureKind::ResolutionFailed { error } => {
                assert_eq!(error.kind(), io::ErrorKind::NotFound);
            }
            other => panic!("resolution failure must stay typed, got {other:?}"),
        }
    }

    #[test]
    fn warnings_from_private_preserve_album_root_and_selector_order() {
        let private = vec![
            crate::album_scope::RedundancyWarning {
                album_handle: "one".to_owned(),
                resolved_directory: PathBuf::from("/music/one"),
                contributing_selectors: vec![
                    PathBuf::from("one"),
                    PathBuf::from("./one"),
                    PathBuf::from("one"),
                ],
            },
            crate::album_scope::RedundancyWarning {
                album_handle: "three".to_owned(),
                resolved_directory: PathBuf::from("/music/two"),
                contributing_selectors: vec![PathBuf::from("two"), PathBuf::from("two")],
            },
        ];

        let converted = warnings_from_private(private);

        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].album_handle, "one");
        assert_eq!(
            converted[0].resolved_directory.as_os_str(),
            OsStr::new("/music/one")
        );
        assert_eq!(converted[0].contributing_selectors.len(), 3);
        assert_eq!(
            converted[0].contributing_selectors[0].as_os_str(),
            OsStr::new("one")
        );
        assert_eq!(
            converted[0].contributing_selectors[1].as_os_str(),
            OsStr::new("./one")
        );
        assert_eq!(
            converted[0].contributing_selectors[2].as_os_str(),
            OsStr::new("one")
        );
        assert_eq!(converted[1].album_handle, "three");
        assert_eq!(
            converted[1].resolved_directory.as_os_str(),
            OsStr::new("/music/two")
        );
        assert_eq!(converted[1].contributing_selectors.len(), 2);
    }
}
