/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Builds filesystem scopes for albums in a validated library build spec.
//!
//! Relative selectors use the supplied source root, absolute selectors ignore
//! it. Selectors are grouped by resolved path within each album. Multiple
//! selectors for one path produce one warning per group. Albums without
//! selectors (as in, metadata-selecting albums) share one prepared
//! source-root scope.
//!
//! Relative selectors always use the original source root, not the prepared
//! default root. Scopes never merge across albums or between configured and
//! default roots. Preparation returns scopes only if every required resolution
//! succeeds; otherwise it returns failures and any warnings from successful
//! groups.
//!
//! This module only inspects paths. It does not modify the source tree, find
//! files, or decide which album owns them.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::album_directory::resolve_album_directory;
use crate::config::LibraryBuildSpec;

/// Result of preparing album filesystem scopes.
#[derive(Debug)]
pub(crate) enum AlbumScopePreparation {
    /// Every required scope was prepared.
    Prepared {
        /// The prepared scopes.
        scopes: PreparedAlbumScopes,
        /// Redundancy warnings from successful configured groups.
        warnings: Vec<RedundancyWarning>,
    },
    /// >=1 required scope failed to prepare.
    Failed {
        /// Selector failures in album and selector order; duplicates stay separate.
        configured_failures: Vec<ConfiguredSelectorFailure>,
        /// Failure to prepare the shared default root, if needed.
        default_source_root_failure: Option<DefaultSourceRootFailure>,
        /// Redundancy warnings from successful configured groups.
        warnings: Vec<RedundancyWarning>,
    },
}

/// Scopes from a successful preparation.
#[derive(Debug)]
pub(crate) struct PreparedAlbumScopes {
    /// Configured scopes in album and first-selector order.
    pub(crate) configured_directory_scopes: Vec<ConfiguredDirectoryScope>,
    /// Shared by albums without configured selectors, if any.
    pub(crate) default_source_root_scope: Option<DefaultSourceRootScope>,
}

/// One group of selectors in an album that resolved to the same path.
#[derive(Debug)]
pub(crate) struct ConfiguredDirectoryScope {
    /// Album that owns this scope.
    pub(crate) album_handle: String,
    /// Resolved path shared by this group.
    pub(crate) resolved_directory: PathBuf,
    /// Selector spellings in declaration order, including duplicates.
    pub(crate) contributing_selectors: Vec<PathBuf>,
}

/// One configured selector occurrence that failed to resolve.
#[derive(Debug)]
pub(crate) struct ConfiguredSelectorFailure {
    /// Album that owns the failed selector.
    pub(crate) album_handle: String,
    /// Selector spelling from the configuration.
    pub(crate) configured_selector: PathBuf,
    /// Error returned by the resolver.
    pub(crate) error: io::Error,
}

/// One source-root scope shared by albums without selectors.
#[derive(Debug)]
pub(crate) struct DefaultSourceRootScope {
    /// Source-root spelling supplied by the caller.
    pub(crate) original_source_root: PathBuf,
    /// Resolved path for later filesystem work.
    pub(crate) resolved_traversal_root: PathBuf,
    /// Albums that use this scope, in declaration order.
    pub(crate) dependent_album_handles: Vec<String>,
}

