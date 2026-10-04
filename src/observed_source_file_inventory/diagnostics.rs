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
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn discovery_failure_translation_retains_scope_and_error() {
        let scope = crate::collection_scope::ConfiguredDirectoryScope {
            collection_handle: "deadwing".to_owned(),
            resolved_directory: PathBuf::from("/music/Deadwing"),
            contributing_selectors: vec![PathBuf::from("Deadwing")],
        };
        let error = DiscoveryError::NotADirectory {
            path: PathBuf::from("/music/Deadwing"),
        };
        let failures = crate::required_discovery::RequiredFailures {
            configured: vec![crate::required_discovery::ConfiguredScopeFailure { scope, error }],
            default: Some(crate::required_discovery::DefaultScopeFailure {
                scope: crate::collection_scope::DefaultSourceRootScope {
                    original_source_root: PathBuf::from("source"),
                    resolved_traversal_root: PathBuf::from("/music/source"),
                    dependent_collection_handles: vec!["in-absentia".to_owned()],
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
                collection_handle,
                resolved_directory,
                contributing_selectors,
            } => {
                assert_eq!(collection_handle, "deadwing");
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
                dependent_collection_handles,
            } => {
                assert_eq!(original_source_root.as_os_str(), OsStr::new("source"));
                assert_eq!(
                    resolved_traversal_root.as_os_str(),
                    OsStr::new("/music/source")
                );
                assert_eq!(dependent_collection_handles, &["in-absentia".to_owned()]);
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

        let empty =
            default_failure_from_private(crate::collection_scope::DefaultSourceRootFailure {
                original_source_root: original_root.clone(),
                dependent_collection_handles: dependents.clone(),
                kind: crate::collection_scope::DefaultSourceRootFailureKind::EmptyInput,
            });
        assert!(matches!(
            empty.kind,
            DefaultSourceRootFailureKind::EmptyInput
        ));
        assert_eq!(empty.original_source_root, original_root);
        assert_eq!(empty.dependent_collection_handles, dependents);

        let unsupported =
            default_failure_from_private(crate::collection_scope::DefaultSourceRootFailure {
                original_source_root: original_root.clone(),
                dependent_collection_handles: dependents.clone(),
                kind: crate::collection_scope::DefaultSourceRootFailureKind::UnsupportedPathForm,
            });
        assert!(matches!(
            unsupported.kind,
            DefaultSourceRootFailureKind::UnsupportedPathForm
        ));

        let resolved = PathBuf::from("/music/resolved");
        let inspection =
            default_failure_from_private(crate::collection_scope::DefaultSourceRootFailure {
                original_source_root: original_root.clone(),
                dependent_collection_handles: dependents.clone(),
                kind:
                    crate::collection_scope::DefaultSourceRootFailureKind::ResolvedPathInspectionFailed {
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

        let not_dir = default_failure_from_private(crate::collection_scope::DefaultSourceRootFailure {
            original_source_root: original_root.clone(),
            dependent_collection_handles: dependents.clone(),
            kind: crate::collection_scope::DefaultSourceRootFailureKind::ResolvedTargetNotDirectory {
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
            default_failure_from_private(crate::collection_scope::DefaultSourceRootFailure {
                original_source_root: original_root.clone(),
                dependent_collection_handles: dependents.clone(),
                kind: crate::collection_scope::DefaultSourceRootFailureKind::ResolutionFailed {
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
    fn warnings_from_private_preserve_collection_root_and_selector_order() {
        let private = vec![
            crate::collection_scope::RedundancyWarning {
                collection_handle: "one".to_owned(),
                resolved_directory: PathBuf::from("/music/one"),
                contributing_selectors: vec![
                    PathBuf::from("one"),
                    PathBuf::from("./one"),
                    PathBuf::from("one"),
                ],
            },
            crate::collection_scope::RedundancyWarning {
                collection_handle: "three".to_owned(),
                resolved_directory: PathBuf::from("/music/two"),
                contributing_selectors: vec![PathBuf::from("two"), PathBuf::from("two")],
            },
        ];

        let converted = warnings_from_private(private);

        assert_eq!(converted.len(), 2);
        assert_eq!(converted[0].collection_handle, "one");
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
        assert_eq!(converted[1].collection_handle, "three");
        assert_eq!(
            converted[1].resolved_directory.as_os_str(),
            OsStr::new("/music/two")
        );
        assert_eq!(converted[1].contributing_selectors.len(), 2);
    }

    #[test]
    fn failure_display_and_error_source_are_stable() {
        let preparation = ObservedSourceFileInventoryFailure::Preparation {
            configured_failures: vec![ConfiguredSelectorFailure {
                collection_handle: "tool".to_owned(),
                configured_selector: PathBuf::from("missing"),
                error: io::Error::new(io::ErrorKind::NotFound, "missing"),
            }],
            default_source_root_failure: Some(DefaultSourceRootFailure {
                original_source_root: PathBuf::from("source"),
                dependent_collection_handles: vec!["dependent".to_owned()],
                kind: DefaultSourceRootFailureKind::EmptyInput,
            }),
            warnings: Vec::new(),
        };
        assert_eq!(
            preparation.to_string(),
            "observed source file inventory preparation failed with 1 configured failure(s) \
             and a default source root failure"
        );
        assert!(std::error::Error::source(&preparation).is_none());

        let configured_only = ObservedSourceFileInventoryFailure::Preparation {
            configured_failures: Vec::new(),
            default_source_root_failure: None,
            warnings: Vec::new(),
        };
        assert_eq!(
            configured_only.to_string(),
            "observed source file inventory preparation failed with 0 configured failure(s)"
        );
        assert!(std::error::Error::source(&configured_only).is_none());

        let discovery = ObservedSourceFileInventoryFailure::Discovery {
            failures: vec![RequiredScopeDiscoveryFailure {
                scope: RequiredScope::DefaultSourceRoot {
                    original_source_root: PathBuf::from("source"),
                    resolved_traversal_root: PathBuf::from("/music/source"),
                    dependent_collection_handles: vec!["dependent".to_owned()],
                },
                error: DiscoveryError::NotADirectory {
                    path: PathBuf::from("/music/source"),
                },
            }],
            warnings: Vec::new(),
        };
        assert_eq!(
            discovery.to_string(),
            "observed source file inventory discovery failed with 1 scope failure(s)"
        );
        assert!(std::error::Error::source(&discovery).is_none());
    }
}
