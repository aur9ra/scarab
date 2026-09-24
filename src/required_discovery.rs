/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Discovers files for prepared album scopes.
//!
//! Configured scopes and the optional shared default scope remain separate,
//! even when their roots are equal or overlap.
//!
//! The read-only walk collects ordinary files without extension filtering,
//! classification, probing, or album-membership decisions.

use std::path::PathBuf;

use crate::album_scope::{PreparedAlbumScopes, RedundancyWarning};
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
    pub(crate) scope: crate::album_scope::ConfiguredDirectoryScope,
    /// Files found under the resolved directory.
    pub(crate) files: Vec<PathBuf>,
}

/// The shared default scope and files found under its resolved root.
#[derive(Debug)]
pub(crate) struct DefaultScopeCoverage {
    /// Prepared scope and dependent albums.
    pub(crate) scope: crate::album_scope::DefaultSourceRootScope,
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
    pub(crate) scope: crate::album_scope::ConfiguredDirectoryScope,
    /// Original discovery error.
    pub(crate) error: DiscoveryError,
}

/// The shared default scope and its discovery error.
#[derive(Debug)]
pub(crate) struct DefaultScopeFailure {
    /// Affected scope and dependent albums.
    pub(crate) scope: crate::album_scope::DefaultSourceRootScope,
    /// Original discovery error.
    pub(crate) error: DiscoveryError,
}

