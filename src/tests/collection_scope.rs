/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::*;
use crate::test_support::{
    SourceTreeSnapshot, TempSandbox, canonical, create_source, expect_prepared, parse_spec,
};
use std::ffi::OsStr;

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

fn expect_failed(
    preparation: CollectionScopePreparation,
) -> (
    Vec<ConfiguredSelectorFailure>,
    Option<DefaultSourceRootFailure>,
    Vec<RedundancyWarning>,
) {
    match preparation {
        CollectionScopePreparation::Failed {
            configured_failures,
            default_source_root_failure,
            warnings,
        } => (configured_failures, default_source_root_failure, warnings),
        CollectionScopePreparation::Prepared { scopes, warnings } => {
            panic!("expected failure, got prepared scopes {scopes:?} and warnings {warnings:?}")
        }
    }
}

#[test]
fn zero_collections_succeed_without_inspecting_a_nonexistent_root() {
    let sandbox = TempSandbox::new();
    let missing_root = sandbox.path().join("missing-root");
    let spec = parse_spec("");

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &missing_root));

    assert!(scopes.configured_directory_scopes.is_empty());
    assert!(scopes.default_source_root_scope.is_none());
    assert!(warnings.is_empty());
}

#[test]
fn configured_only_collections_prepare_no_default_source_root() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::create_dir(source.join("Lateralus")).expect("create album directory");
    let spec = parse_spec("[collections.tool]\ndirectory = \"Lateralus\"\n");

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));

    assert!(scopes.default_source_root_scope.is_none());
    assert!(warnings.is_empty());
    assert_eq!(scopes.configured_directory_scopes.len(), 1);
    let scope = &scopes.configured_directory_scopes[0];
    assert_eq!(scope.collection_handle, "tool");
    assert_eq!(
        scope.resolved_directory,
        canonical(&source.join("Lateralus"))
    );
    assert_eq!(scope.contributing_selectors.len(), 1);
    assert_spelling(&scope.contributing_selectors[0], "Lateralus");
}

#[test]
fn absolute_configured_selector_prepares_with_an_unrelated_nonexistent_root() {
    let sandbox = TempSandbox::new();
    let target = sandbox.path().join("absolute-target");
    fs::create_dir(&target).expect("create target");
    let missing_root = sandbox.path().join("missing-root");
    // Checking this missing root would fail, so success means it was skipped.
    let spec = parse_spec(&format!(
        "[collections.tool]\ndirectory = {}\n",
        toml_string(&target)
    ));

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &missing_root));

    assert!(scopes.default_source_root_scope.is_none());
    assert!(warnings.is_empty());
    assert_eq!(scopes.configured_directory_scopes.len(), 1);
    assert_eq!(
        scopes.configured_directory_scopes[0].resolved_directory,
        canonical(&target)
    );
}

#[test]
fn one_dependent_collection_prepares_the_default_source_root() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let spec = parse_spec("[collections.tool]\nalbum_name = \"Tool\"\n");

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));

    assert!(scopes.configured_directory_scopes.is_empty());
    assert!(warnings.is_empty());
    let default_scope = scopes
        .default_source_root_scope
        .expect("the dependent collection must produce a default source-root scope");
    assert_eq!(
        default_scope.original_source_root.as_os_str(),
        source.as_os_str()
    );
    assert_eq!(default_scope.resolved_traversal_root, canonical(&source));
    assert_eq!(default_scope.dependent_collection_handles, ["tool"]);
}

#[test]
fn several_dependent_collections_retain_declaration_order() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::create_dir(source.join("Lateralus")).expect("create album directory");
    let spec = parse_spec(
        "[collections.z]\nalbum_name = \"Z\"\n\
         [collections.a]\nalbum_artist = \"A\"\n\
         [collections.m]\ndirectory = \"Lateralus\"\n\
         [collections.q]\ntrack_artist = \"Q\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));

    assert!(warnings.is_empty());
    assert_eq!(scopes.configured_directory_scopes.len(), 1);
    assert_eq!(scopes.configured_directory_scopes[0].collection_handle, "m");
    let default_scope = scopes
        .default_source_root_scope
        .expect("dependent collections must produce a default source-root scope");
    assert_eq!(default_scope.dependent_collection_handles, ["z", "a", "q"]);
    assert_eq!(default_scope.resolved_traversal_root, canonical(&source));
}

