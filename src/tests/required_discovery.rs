/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::*;
use crate::collection_scope::prepare_collection_scopes;
use crate::test_support::{
    SourceTreeSnapshot, TempSandbox, canonical, create_source, expect_prepared, parse_spec,
};
use std::fs;

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

#[test]
fn zero_scopes_succeed_without_scanning() {
    let sandbox = TempSandbox::new();
    let missing_root = sandbox.path().join("missing-root");
    let spec = parse_spec("");

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &missing_root));
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
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let fear_of_a_blank_planet = source.join("Fear of a Blank Planet");
    fs::create_dir(&fear_of_a_blank_planet).expect("create album directory");
    fs::write(fear_of_a_blank_planet.join("notes.txt"), b"notes").expect("write non-audio file");
    fs::write(fear_of_a_blank_planet.join("no-extension"), b"raw")
        .expect("write extensionless file");
    let spec = parse_spec(
        "[collections.fear-of-a-blank-planet]\ndirectory = \"Fear of a Blank Planet\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
    let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

    assert!(warnings.is_empty());
    assert_eq!(coverage.configured.len(), 1);
    assert!(coverage.default.is_none());
    let entry = &coverage.configured[0];
    assert_eq!(entry.scope.collection_handle, "fear-of-a-blank-planet");
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
fn shared_default_scope_retains_several_dependent_collections() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("sentinel.txt"), b"sentinel").expect("write sentinel");
    let spec = parse_spec(
        "[collections.z]\nalbum_name = \"Z\"\n\
         [collections.a]\nalbum_artist = \"A\"\n\
         [collections.q]\ntrack_artist = \"Q\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
    let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

    assert!(warnings.is_empty());
    assert!(coverage.configured.is_empty());
    let default = coverage.default.expect("default scope must stay covered");
    assert_eq!(default.scope.dependent_collection_handles, ["z", "a", "q"]);
    assert_eq!(default.scope.resolved_traversal_root, canonical(&source));
    assert_eq!(
        default.files,
        vec![default.scope.resolved_traversal_root.join("sentinel.txt")]
    );
}

#[test]
fn several_scopes_succeed_with_empty_scope_covered() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let fear_of_a_blank_planet = source.join("Fear of a Blank Planet");
    fs::create_dir(&fear_of_a_blank_planet).expect("create album with files");
    fs::write(fear_of_a_blank_planet.join("Anesthetize.flac"), b"audio").expect("write track");
    let empty = source.join("empty-album");
    fs::create_dir(&empty).expect("create empty album");
    let spec = parse_spec(
        "[collections.one]\ndirectory = \"Fear of a Blank Planet\"\n\
         [collections.two]\ndirectory = \"empty-album\"\n\
         [collections.three]\nalbum_name = \"Three\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
    let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

    assert!(warnings.is_empty());
    assert_eq!(coverage.configured.len(), 2);
    assert_eq!(coverage.configured[0].scope.collection_handle, "one");
    assert_eq!(
        coverage.configured[0].files,
        vec![canonical(&fear_of_a_blank_planet).join("Anesthetize.flac")]
    );
    assert_eq!(coverage.configured[1].scope.collection_handle, "two");
    assert_eq!(
        coverage.configured[1].scope.resolved_directory,
        canonical(&empty)
    );
    assert!(
        coverage.configured[1].files.is_empty(),
        "a successfully empty scope remains represented with no files"
    );
    let default = coverage.default.expect("default scope must stay covered");
    assert_eq!(default.scope.dependent_collection_handles, ["three"]);
}

#[test]
fn removing_two_of_three_roots_retains_both_failures() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    for name in ["a", "b", "c"] {
        let dir = source.join(name);
        fs::create_dir(&dir).expect("create album directory");
        fs::write(dir.join("Anesthetize.dat"), b"track").expect("write track");
    }
    let spec = parse_spec(
        "[collections.a]\ndirectory = \"a\"\n\
         [collections.b]\ndirectory = \"b\"\n\
         [collections.c]\ndirectory = \"c\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
    let expected_a = canonical(&source.join("a"));
    let expected_c = canonical(&source.join("c"));
    fs::remove_dir_all(source.join("a")).expect("remove A");
    fs::remove_dir_all(source.join("c")).expect("remove C");

    let (failures, warnings) = expect_failed(discover_required_files(scopes, warnings));

    assert!(warnings.is_empty());
    assert!(failures.default.is_none());
    assert_eq!(failures.configured.len(), 2);
    assert_eq!(failures.configured[0].scope.collection_handle, "a");
    assert_eq!(failures.configured[0].scope.resolved_directory, expected_a);
    assert_eq!(failures.configured[1].scope.collection_handle, "c");
    assert_eq!(failures.configured[1].scope.resolved_directory, expected_c);
}

#[test]
fn configured_and_default_failures_are_both_retained() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let deadwing = source.join("Deadwing");
    fs::create_dir(&deadwing).expect("create album directory");
    fs::write(deadwing.join("Lazarus.flac"), b"track").expect("write track");
    let spec = parse_spec(
        "[collections.deadwing]\ndirectory = \"Deadwing\"\n\
         [collections.in-absentia]\nalbum_name = \"In Absentia\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
    let expected_configured = canonical(&deadwing);
    let expected_default = canonical(&source);
    fs::remove_dir_all(&source).expect("remove source root");

    let (failures, warnings) = expect_failed(discover_required_files(scopes, warnings));

    assert!(warnings.is_empty());
    assert_eq!(failures.configured.len(), 1);
    assert_eq!(failures.configured[0].scope.collection_handle, "deadwing");
    assert_eq!(
        failures.configured[0].scope.resolved_directory,
        expected_configured
    );
    let default = failures
        .default
        .expect("default failure must stay separate");
    assert_eq!(default.scope.dependent_collection_handles, ["in-absentia"]);
    assert_eq!(default.scope.resolved_traversal_root, expected_default);
}