/// Failure to prepare the shared default source root.
#[derive(Debug)]
pub(crate) struct DefaultSourceRootFailure {
    /// Source-root spelling supplied by the caller.
    pub(crate) original_source_root: PathBuf,
    /// Albums affected by the failure, in declaration order.
    pub(crate) dependent_album_handles: Vec<String>,
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

/// Warning that several selectors in an album resolved to the same path.
#[derive(Debug)]
pub(crate) struct RedundancyWarning {
    /// Album with redundant selectors.
    pub(crate) album_handle: String,
    /// Shared resolved path.
    pub(crate) resolved_directory: PathBuf,
    /// Selector spellings in declaration order, including duplicates.
    pub(crate) contributing_selectors: Vec<PathBuf>,
}

/// Prepares filesystem scopes for albums in `spec` using `source_root`.
///
/// Each configured selector is resolved independently, in album and selector
/// order. Relative selectors use the supplied root, absolute selectors ignore
/// it. Selectors that resolve to the same path are grouped within their album,
/// groups with multiple selectors produce one warning. Failures are collected and
/// do not stop preparation.
///
/// If any album has no configured selectors, the root is resolved once and
/// shared by all albums with no configured selectors.
///
/// Configured selectors and the default root are checked even if one fails.
/// Scopes are returned only if all required resolutions succeed, otherwise the
/// result contains the failures and any warnings from successful groups.
pub(crate) fn prepare_album_scopes(
    spec: &LibraryBuildSpec,
    source_root: &Path,
) -> AlbumScopePreparation {
    let mut configured_directory_scopes: Vec<ConfiguredDirectoryScope> = Vec::new();
    let mut configured_failures: Vec<ConfiguredSelectorFailure> = Vec::new();
    let mut warnings: Vec<RedundancyWarning> = Vec::new();

    // Groups belong to one album; equal paths in other albums stay separate.
    for (album_handle, album) in spec.albums() {
        let Some(selectors) = album.directories.as_deref() else {
            continue;
        };

        let mut album_scopes: Vec<ConfiguredDirectoryScope> = Vec::new();
        for selector in selectors {
            // Resolve every occurrence independently, including duplicates.
            match resolve_album_directory(source_root, selector) {
                Ok(resolved_directory) => {
                    // See if resolve to same dir as other selector(s) prev
                    match album_scopes
                        .iter_mut()
                        .find(|scope| scope.resolved_directory == resolved_directory)
                    {
                        Some(scope) => scope.contributing_selectors.push(selector.clone()),
                        None => album_scopes.push(ConfiguredDirectoryScope {
                            album_handle: album_handle.clone(),
                            resolved_directory,
                            contributing_selectors: vec![selector.clone()],
                        }),
                    }
                }
                // Accumulate errors and continue
                Err(error) => configured_failures.push(ConfiguredSelectorFailure {
                    album_handle: album_handle.clone(),
                    configured_selector: selector.clone(),
                    error,
                }),
            }
        }

        for scope in &album_scopes {
            if scope.contributing_selectors.len() >= 2 {
                warnings.push(RedundancyWarning {
                    album_handle: scope.album_handle.clone(),
                    resolved_directory: scope.resolved_directory.clone(),
                    contributing_selectors: scope.contributing_selectors.clone(),
                });
            }
        }
        configured_directory_scopes.extend(album_scopes);
    }

    let dependent_album_handles: Vec<String> = spec
        .albums()
        .iter()
        .filter(|(_, album)| album.directories.is_none())
        .map(|(album_handle, _)| album_handle.clone())
        .collect();

    // Only prepare a default root when at least one album needs it.
    let mut default_source_root_scope = None;
    let mut default_source_root_failure = None;
    if !dependent_album_handles.is_empty() {
        match prepare_default_source_root(source_root, &dependent_album_handles) {
            Ok(scope) => default_source_root_scope = Some(scope),
            Err(kind) => {
                default_source_root_failure = Some(DefaultSourceRootFailure {
                    original_source_root: source_root.to_path_buf(),
                    dependent_album_handles,
                    kind,
                });
            }
        }
    }

    if configured_failures.is_empty() && default_source_root_failure.is_none() {
        AlbumScopePreparation::Prepared {
            scopes: PreparedAlbumScopes {
                configured_directory_scopes,
                default_source_root_scope,
            },
            warnings,
        }
    } else {
        AlbumScopePreparation::Failed {
            configured_failures,
            default_source_root_failure,
            warnings,
        }
    }
}

/// Resolves and checks the shared source root for albums that need it.
///
/// The scope keeps both the caller's spelling and the resolved directory.
fn prepare_default_source_root(
    source_root: &Path,
    dependent_album_handles: &[String],
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
        dependent_album_handles: dependent_album_handles.to_vec(),
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
mod tests {
    use super::*;
    use std::ffi::OsStr;

    /// Parses a test spec with the supplied album declarations.
    fn parse_spec(album_declarations: &str) -> LibraryBuildSpec {
        let text = format!("codec = \"opus\"\nbitrate = 128\n{album_declarations}");
        crate::parse(&text).expect("test configuration must parse and validate")
    }

    fn new_sandbox() -> tempfile::TempDir {
        tempfile::tempdir().expect("create test sandbox")
    }

    fn create_source(sandbox: &tempfile::TempDir) -> PathBuf {
        let source = sandbox.path().join("source");
        fs::create_dir(&source).expect("create source root");
        source
    }

    fn canonical(path: &Path) -> PathBuf {
        fs::canonicalize(path)
            .unwrap_or_else(|error| panic!("expected {} to canonicalize: {error}", path.display()))
    }

    /// Compares `error` with a fresh canonicalization failure for `effective`.
    fn assert_native_canonicalize_error(error: &io::Error, effective: &Path) {
        let expected = fs::canonicalize(effective)
            .expect_err("independent canonicalization of a known-failing path must fail");
        assert_eq!(error.kind(), expected.kind(), "native canonicalize kind");
        assert_eq!(
            error.raw_os_error(),
            expected.raw_os_error(),
            "native canonicalize raw OS code"
        );
    }

    fn assert_spelling(path: &Path, expected: &str) {
        assert_eq!(
            path.as_os_str(),
            OsStr::new(expected),
            "configured selector spelling changed"
        );
    }

    /// Escapes `path` for use as a TOML basic string.
    fn toml_string(path: &Path) -> String {
        let mut escaped = String::from("\"");
        for character in path.display().to_string().chars() {
            match character {
                '\\' => escaped.push_str("\\\\"),
                '"' => escaped.push_str("\\\""),
                character if character.is_control() => {
                    escaped.push_str(&format!("\\u{:04X}", character as u32));
                }
                character => escaped.push(character),
            }
        }
        escaped.push('"');
        escaped
    }

    fn expect_prepared(
        preparation: AlbumScopePreparation,
    ) -> (PreparedAlbumScopes, Vec<RedundancyWarning>) {
        match preparation {
            AlbumScopePreparation::Prepared { scopes, warnings } => (scopes, warnings),
            AlbumScopePreparation::Failed {
                configured_failures,
                default_source_root_failure,
                warnings,
            } => panic!(
                "expected complete preparation, got failures {configured_failures:?} \
                 {default_source_root_failure:?} and warnings {warnings:?}"
            ),
        }
    }

    fn expect_failed(
        preparation: AlbumScopePreparation,
    ) -> (
        Vec<ConfiguredSelectorFailure>,
        Option<DefaultSourceRootFailure>,
        Vec<RedundancyWarning>,
    ) {
        match preparation {
            AlbumScopePreparation::Failed {
                configured_failures,
                default_source_root_failure,
                warnings,
            } => (configured_failures, default_source_root_failure, warnings),
            AlbumScopePreparation::Prepared { scopes, warnings } => {
                panic!("expected failure, got prepared scopes {scopes:?} and warnings {warnings:?}")
            }
        }
    }

    #[derive(Debug, PartialEq)]
    enum TreeEntryKind {
        Directory,
        File,
        Other,
    }

    /// Captures each descendant's path, type, and file bytes, sorted by path.
    /// Includes directories and reads entry types without following symlinks.
    fn tree_fingerprint(root: &Path) -> Vec<(PathBuf, TreeEntryKind, Vec<u8>)> {
        fn walk(dir: &Path, entries: &mut Vec<(PathBuf, TreeEntryKind, Vec<u8>)>) {
            for entry in fs::read_dir(dir).expect("read tree directory") {
                let entry = entry.expect("read tree entry");
                let path = entry.path();
                let kind = match entry.file_type().expect("read tree entry type") {
                    file_type if file_type.is_dir() => TreeEntryKind::Directory,
                    file_type if file_type.is_file() => TreeEntryKind::File,
                    _ => TreeEntryKind::Other,
                };
                let bytes = match kind {
                    TreeEntryKind::File => fs::read(&path).expect("read tree file"),
                    _ => Vec::new(),
                };
                let descend = kind == TreeEntryKind::Directory;
                entries.push((path.clone(), kind, bytes));
                if descend {
                    walk(&path, entries);
                }
            }
        }

        let mut entries = Vec::new();
        walk(root, &mut entries);
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        entries
    }

    #[test]
    fn zero_albums_succeed_without_inspecting_a_nonexistent_root() {
        let sandbox = new_sandbox();
        let missing_root = sandbox.path().join("missing-root");
        let spec = parse_spec("");

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &missing_root));

        assert!(scopes.configured_directory_scopes.is_empty());
        assert!(scopes.default_source_root_scope.is_none());
        assert!(warnings.is_empty());
    }