#[test]
fn default_source_root_scope_retains_the_exact_original_spelling() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let spelling = source.join(".");
    let spec = parse_spec("[collections.tool]\nalbum_name = \"Tool\"\n");

    let (scopes, _) = expect_prepared(prepare_collection_scopes(&spec, &spelling));

    let default_scope = scopes
        .default_source_root_scope
        .expect("dependent collection must produce a default source-root scope");
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
    let spec = parse_spec("[collections.tool]\nalbum_name = \"Tool\"\n");

    let (configured_failures, default_failure, warnings) =
        expect_failed(prepare_collection_scopes(&spec, Path::new("")));

    assert!(configured_failures.is_empty());
    assert!(warnings.is_empty());
    let failure = default_failure.expect("the dependent collection must fail with the empty root");
    assert!(matches!(
        failure.kind,
        DefaultSourceRootFailureKind::EmptyInput
    ));
    assert_eq!(failure.original_source_root.as_os_str(), OsStr::new(""));
    assert_eq!(failure.dependent_collection_handles, ["tool"]);
}
#[test]
fn whitespace_only_default_source_root_is_not_empty_input() {
    let spec = parse_spec("[collections.tool]\nalbum_name = \"Tool\"\n");

    // Resolution depends on the test's working directory; only check that
    // whitespace is not classified as empty.
    match prepare_collection_scopes(&spec, Path::new("   ")) {
        CollectionScopePreparation::Prepared { scopes, warnings } => {
            assert!(warnings.is_empty());
            let default_scope = scopes
                .default_source_root_scope
                .expect("the dependent collection must produce a default source-root scope");
            assert_eq!(
                default_scope.original_source_root.as_os_str(),
                OsStr::new("   ")
            );
        }
        CollectionScopePreparation::Failed {
            default_source_root_failure,
            ..
        } => {
            let failure = default_source_root_failure
                .expect("the dependent collection must observe the whitespace root");
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
    let sandbox = TempSandbox::new();
    let missing_root = sandbox.path().join("missing-root");
    let spec = parse_spec("[collections.tool]\nalbum_name = \"Tool\"\n");

    let (configured_failures, default_failure, warnings) =
        expect_failed(prepare_collection_scopes(&spec, &missing_root));

    assert!(configured_failures.is_empty());
    assert!(warnings.is_empty());
    let failure = default_failure.expect("the dependent collection must fail resolving the root");
    assert_eq!(
        failure.original_source_root.as_os_str(),
        missing_root.as_os_str()
    );
    assert_eq!(failure.dependent_collection_handles, ["tool"]);
    match failure.kind {
        DefaultSourceRootFailureKind::ResolutionFailed { error } => {
            assert_native_canonicalize_error(&error, &missing_root);
        }
        other => panic!("missing root must fail through native resolution, got {other:?}"),
    }
}

#[test]
fn ordinary_file_default_source_root_reports_resolved_target_not_directory() {
    let sandbox = TempSandbox::new();
    let file = sandbox.path().join("source-file");
    fs::write(&file, b"not a directory").expect("write file");
    let spec = parse_spec("[collections.tool]\nalbum_name = \"Tool\"\n");

    let (_, default_failure, _) = expect_failed(prepare_collection_scopes(&spec, &file));

    let failure = default_failure.expect("the dependent collection must fail inspecting the file");
    match failure.kind {
        DefaultSourceRootFailureKind::ResolvedTargetNotDirectory { resolved_path } => {
            assert_eq!(resolved_path, canonical(&file));
        }
        other => panic!("an ordinary file must be reported as not a directory, got {other:?}"),
    }
}

#[test]
fn identical_successful_occurrences_group_with_one_warning() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Lateralus");
    fs::create_dir(&album).expect("create album directory");
    let spec = parse_spec("[collections.tool]\ndirectories = [\"Lateralus\", \"Lateralus\"]\n");

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));

    assert_eq!(scopes.configured_directory_scopes.len(), 1);
    let scope = &scopes.configured_directory_scopes[0];
    assert_eq!(scope.collection_handle, "tool");
    assert_eq!(scope.resolved_directory, canonical(&album));
    assert_eq!(scope.contributing_selectors.len(), 2);
    assert_spelling(&scope.contributing_selectors[0], "Lateralus");
    assert_spelling(&scope.contributing_selectors[1], "Lateralus");

    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].collection_handle, "tool");
    assert_eq!(warnings[0].resolved_directory, canonical(&album));
    assert_eq!(
        warnings[0].contributing_selectors,
        scope.contributing_selectors
    );
}