#[test]
fn parent_and_child_scopes_remain_independent_after_removing_child() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::create_dir_all(source.join("parent/child")).expect("create parent and child");
    fs::write(source.join("parent/top.dat"), b"top").expect("write parent file");
    fs::write(source.join("parent/child/nested.dat"), b"nested").expect("write child file");
    let spec = parse_spec(
        "[collections.fear-of-a-blank-planet]\ndirectories = [\"parent\", \"parent/child\"]\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
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
fn equal_configured_roots_across_collections_remain_separate() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let shared = source.join("shared");
    fs::create_dir(&shared).expect("create shared directory");
    fs::write(shared.join("Lazarus.flac"), b"track").expect("write track");
    let spec = parse_spec(
        "[collections.fear-of-a-blank-planet]\ndirectory = \"shared\"\n\
         [collections.deadwing]\ndirectory = \"shared\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
    let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

    assert!(warnings.is_empty());
    assert_eq!(coverage.configured.len(), 2);
    assert_eq!(
        coverage.configured[0].scope.collection_handle,
        "fear-of-a-blank-planet"
    );
    assert_eq!(coverage.configured[1].scope.collection_handle, "deadwing");
    for entry in &coverage.configured {
        assert_eq!(entry.scope.resolved_directory, canonical(&shared));
        assert_eq!(entry.files, vec![canonical(&shared).join("Lazarus.flac")]);
    }
}

#[test]
fn equal_configured_and_default_roots_remain_separate() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("Blackest Eyes.flac"), b"track").expect("write track");
    let spec = parse_spec(
        "[collections.fear-of-a-blank-planet]\ndirectory = \".\"\n\
         [collections.in-absentia]\nalbum_name = \"In Absentia\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
    let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

    assert!(warnings.is_empty());
    assert_eq!(coverage.configured.len(), 1);
    assert_eq!(
        coverage.configured[0].scope.collection_handle,
        "fear-of-a-blank-planet"
    );
    assert_eq!(
        coverage.configured[0].scope.resolved_directory,
        canonical(&source)
    );
    let default = coverage.default.expect("default scope must stay separate");
    assert_eq!(default.scope.dependent_collection_handles, ["in-absentia"]);
    assert_eq!(default.scope.resolved_traversal_root, canonical(&source));
    assert_eq!(coverage.configured[0].files, default.files);
}

#[test]
fn redundancy_warnings_survive_successful_discovery() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let stupid_dream = source.join("Stupid Dream");
    fs::create_dir(&stupid_dream).expect("create album directory");
    fs::write(stupid_dream.join("Even Less.flac"), b"track").expect("write track");
    let spec = parse_spec(
        "[collections.stupid-dream]\ndirectories = [\"Stupid Dream\", \"Stupid Dream\"]\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
    assert_eq!(warnings.len(), 1);
    let (coverage, warnings) = expect_completed(discover_required_files(scopes, warnings));

    assert_eq!(coverage.configured.len(), 1);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].collection_handle, "stupid-dream");
    assert_eq!(warnings[0].resolved_directory, canonical(&stupid_dream));
    assert_eq!(
        warnings[0].contributing_selectors,
        coverage.configured[0].scope.contributing_selectors
    );
}

#[test]
fn redundancy_warnings_survive_failed_discovery() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let fear_of_a_blank_planet = source.join("Fear of a Blank Planet");
    fs::create_dir(&fear_of_a_blank_planet).expect("create album directory");
    fs::write(fear_of_a_blank_planet.join("My Ashes.flac"), b"track").expect("write track");
    let spec = parse_spec(
        "[collections.fear-of-a-blank-planet]\ndirectories = [\"Fear of a Blank Planet\", \"Fear of a Blank Planet\"]\n\
         [collections.deadwing]\ndirectory = \"Fear of a Blank Planet\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
    assert_eq!(warnings.len(), 1);
    fs::remove_dir_all(&fear_of_a_blank_planet).expect("remove album");

    let (failures, warnings) = expect_failed(discover_required_files(scopes, warnings));

    assert_eq!(failures.configured.len(), 2);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].collection_handle, "fear-of-a-blank-planet");
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

    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let real = source.join("real");
    fs::create_dir(&real).expect("create real directory");
    fs::write(real.join("Lazarus.flac"), b"track").expect("write track");
    symlink(&real, source.join("link")).expect("create selector symlink");
    let spec = parse_spec("[collections.deadwing]\ndirectory = \"link\"\n");

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
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
    let sandbox = TempSandbox::new();
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
        "[collections.fear-of-a-blank-planet]\ndirectory = \"Fear of a Blank Planet\"\n\
         [collections.deadwing]\ndirectory = \"Deadwing\"\n\
         [collections.dependent]\nalbum_name = \"Dependent\"\n",
    );

    let (scopes, warnings) = expect_prepared(prepare_collection_scopes(&spec, &source));
    let snapshot = SourceTreeSnapshot::capture(&source);
    let outcome = discover_required_files(scopes, warnings);
    snapshot.assert_unchanged();

    let (coverage, _) = expect_completed(outcome);
    assert_eq!(coverage.configured.len(), 2);
    assert!(coverage.default.is_some());
}
