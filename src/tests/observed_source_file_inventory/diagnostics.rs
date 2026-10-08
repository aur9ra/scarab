/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

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

    let empty = default_failure_from_private(crate::collection_scope::DefaultSourceRootFailure {
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