#[test]
fn three_equal_root_occurrences_retain_all_spellings_with_one_warning() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Lateralus");
    fs::create_dir(&album).expect("create album directory");
    let spec = parse_spec(
        "[collections.tool]\ndirectories = [\"Lateralus\", \"./Lateralus\", \"Lateralus\"]\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));

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
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::create_dir(source.join("album-one")).expect("create album-one");
    fs::create_dir(source.join("album-two")).expect("create album-two");
    // Reverse lexical order checks that groups follow declaration order.
    let spec = parse_spec("[collections.tool]\ndirectories = [\"album-two\", \"album-one\"]\n");

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));

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
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::create_dir_all(source.join("parent/child")).expect("create parent and child");

    for selectors in [["parent", "parent/child"], ["parent/child", "parent"]] {
        let spec = parse_spec(&format!(
            "[collections.tool]\ndirectories = [\"{}\", \"{}\"]\n",
            selectors[0], selectors[1]
        ));

        let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));

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
fn equal_configured_roots_across_collections_remain_separate() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Lateralus");
    fs::create_dir(&album).expect("create album directory");
    let spec = parse_spec(
        "[collections.one]\ndirectory = \"Lateralus\"\n\
         [collections.two]\ndirectory = \"Lateralus\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));

    assert!(
        warnings.is_empty(),
        "one contributor per collection produces no warning"
    );
    assert_eq!(scopes.configured_directory_scopes.len(), 2);
    assert_eq!(
        scopes.configured_directory_scopes[0].collection_handle,
        "one"
    );
    assert_eq!(
        scopes.configured_directory_scopes[1].collection_handle,
        "two"
    );
    for scope in &scopes.configured_directory_scopes {
        assert_eq!(scope.resolved_directory, canonical(&album));
        assert_eq!(scope.contributing_selectors.len(), 1);
    }
}

#[test]
fn configured_and_default_equal_roots_stay_separate_without_warning() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let spec = parse_spec(
        "[collections.one]\ndirectory = \".\"\n\
         [collections.two]\nalbum_name = \"Two\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));

    assert!(
        warnings.is_empty(),
        "equal configured and default roots produce no redundancy warning"
    );
    assert_eq!(scopes.configured_directory_scopes.len(), 1);
    let configured = &scopes.configured_directory_scopes[0];
    assert_eq!(configured.collection_handle, "one");
    assert_eq!(configured.resolved_directory, canonical(&source));
    let default_scope = scopes
        .default_source_root_scope
        .expect("the dependent collection must produce a default source-root scope");
    assert_eq!(default_scope.dependent_collection_handles, ["two"]);
    assert_eq!(default_scope.resolved_traversal_root, canonical(&source));
}

#[test]
fn repeated_missing_selectors_produce_repeated_ordered_failures() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let spec = parse_spec("[collections.tool]\ndirectories = [\"missing\", \"missing\"]\n");

    let (configured_failures, default_failure, warnings) =
        expect_failed(prepare_collection_scopes(&spec, &source));

    assert!(
        default_failure.is_none(),
        "a configured collection is not dependent on the default root"
    );
    assert!(warnings.is_empty());
    assert_eq!(
        configured_failures.len(),
        2,
        "repeated failures stay separate"
    );
    for failure in &configured_failures {
        assert_eq!(failure.collection_handle, "tool");
        assert_spelling(&failure.configured_selector, "missing");
        assert_native_canonicalize_error(&failure.error, &source.join("missing"));
    }
}