    #[test]
    fn configured_only_albums_prepare_no_default_source_root() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        fs::create_dir(source.join("Lateralus")).expect("create album directory");
        let spec = parse_spec("[albums.tool]\ndirectory = \"Lateralus\"\n");

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));

        assert!(scopes.default_source_root_scope.is_none());
        assert!(warnings.is_empty());
        assert_eq!(scopes.configured_directory_scopes.len(), 1);
        let scope = &scopes.configured_directory_scopes[0];
        assert_eq!(scope.album_handle, "tool");
        assert_eq!(
            scope.resolved_directory,
            canonical(&source.join("Lateralus"))
        );
        assert_eq!(scope.contributing_selectors.len(), 1);
        assert_spelling(&scope.contributing_selectors[0], "Lateralus");
    }

    #[test]
    fn absolute_configured_selector_prepares_with_an_unrelated_nonexistent_root() {
        let sandbox = new_sandbox();
        let target = sandbox.path().join("absolute-target");
        fs::create_dir(&target).expect("create target");
        let missing_root = sandbox.path().join("missing-root");
        // Checking this missing root would fail, so success means it was skipped.
        let spec = parse_spec(&format!(
            "[albums.tool]\ndirectory = {}\n",
            toml_string(&target)
        ));

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &missing_root));

        assert!(scopes.default_source_root_scope.is_none());
        assert!(warnings.is_empty());
        assert_eq!(scopes.configured_directory_scopes.len(), 1);
        assert_eq!(
            scopes.configured_directory_scopes[0].resolved_directory,
            canonical(&target)
        );
    }

    #[test]
    fn one_dependent_album_prepares_the_default_source_root() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let spec = parse_spec("[albums.tool]\nname = \"Tool\"\n");

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));

        assert!(scopes.configured_directory_scopes.is_empty());
        assert!(warnings.is_empty());
        let default_scope = scopes
            .default_source_root_scope
            .expect("the dependent album must produce a default source-root scope");
        assert_eq!(
            default_scope.original_source_root.as_os_str(),
            source.as_os_str()
        );
        assert_eq!(default_scope.resolved_traversal_root, canonical(&source));
        assert_eq!(default_scope.dependent_album_handles, ["tool"]);
    }

    #[test]
    fn several_dependent_albums_retain_declaration_order() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        fs::create_dir(source.join("Lateralus")).expect("create album directory");
        let spec = parse_spec(
            "[albums.z]\nname = \"Z\"\n\
             [albums.a]\nname = \"A\"\n\
             [albums.m]\ndirectory = \"Lateralus\"\n\
             [albums.q]\nartist = \"Q\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));

        assert!(warnings.is_empty());
        assert_eq!(scopes.configured_directory_scopes.len(), 1);
        assert_eq!(scopes.configured_directory_scopes[0].album_handle, "m");
        let default_scope = scopes
            .default_source_root_scope
            .expect("dependent albums must produce a default source-root scope");
        assert_eq!(default_scope.dependent_album_handles, ["z", "a", "q"]);
        assert_eq!(default_scope.resolved_traversal_root, canonical(&source));
    }

    #[test]
    fn default_source_root_scope_retains_the_exact_original_spelling() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let spelling = source.join(".");
        let spec = parse_spec("[albums.tool]\nname = \"Tool\"\n");

        let (scopes, _) = expect_prepared(prepare_album_scopes(&spec, &spelling));

        let default_scope = scopes
            .default_source_root_scope
            .expect("dependent album must produce a default source-root scope");
        assert_eq!(
            default_scope.original_source_root.as_os_str(),
            spelling.as_os_str()
        );
        assert_ne!(
            default_scope.original_source_root.as_os_str(),
            canonical(&source).as_os_str(),
            "the original spelling must not be replaced by the resolved root"
        );
        assert_eq!(default_scope.resolved_traversal_root, canonical(&source));
    }

    #[test]
    fn exactly_empty_default_source_root_is_rejected() {
        let spec = parse_spec("[albums.tool]\nname = \"Tool\"\n");

        let (configured_failures, default_failure, warnings) =
            expect_failed(prepare_album_scopes(&spec, Path::new("")));

        assert!(configured_failures.is_empty());
        assert!(warnings.is_empty());
        let failure = default_failure.expect("the dependent album must fail with the empty root");
        assert!(matches!(
            failure.kind,
            DefaultSourceRootFailureKind::EmptyInput
        ));
        assert_eq!(failure.original_source_root.as_os_str(), OsStr::new(""));
        assert_eq!(failure.dependent_album_handles, ["tool"]);
    }
    #[test]
    fn whitespace_only_default_source_root_is_not_empty_input() {
        let spec = parse_spec("[albums.tool]\nname = \"Tool\"\n");

        // Resolution depends on the test's working directory; only check that
        // whitespace is not classified as empty.
        match prepare_album_scopes(&spec, Path::new("   ")) {
            AlbumScopePreparation::Prepared { scopes, warnings } => {
                assert!(warnings.is_empty());
                let default_scope = scopes
                    .default_source_root_scope
                    .expect("the dependent album must produce a default source-root scope");
                assert_eq!(
                    default_scope.original_source_root.as_os_str(),
                    OsStr::new("   ")
                );
            }
            AlbumScopePreparation::Failed {
                default_source_root_failure,
                ..
            } => {
                let failure = default_source_root_failure
                    .expect("the dependent album must observe the whitespace root");
                assert_eq!(failure.original_source_root.as_os_str(), OsStr::new("   "));
                assert!(
                    !matches!(failure.kind, DefaultSourceRootFailureKind::EmptyInput),
                    "whitespace-only input must not be classified as empty"
                );
                if let DefaultSourceRootFailureKind::ResolutionFailed { error } = failure.kind {
                    assert_native_canonicalize_error(&error, Path::new("   "));
                }
            }
        }
    }

    #[test]
    fn missing_default_source_root_reports_native_resolution_failure() {
        let sandbox = new_sandbox();
        let missing_root = sandbox.path().join("missing-root");
        let spec = parse_spec("[albums.tool]\nname = \"Tool\"\n");

        let (configured_failures, default_failure, warnings) =
            expect_failed(prepare_album_scopes(&spec, &missing_root));

        assert!(configured_failures.is_empty());
        assert!(warnings.is_empty());
        let failure = default_failure.expect("the dependent album must fail resolving the root");
        assert_eq!(
            failure.original_source_root.as_os_str(),
            missing_root.as_os_str()
        );
        assert_eq!(failure.dependent_album_handles, ["tool"]);
        match failure.kind {
            DefaultSourceRootFailureKind::ResolutionFailed { error } => {
                assert_native_canonicalize_error(&error, &missing_root);
            }
            other => panic!("missing root must fail through native resolution, got {other:?}"),
        }
    }

    #[test]
    fn ordinary_file_default_source_root_reports_resolved_target_not_directory() {
        let sandbox = new_sandbox();
        let file = sandbox.path().join("source-file");
        fs::write(&file, b"not a directory").expect("write file");
        let spec = parse_spec("[albums.tool]\nname = \"Tool\"\n");

        let (_, default_failure, _) = expect_failed(prepare_album_scopes(&spec, &file));

        let failure = default_failure.expect("the dependent album must fail inspecting the file");
        match failure.kind {
            DefaultSourceRootFailureKind::ResolvedTargetNotDirectory { resolved_path } => {
                assert_eq!(resolved_path, canonical(&file));
            }
            other => panic!("an ordinary file must be reported as not a directory, got {other:?}"),
        }
    }

    #[test]
    fn identical_successful_occurrences_group_with_one_warning() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let album = source.join("Lateralus");
        fs::create_dir(&album).expect("create album directory");
        let spec = parse_spec("[albums.tool]\ndirectories = [\"Lateralus\", \"Lateralus\"]\n");

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));

        assert_eq!(scopes.configured_directory_scopes.len(), 1);
        let scope = &scopes.configured_directory_scopes[0];
        assert_eq!(scope.album_handle, "tool");
        assert_eq!(scope.resolved_directory, canonical(&album));
        assert_eq!(scope.contributing_selectors.len(), 2);
        assert_spelling(&scope.contributing_selectors[0], "Lateralus");
        assert_spelling(&scope.contributing_selectors[1], "Lateralus");

        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].album_handle, "tool");
        assert_eq!(warnings[0].resolved_directory, canonical(&album));
        assert_eq!(
            warnings[0].contributing_selectors,
            scope.contributing_selectors
        );
    }

    #[test]
    fn three_equal_root_occurrences_retain_all_spellings_with_one_warning() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let album = source.join("Lateralus");
        fs::create_dir(&album).expect("create album directory");
        let spec = parse_spec(
            "[albums.tool]\ndirectories = [\"Lateralus\", \"./Lateralus\", \"Lateralus\"]\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));

        assert_eq!(scopes.configured_directory_scopes.len(), 1);
        let scope = &scopes.configured_directory_scopes[0];
        assert_eq!(scope.resolved_directory, canonical(&album));
        assert_eq!(scope.contributing_selectors.len(), 3);
        assert_spelling(&scope.contributing_selectors[0], "Lateralus");
        assert_spelling(&scope.contributing_selectors[1], "./Lateralus");
        assert_spelling(&scope.contributing_selectors[2], "Lateralus");

        assert_eq!(warnings.len(), 1, "three contributors still warn once");
        assert_eq!(warnings[0].contributing_selectors.len(), 3);
        assert_eq!(
            warnings[0].contributing_selectors,
            scope.contributing_selectors
        );
    }

    #[test]
    fn distinct_resolved_directories_form_distinct_groups_in_occurrence_order() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        fs::create_dir(source.join("album-one")).expect("create album-one");
        fs::create_dir(source.join("album-two")).expect("create album-two");
        // Reverse lexical order checks that groups follow declaration order.
        let spec = parse_spec("[albums.tool]\ndirectories = [\"album-two\", \"album-one\"]\n");

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));

        assert!(warnings.is_empty());
        assert_eq!(scopes.configured_directory_scopes.len(), 2);
        let first = &scopes.configured_directory_scopes[0];
        assert_eq!(
            first.resolved_directory,
            canonical(&source.join("album-two"))
        );
        assert_spelling(&first.contributing_selectors[0], "album-two");
        let second = &scopes.configured_directory_scopes[1];
        assert_eq!(
            second.resolved_directory,
            canonical(&source.join("album-one"))
        );
        assert_spelling(&second.contributing_selectors[0], "album-one");
    }

    #[test]
    fn ancestor_and_descendant_resolved_directories_are_not_merged() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        fs::create_dir_all(source.join("parent/child")).expect("create parent and child");

        for selectors in [["parent", "parent/child"], ["parent/child", "parent"]] {
            let spec = parse_spec(&format!(
                "[albums.tool]\ndirectories = [\"{}\", \"{}\"]\n",
                selectors[0], selectors[1]
            ));

            let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));

            assert!(warnings.is_empty(), "case {selectors:?}");
            assert_eq!(
                scopes.configured_directory_scopes.len(),
                2,
                "case {selectors:?}"
            );
            for (index, selector) in selectors.iter().enumerate() {
                let scope = &scopes.configured_directory_scopes[index];
                assert_eq!(
                    scope.resolved_directory,
                    canonical(&source.join(selector)),
                    "case {selectors:?}"
                );
                assert_spelling(&scope.contributing_selectors[0], selector);
            }
        }
    }

    #[test]
    fn equal_configured_roots_across_albums_remain_separate() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let album = source.join("Lateralus");
        fs::create_dir(&album).expect("create album directory");
        let spec = parse_spec(
            "[albums.one]\ndirectory = \"Lateralus\"\n\
             [albums.two]\ndirectory = \"Lateralus\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));

        assert!(
            warnings.is_empty(),
            "one contributor per album produces no warning"
        );
        assert_eq!(scopes.configured_directory_scopes.len(), 2);
        assert_eq!(scopes.configured_directory_scopes[0].album_handle, "one");
        assert_eq!(scopes.configured_directory_scopes[1].album_handle, "two");
        for scope in &scopes.configured_directory_scopes {
            assert_eq!(scope.resolved_directory, canonical(&album));
            assert_eq!(scope.contributing_selectors.len(), 1);
        }
    }

    #[test]
    fn configured_and_default_equal_roots_stay_separate_without_warning() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let spec = parse_spec(
            "[albums.one]\ndirectory = \".\"\n\
             [albums.two]\nname = \"Two\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));

        assert!(
            warnings.is_empty(),
            "equal configured and default roots produce no redundancy warning"
        );
        assert_eq!(scopes.configured_directory_scopes.len(), 1);
        let configured = &scopes.configured_directory_scopes[0];
        assert_eq!(configured.album_handle, "one");
        assert_eq!(configured.resolved_directory, canonical(&source));
        let default_scope = scopes
            .default_source_root_scope
            .expect("the dependent album must produce a default source-root scope");
        assert_eq!(default_scope.dependent_album_handles, ["two"]);
        assert_eq!(default_scope.resolved_traversal_root, canonical(&source));
    }

    #[test]
    fn repeated_missing_selectors_produce_repeated_ordered_failures() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let spec = parse_spec("[albums.tool]\ndirectories = [\"missing\", \"missing\"]\n");

        let (configured_failures, default_failure, warnings) =
            expect_failed(prepare_album_scopes(&spec, &source));

        assert!(
            default_failure.is_none(),
            "a configured album is not dependent on the default root"
        );
        assert!(warnings.is_empty());
        assert_eq!(
            configured_failures.len(),
            2,
            "repeated failures stay separate"
        );
        for failure in &configured_failures {
            assert_eq!(failure.album_handle, "tool");
            assert_spelling(&failure.configured_selector, "missing");
            assert_native_canonicalize_error(&failure.error, &source.join("missing"));
        }
    }

    #[test]
    fn distinct_missing_selectors_retain_occurrence_order() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let spec = parse_spec("[albums.tool]\ndirectories = [\"missing-b\", \"missing-a\"]\n");

        let (configured_failures, default_failure, warnings) =
            expect_failed(prepare_album_scopes(&spec, &source));

        assert!(default_failure.is_none());
        assert!(warnings.is_empty());
        assert_eq!(configured_failures.len(), 2);
        assert_spelling(&configured_failures[0].configured_selector, "missing-b");
        assert_spelling(&configured_failures[1].configured_selector, "missing-a");
        for failure in &configured_failures {
            assert_native_canonicalize_error(
                &failure.error,
                &source.join(&failure.configured_selector),
            );
        }
    }

    #[test]
    fn mixed_successes_and_failures_retain_diagnostics_and_warnings() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        fs::create_dir(source.join("album-one")).expect("create album-one");
        fs::create_dir(source.join("album-two")).expect("create album-two");
        let spec = parse_spec(
            "[albums.one]\ndirectories = [\"album-one\", \"missing\", \"album-one\"]\n\
             [albums.two]\ndirectories = [\"missing\"]\n\
             [albums.three]\ndirectories = [\"album-two\", \"album-two\"]\n",
        );

        // Failure returns no scopes but keeps diagnostics and warnings.
        let (configured_failures, default_failure, warnings) =
            expect_failed(prepare_album_scopes(&spec, &source));

        assert!(default_failure.is_none());
        assert_eq!(configured_failures.len(), 2);
        assert_eq!(configured_failures[0].album_handle, "one");
        assert_spelling(&configured_failures[0].configured_selector, "missing");
        assert_eq!(configured_failures[1].album_handle, "two");
        assert_spelling(&configured_failures[1].configured_selector, "missing");

        assert_eq!(warnings.len(), 2, "successful groups still warn on failure");
        assert_eq!(warnings[0].album_handle, "one");
        assert_eq!(
            warnings[0].resolved_directory,
            canonical(&source.join("album-one"))
        );
        assert_eq!(warnings[0].contributing_selectors.len(), 2);
        assert_eq!(warnings[1].album_handle, "three");
        assert_eq!(
            warnings[1].resolved_directory,
            canonical(&source.join("album-two"))
        );
        assert_eq!(warnings[1].contributing_selectors.len(), 2);
    }

    #[test]
    fn failing_configured_and_default_mechanisms_retain_both_categories() {
        let sandbox = new_sandbox();
        let missing_root = sandbox.path().join("missing-root");
        let missing_album = sandbox.path().join("missing-album");
        let spec = parse_spec(&format!(
            "[albums.one]\ndirectory = {}\n\
             [albums.two]\nname = \"Two\"\n",
            toml_string(&missing_album)
        ));

        let (configured_failures, default_failure, warnings) =
            expect_failed(prepare_album_scopes(&spec, &missing_root));

        assert!(warnings.is_empty());
        assert_eq!(configured_failures.len(), 1);
        assert_eq!(configured_failures[0].album_handle, "one");
        assert_native_canonicalize_error(&configured_failures[0].error, &missing_album);

        let failure = default_failure.expect("the dependent album must fail on the missing root");
        assert_eq!(failure.dependent_album_handles, ["two"]);
        assert!(matches!(
            failure.kind,
            DefaultSourceRootFailureKind::ResolutionFailed { .. }
        ));
    }

    #[test]
    fn preparation_does_not_mutate_the_source_tree() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        fs::create_dir(source.join("album-one")).expect("create album-one");
        fs::write(source.join("album-one/01-track.flac"), b"track bytes").expect("write track");
        fs::create_dir(source.join("album-two")).expect("create album-two");
        fs::write(source.join("album-two/02-track.flac"), b"other bytes").expect("write track");
        fs::write(source.join("sentinel.txt"), b"sentinel").expect("write sentinel");
        let spec = parse_spec(
            "[albums.one]\ndirectory = \"album-one\"\n\
             [albums.one-again]\ndirectory = \"album-one\"\n\
             [albums.two]\ndirectory = \"album-two\"\n\
             [albums.gone]\ndirectory = \"no-such-album\"\n\
             [albums.dependent]\nname = \"Dependent\"\n",
        );

        let before = tree_fingerprint(&source);
        let (configured_failures, default_failure, _) =
            expect_failed(prepare_album_scopes(&spec, &source));

        assert_eq!(configured_failures.len(), 1);
        assert!(default_failure.is_none());
        assert_eq!(
            tree_fingerprint(&source),
            before,
            "preparation must not mutate the source tree"
        );
    }

    #[cfg(windows)]
    mod windows {
        use super::super::{
            DefaultSourceRootFailureKind, default_root_form_supported, prepare_default_source_root,
        };
        use std::path::{Component, Path, Prefix};

        /// Returns the parsed prefix of `path`, if any.
        fn native_prefix(path: &Path) -> Option<Prefix<'_>> {
            match path.components().next() {
                Some(Component::Prefix(prefix)) => Some(prefix.kind()),
                _ => None,
            }
        }

        #[test]
        fn admitted_default_source_root_forms() {
            let admitted = [
                "music",
                "music/root",
                ".",
                "..",
                "~",
                r"~\Music",
                "   ",
                r"C:\Music",
                "C:/Music",
                r"\\server\share\Music",
                r"\\server\share",
                r"\\?\C:\Music",
                r"\\?\UNC\server\share\Music",
            ];

            for literal in admitted {
                assert!(
                    default_root_form_supported(Path::new(literal)),
                    "{literal:?} must be admitted"
                );
            }
        }

        #[test]
        fn rejected_default_source_root_forms() {
            let rejected = [
                "C:",
                "C:Music",
                r"\Music",
                "/Music",
                r"\\.\C:\Music",
                r"\\?\cat_pics",
                r"\\?\cat_pics\Music",
                r"\\?\C:",
            ];

            for literal in rejected {
                assert!(
                    !default_root_form_supported(Path::new(literal)),
                    "{literal:?} must be rejected"
                );
            }
        }

        #[test]
        fn partially_qualified_forms_are_native_nonabsolute_and_rejected() {
            // Path spelling, has_root(), and whether native parsing found a prefix.
            let cases: [(&str, bool, bool); 4] = [
                ("C:", false, true),
                ("C:Music", false, true),
                (r"\Music", true, false),
                ("/Music", true, false),
            ];

            for (literal, expected_root, expected_prefix) in cases {
                let root = Path::new(literal);
                assert_eq!(root.has_root(), expected_root, "root of {literal:?}");
                assert_eq!(
                    native_prefix(root).is_some(),
                    expected_prefix,
                    "prefix of {literal:?}"
                );
                assert!(!root.is_absolute(), "{literal:?} must be nonabsolute");
                assert!(
                    !default_root_form_supported(root),
                    "{literal:?} must be rejected"
                );
            }
        }

        #[test]
        fn unsupported_default_root_form_is_typed_without_filesystem_work() {
            let dependent_album_handles = vec!["tool".to_owned()];

            // Reject a drive-relative root before filesystem resolution.
            let kind = prepare_default_source_root(Path::new("C:"), &dependent_album_handles)
                .expect_err("drive-relative root must be rejected");
            assert!(
                matches!(kind, DefaultSourceRootFailureKind::UnsupportedPathForm),
                "unexpected failure kind {kind:?}"
            );
        }

        #[test]
        fn bare_verbatim_disk_is_native_absolute_but_rejected() {
            // Native parsing marks `\\?\C:` absolute despite its missing root
            // component; this policy still rejects it.
            let bare = Path::new(r"\\?\C:");
            assert!(
                bare.is_absolute(),
                "native classification precondition for {bare:?}"
            );
            assert!(
                !default_root_form_supported(bare),
                "bare verbatim disk spelling must be rejected"
            );

            let rooted = Path::new(r"\\?\C:\Music");
            assert!(rooted.is_absolute(), "test precondition for {rooted:?}");
            assert!(
                default_root_form_supported(rooted),
                "rooted verbatim disk spelling must be admitted"
            );
        }
    }
}