/// Scans each required scope once using its resolved root.
///
/// All scopes are attempted. If any scan fails, all errors are returned and
/// successful results are discarded. `warnings` accompany either outcome.
pub(crate) fn discover_required_files(
    scopes: PreparedAlbumScopes,
    warnings: Vec<RedundancyWarning>,
) -> RequiredDiscovery {
    let PreparedAlbumScopes {
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
mod tests {
    use super::*;
    use crate::album_scope::{AlbumScopePreparation, prepare_album_scopes};
    use std::fs;
    use std::path::Path;

    fn parse_spec(album_declarations: &str) -> crate::config::LibraryBuildSpec {
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

    fn expect_completed(outcome: RequiredDiscovery) -> (RequiredCoverage, Vec<RedundancyWarning>) {
        match outcome {
            RequiredDiscovery::Completed { coverage, warnings } => (coverage, warnings),
            RequiredDiscovery::Failed { failures, warnings } => panic!(
                "expected completed discovery, got failures {failures:?} \
                 and warnings {warnings:?}"
            ),
        }
    }

    fn expect_failed(outcome: RequiredDiscovery) -> (RequiredFailures, Vec<RedundancyWarning>) {
        match outcome {
            RequiredDiscovery::Failed { failures, warnings } => (failures, warnings),
            RequiredDiscovery::Completed { coverage, warnings } => panic!(
                "expected failed discovery, got coverage {coverage:?} \
                 and warnings {warnings:?}"
            ),
        }
    }

    #[derive(Debug, PartialEq)]
    enum TreeEntryKind {
        Directory,
        File,
        Other,
    }

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
    fn zero_scopes_succeed_without_scanning() {
        let sandbox = new_sandbox();
        let missing_root = sandbox.path().join("missing-root");
        let spec = parse_spec("");

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &missing_root));
        assert!(scopes.configured_directory_scopes.is_empty());
        assert!(scopes.default_source_root_scope.is_none());

        // This missing root would fail if scanned.
        let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

        assert!(coverage.configured.is_empty());
        assert!(coverage.default.is_none());
        assert!(warnings.is_empty());
    }

    #[test]
    fn one_configured_scope_collects_ordinary_files_file_blind() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let fear_of_a_blank_planet = source.join("Fear of a Blank Planet");
        fs::create_dir(&fear_of_a_blank_planet).expect("create album directory");
        fs::write(fear_of_a_blank_planet.join("notes.txt"), b"notes")
            .expect("write non-audio file");
        fs::write(fear_of_a_blank_planet.join("no-extension"), b"raw")
            .expect("write extensionless file");
        let spec =
            parse_spec("[albums.fear-of-a-blank-planet]\ndirectory = \"Fear of a Blank Planet\"\n");

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

        assert!(warnings.is_empty());
        assert_eq!(coverage.configured.len(), 1);
        assert!(coverage.default.is_none());
        let entry = &coverage.configured[0];
        assert_eq!(entry.scope.album_handle, "fear-of-a-blank-planet");
        assert_eq!(
            entry.scope.resolved_directory,
            canonical(&fear_of_a_blank_planet)
        );
        assert_eq!(
            entry.scope.contributing_selectors,
            [PathBuf::from("Fear of a Blank Planet")]
        );
        let mut expected = vec![
            canonical(&fear_of_a_blank_planet).join("no-extension"),
            canonical(&fear_of_a_blank_planet).join("notes.txt"),
        ];
        expected.sort();
        // Returned paths retain the resolved-root prefix.
        let mut discovered = entry.files.clone();
        discovered.sort();
        assert_eq!(discovered.len(), 2);
        for path in &discovered {
            assert!(
                path.starts_with(canonical(&fear_of_a_blank_planet)),
                "discovered {} must stay under the resolved root",
                path.display()
            );
        }
        assert_eq!(discovered, expected);
    }

    #[test]
    fn shared_default_scope_retains_several_dependent_albums() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        fs::write(source.join("sentinel.txt"), b"sentinel").expect("write sentinel");
        let spec = parse_spec(
            "[albums.z]\nname = \"Z\"\n\
             [albums.a]\nname = \"A\"\n\
             [albums.q]\nartist = \"Q\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

        assert!(warnings.is_empty());
        assert!(coverage.configured.is_empty());
        let default = coverage.default.expect("default scope must stay covered");
        assert_eq!(default.scope.dependent_album_handles, ["z", "a", "q"]);
        assert_eq!(default.scope.resolved_traversal_root, canonical(&source));
        assert_eq!(
            default.files,
            vec![default.scope.resolved_traversal_root.join("sentinel.txt")]
        );
    }

    #[test]
    fn several_scopes_succeed_with_empty_scope_covered() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let fear_of_a_blank_planet = source.join("Fear of a Blank Planet");
        fs::create_dir(&fear_of_a_blank_planet).expect("create album with files");
        fs::write(fear_of_a_blank_planet.join("Anesthetize.flac"), b"audio").expect("write track");
        let empty = source.join("empty-album");
        fs::create_dir(&empty).expect("create empty album");
        let spec = parse_spec(
            "[albums.one]\ndirectory = \"Fear of a Blank Planet\"\n\
             [albums.two]\ndirectory = \"empty-album\"\n\
             [albums.three]\nname = \"Three\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

        assert!(warnings.is_empty());
        assert_eq!(coverage.configured.len(), 2);
        assert_eq!(coverage.configured[0].scope.album_handle, "one");
        assert_eq!(
            coverage.configured[0].files,
            vec![canonical(&fear_of_a_blank_planet).join("Anesthetize.flac")]
        );
        assert_eq!(coverage.configured[1].scope.album_handle, "two");
        assert_eq!(
            coverage.configured[1].scope.resolved_directory,
            canonical(&empty)
        );
        assert!(
            coverage.configured[1].files.is_empty(),
            "a successfully empty scope remains represented with no files"
        );
        let default = coverage.default.expect("default scope must stay covered");
        assert_eq!(default.scope.dependent_album_handles, ["three"]);
    }

    #[test]
    fn removing_two_of_three_roots_retains_both_failures() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        for name in ["a", "b", "c"] {
            let dir = source.join(name);
            fs::create_dir(&dir).expect("create album directory");
            fs::write(dir.join("Anesthetize.dat"), b"track").expect("write track");
        }
        let spec = parse_spec(
            "[albums.a]\ndirectory = \"a\"\n\
             [albums.b]\ndirectory = \"b\"\n\
             [albums.c]\ndirectory = \"c\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        let expected_a = canonical(&source.join("a"));
        let expected_c = canonical(&source.join("c"));
        fs::remove_dir_all(source.join("a")).expect("remove A");
        fs::remove_dir_all(source.join("c")).expect("remove C");

        let (failures, warnings) = expect_failed(discover_required_files(scopes, warnings));

        assert!(warnings.is_empty());
        assert!(failures.default.is_none());
        assert_eq!(failures.configured.len(), 2);
        assert_eq!(failures.configured[0].scope.album_handle, "a");
        assert_eq!(failures.configured[0].scope.resolved_directory, expected_a);
        assert_eq!(failures.configured[1].scope.album_handle, "c");
        assert_eq!(failures.configured[1].scope.resolved_directory, expected_c);
    }

    #[test]
    fn configured_and_default_failures_are_both_retained() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let deadwing = source.join("Deadwing");
        fs::create_dir(&deadwing).expect("create album directory");
        fs::write(deadwing.join("Lazarus.flac"), b"track").expect("write track");
        let spec = parse_spec(
            "[albums.deadwing]\ndirectory = \"Deadwing\"\n\
             [albums.in-absentia]\nname = \"In Absentia\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        let expected_configured = canonical(&deadwing);
        let expected_default = canonical(&source);
        fs::remove_dir_all(&source).expect("remove source root");

        let (failures, warnings) = expect_failed(discover_required_files(scopes, warnings));

        assert!(warnings.is_empty());
        assert_eq!(failures.configured.len(), 1);
        assert_eq!(failures.configured[0].scope.album_handle, "deadwing");
        assert_eq!(
            failures.configured[0].scope.resolved_directory,
            expected_configured
        );
        let default = failures
            .default
            .expect("default failure must stay separate");
        assert_eq!(default.scope.dependent_album_handles, ["in-absentia"]);
        assert_eq!(default.scope.resolved_traversal_root, expected_default);
    }

    #[test]
    fn parent_and_child_scopes_remain_independent_after_removing_child() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        fs::create_dir_all(source.join("parent/child")).expect("create parent and child");
        fs::write(source.join("parent/top.dat"), b"top").expect("write parent file");
        fs::write(source.join("parent/child/nested.dat"), b"nested").expect("write child file");
        let spec = parse_spec(
            "[albums.fear-of-a-blank-planet]\ndirectories = [\"parent\", \"parent/child\"]\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        assert_eq!(scopes.configured_directory_scopes.len(), 2);
        let expected_child = canonical(&source.join("parent/child"));
        fs::remove_dir_all(source.join("parent/child")).expect("remove child");

        let (failures, warnings) = expect_failed(discover_required_files(scopes, warnings));

        assert!(warnings.is_empty());
        assert!(failures.default.is_none());
        assert_eq!(
            failures.configured.len(),
            1,
            "only the removed child scope fails"
        );
        assert_eq!(
            failures.configured[0].scope.resolved_directory,
            expected_child
        );
        assert_eq!(
            failures.configured[0].scope.contributing_selectors,
            [PathBuf::from("parent/child")]
        );
    }

    #[test]
    fn equal_configured_roots_across_albums_remain_separate() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let shared = source.join("shared");
        fs::create_dir(&shared).expect("create shared directory");
        fs::write(shared.join("Lazarus.flac"), b"track").expect("write track");
        let spec = parse_spec(
            "[albums.fear-of-a-blank-planet]\ndirectory = \"shared\"\n\
             [albums.deadwing]\ndirectory = \"shared\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

        assert!(warnings.is_empty());
        assert_eq!(coverage.configured.len(), 2);
        assert_eq!(
            coverage.configured[0].scope.album_handle,
            "fear-of-a-blank-planet"
        );
        assert_eq!(coverage.configured[1].scope.album_handle, "deadwing");
        for entry in &coverage.configured {
            assert_eq!(entry.scope.resolved_directory, canonical(&shared));
            assert_eq!(entry.files, vec![canonical(&shared).join("Lazarus.flac")]);
        }
    }

    #[test]
    fn equal_configured_and_default_roots_remain_separate() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        fs::write(source.join("Blackest Eyes.flac"), b"track").expect("write track");
        let spec = parse_spec(
            "[albums.fear-of-a-blank-planet]\ndirectory = \".\"\n\
             [albums.in-absentia]\nname = \"In Absentia\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

        assert!(warnings.is_empty());
        assert_eq!(coverage.configured.len(), 1);
        assert_eq!(
            coverage.configured[0].scope.album_handle,
            "fear-of-a-blank-planet"
        );
        assert_eq!(
            coverage.configured[0].scope.resolved_directory,
            canonical(&source)
        );
        let default = coverage.default.expect("default scope must stay separate");
        assert_eq!(default.scope.dependent_album_handles, ["in-absentia"]);
        assert_eq!(default.scope.resolved_traversal_root, canonical(&source));
        assert_eq!(coverage.configured[0].files, default.files);
    }

    #[test]
    fn redundancy_warnings_survive_successful_discovery() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let stupid_dream = source.join("Stupid Dream");
        fs::create_dir(&stupid_dream).expect("create album directory");
        fs::write(stupid_dream.join("Even Less.flac"), b"track").expect("write track");
        let spec = parse_spec(
            "[albums.stupid-dream]\ndirectories = [\"Stupid Dream\", \"Stupid Dream\"]\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        assert_eq!(warnings.len(), 1);
        let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

        assert_eq!(coverage.configured.len(), 1);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].album_handle, "stupid-dream");
        assert_eq!(warnings[0].resolved_directory, canonical(&stupid_dream));
        assert_eq!(
            warnings[0].contributing_selectors,
            coverage.configured[0].scope.contributing_selectors
        );
    }

    #[test]
    fn redundancy_warnings_survive_failed_discovery() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let fear_of_a_blank_planet = source.join("Fear of a Blank Planet");
        fs::create_dir(&fear_of_a_blank_planet).expect("create album directory");
        fs::write(fear_of_a_blank_planet.join("My Ashes.flac"), b"track").expect("write track");
        let spec = parse_spec(
            "[albums.fear-of-a-blank-planet]\ndirectories = [\"Fear of a Blank Planet\", \"Fear of a Blank Planet\"]\n\
             [albums.deadwing]\ndirectory = \"Fear of a Blank Planet\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        assert_eq!(warnings.len(), 1);
        fs::remove_dir_all(&fear_of_a_blank_planet).expect("remove album");

        let (failures, warnings) = expect_failed(discover_required_files(scopes, warnings));

        assert_eq!(failures.configured.len(), 2);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].album_handle, "fear-of-a-blank-planet");
        assert_eq!(
            warnings[0].contributing_selectors.len(),
            2,
            "warning's contributing selectors must survive discovery failure"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_selector_uses_resolved_root() {
        use std::os::unix::fs::symlink;

        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let real = source.join("real");
        fs::create_dir(&real).expect("create real directory");
        fs::write(real.join("Lazarus.flac"), b"track").expect("write track");
        symlink(&real, source.join("link")).expect("create selector symlink");
        let spec = parse_spec("[albums.deadwing]\ndirectory = \"link\"\n");

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        assert_eq!(scopes.configured_directory_scopes.len(), 1);
        assert_eq!(
            scopes.configured_directory_scopes[0].resolved_directory,
            canonical(&real),
            "preparation must resolve the symlink selector"
        );

        let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

        assert!(warnings.is_empty());
        assert_eq!(coverage.configured.len(), 1);
        // The primitive rejects a symlink root, so success confirms the
        // resolved path was scanned.
        assert_eq!(
            coverage.configured[0].files,
            vec![canonical(&real).join("Lazarus.flac")]
        );
    }

    #[test]
    fn discovery_does_not_mutate_the_source_tree() {
        let sandbox = new_sandbox();
        let source = create_source(&sandbox);
        let fear_of_a_blank_planet = source.join("Fear of a Blank Planet");
        fs::create_dir(&fear_of_a_blank_planet).expect("create Fear of a Blank Planet");
        fs::write(
            fear_of_a_blank_planet.join("Anesthetize.flac"),
            b"track bytes",
        )
        .expect("write track");
        let deadwing = source.join("Deadwing");
        fs::create_dir(&deadwing).expect("create Deadwing");
        fs::write(deadwing.join("Lazarus.flac"), b"other bytes").expect("write track");
        fs::write(source.join("sentinel.txt"), b"sentinel").expect("write sentinel");
        let spec = parse_spec(
            "[albums.fear-of-a-blank-planet]\ndirectory = \"Fear of a Blank Planet\"\n\
             [albums.deadwing]\ndirectory = \"Deadwing\"\n\
             [albums.dependent]\nname = \"Dependent\"\n",
        );

        let (scopes, warnings) = expect_prepared(prepare_album_scopes(&spec, &source));
        let before = tree_fingerprint(&source);
        let outcome = discover_required_files(scopes, warnings);
        assert_eq!(
            tree_fingerprint(&source),
            before,
            "discovery must not mutate the source tree"
        );

        let (coverage, _) = expect_completed(outcome);
        assert_eq!(coverage.configured.len(), 2);
        assert!(coverage.default.is_some());
    }
}