#[test]
fn distinct_missing_selectors_retain_occurrence_order() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let spec = parse_spec("[collections.tool]\ndirectories = [\"missing-b\", \"missing-a\"]\n");

    let (configured_failures, default_failure, warnings) =
        expect_failed(prepare_collection_scopes(&spec, &source));

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
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::create_dir(source.join("album-one")).expect("create album-one");
    fs::create_dir(source.join("album-two")).expect("create album-two");
    let spec = parse_spec(
        "[collections.one]\ndirectories = [\"album-one\", \"missing\", \"album-one\"]\n\
         [collections.two]\ndirectories = [\"missing\"]\n\
         [collections.three]\ndirectories = [\"album-two\", \"album-two\"]\n",
    );

    // Failure returns no scopes but keeps diagnostics and warnings.
    let (configured_failures, default_failure, warnings) =
        expect_failed(prepare_collection_scopes(&spec, &source));

    assert!(default_failure.is_none());
    assert_eq!(configured_failures.len(), 2);
    assert_eq!(configured_failures[0].collection_handle, "one");
    assert_spelling(&configured_failures[0].configured_selector, "missing");
    assert_eq!(configured_failures[1].collection_handle, "two");
    assert_spelling(&configured_failures[1].configured_selector, "missing");

    assert_eq!(warnings.len(), 2, "successful groups still warn on failure");
    assert_eq!(warnings[0].collection_handle, "one");
    assert_eq!(
        warnings[0].resolved_directory,
        canonical(&source.join("album-one"))
    );
    assert_eq!(warnings[0].contributing_selectors.len(), 2);
    assert_eq!(warnings[1].collection_handle, "three");
    assert_eq!(
        warnings[1].resolved_directory,
        canonical(&source.join("album-two"))
    );
    assert_eq!(warnings[1].contributing_selectors.len(), 2);
}

#[test]
fn failing_configured_and_default_mechanisms_retain_both_categories() {
    let sandbox = TempSandbox::new();
    let missing_root = sandbox.path().join("missing-root");
    let missing_album = sandbox.path().join("missing-album");
    let spec = parse_spec(&format!(
        "[collections.one]\ndirectory = {}\n\
         [collections.two]\nalbum_name = \"Two\"\n",
        toml_string(&missing_album)
    ));

    let (configured_failures, default_failure, warnings) =
        expect_failed(prepare_collection_scopes(&spec, &missing_root));

    assert!(warnings.is_empty());
    assert_eq!(configured_failures.len(), 1);
    assert_eq!(configured_failures[0].collection_handle, "one");
    assert_native_canonicalize_error(&configured_failures[0].error, &missing_album);

    let failure = default_failure.expect("the dependent collection must fail on the missing root");
    assert_eq!(failure.dependent_collection_handles, ["two"]);
    assert!(matches!(
        failure.kind,
        DefaultSourceRootFailureKind::ResolutionFailed { .. }
    ));
}

#[test]
fn preparation_does_not_mutate_the_source_tree() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::create_dir(source.join("album-one")).expect("create album-one");
    fs::write(source.join("album-one/01-track.flac"), b"track bytes").expect("write track");
    fs::create_dir(source.join("album-two")).expect("create album-two");
    fs::write(source.join("album-two/02-track.flac"), b"other bytes").expect("write track");
    fs::write(source.join("sentinel.txt"), b"sentinel").expect("write sentinel");
    let spec = parse_spec(
        "[collections.one]\ndirectory = \"album-one\"\n\
         [collections.one-again]\ndirectory = \"album-one\"\n\
         [collections.two]\ndirectory = \"album-two\"\n\
         [collections.gone]\ndirectory = \"no-such-album\"\n\
         [collections.dependent]\nalbum_name = \"Dependent\"\n",
    );

    let snapshot = SourceTreeSnapshot::capture(&source);
    let preparation = prepare_collection_scopes(&spec, &source);
    snapshot.assert_unchanged();

    let (configured_failures, default_failure, _) = expect_failed(preparation);
    assert_eq!(configured_failures.len(), 1);
    assert!(default_failure.is_none());
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
        let dependent_collection_handles = vec!["tool".to_owned()];

        // Reject a drive-relative root before filesystem resolution.
        let kind = prepare_default_source_root(Path::new("C:"), &dependent_collection_handles)
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
