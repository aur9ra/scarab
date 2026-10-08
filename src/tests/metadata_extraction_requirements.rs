/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::*;
use crate::collection_membership::{MetadataExtractionOutcomes, evaluate_collection_membership};
use crate::observed_source_file_inventory::build_observed_source_file_inventory;
use crate::test_support::{TempSandbox, canonical, create_source, parse_spec};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

fn write_file(source: &Path, name: &str) -> PathBuf {
    let path = source.join(name);
    fs::create_dir_all(path.parent().expect("file has parent")).expect("create directories");
    fs::write(&path, b"fixture").expect("write disposable fixture");
    path
}

/// Set equality ignores ordering, while length independently detects duplicates.
fn assert_requirements(
    spec: &LibraryBuildSpec,
    inventory: &ObservedSourceFileInventory,
    expected: &[PathBuf],
) {
    let paths = required_metadata_paths(spec, inventory);
    assert_eq!(paths.len(), expected.len(), "unique requirement count");
    assert_eq!(
        paths.iter().copied().collect::<HashSet<&Path>>(),
        expected
            .iter()
            .map(PathBuf::as_path)
            .collect::<HashSet<&Path>>()
    );
    for path in paths {
        let retained = inventory
            .files()
            .find(|file| file.path() == path)
            .expect("returned path belongs to inventory")
            .path();
        assert_eq!(path.as_os_str(), retained.as_os_str());
        assert!(std::ptr::eq(path, retained), "borrow the retained path");
    }
}

#[test]
fn each_metadata_family_and_empty_string_require_configured_domain_metadata() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let track = write_file(&source, "Album/track.flac");
    for selector in [
        "album_name = \"Album\"",
        "album_artist = \"Artist\"",
        "track_artist = \"Artist\"",
        "album_name = \"\"",
        "album_artist = \"\"",
        "track_artist = \"\"",
    ] {
        let spec = parse_spec(&format!(
            "[collections.meta]\ndirectory = \"Album\"\n{selector}\n"
        ));
        let success = build_observed_source_file_inventory(&spec, &source).unwrap();
        assert_requirements(&spec, success.inventory(), &[canonical(&track)]);
    }
}

#[test]
fn configured_metadata_domain_does_not_expand_to_unrelated_scopes_or_root() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let track = write_file(&source, "Album/track.flac");
    write_file(&source, "Other/unrelated.flac");
    write_file(&source, "root.flac");
    let spec = parse_spec(
        "[collections.meta]\ndirectory = \"Album\"\nalbum_name = \"Keep\"\n\
         [collections.other]\ndirectory = \"Other\"\n",
    );
    let success = build_observed_source_file_inventory(&spec, &source).unwrap();
    assert_eq!(success.inventory().files().count(), 2);
    assert_requirements(&spec, success.inventory(), &[canonical(&track)]);
}

#[test]
fn metadata_only_uses_shared_default_scope_and_existing_audio_classification() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let names = ["track.flac", "upper.FLAC", "mixed.FlaC", ".hidden.flac"];
    let expected: Vec<PathBuf> = names
        .iter()
        .map(|name| canonical(&write_file(&source, name)))
        .collect();
    for name in ["cover.jpg", "track.wav", "track.flac.bak", "bare", ".flac"] {
        write_file(&source, name);
    }
    let spec = parse_spec(
        "[collections.album]\nalbum_name = \"Keep\"\n\
         [collections.artist]\ntrack_artist = \"Artist\"\n",
    );
    let success = build_observed_source_file_inventory(&spec, &source).unwrap();
    assert_eq!(success.inventory().covered_scopes().len(), 1);
    assert_requirements(&spec, success.inventory(), &expected);
}

#[test]
fn overlapping_directory_scopes_emit_once_if_any_requires_metadata() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let track = write_file(&source, "Album/track.flac");
    for (selectors, expected_count) in [
        (["", ""], 0),
        (["", "album_name = \"Keep\"\n"], 1),
        (["album_name = \"Keep\"\n", ""], 1),
        (
            ["album_name = \"Keep\"\n", "track_artist = \"Artist\"\n"],
            1,
        ),
    ] {
        let spec = parse_spec(&format!(
            "[collections.one]\ndirectory = \"Album\"\n{}\
             [collections.two]\ndirectory = \"Album\"\n{}",
            selectors[0], selectors[1]
        ));
        let success = build_observed_source_file_inventory(&spec, &source).unwrap();
        assert_eq!(
            success
                .inventory()
                .files()
                .next()
                .unwrap()
                .reporting_scopes()
                .count(),
            2
        );
        let expected = vec![canonical(&track); expected_count];
        assert_requirements(&spec, success.inventory(), &expected);
    }
}

#[test]
fn multiple_scopes_for_one_collection_emit_once() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let track = write_file(&source, "Album/Nested/track.flac");
    let spec = parse_spec(
        "[collections.meta]\ndirectories = [\"Album\", \"Album/Nested\"]\n\
         album_artist = \"Artist\"\n",
    );
    let success = build_observed_source_file_inventory(&spec, &source).unwrap();
    assert_eq!(
        success
            .inventory()
            .files()
            .next()
            .unwrap()
            .reporting_scopes()
            .count(),
        2
    );
    assert_requirements(&spec, success.inventory(), &[canonical(&track)]);
}

#[test]
fn configured_and_default_overlap_emit_once() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let track = write_file(&source, "Album/track.flac");
    let root = write_file(&source, "root.flac");
    let spec = parse_spec(
        "[collections.configured]\ndirectory = \"Album\"\nalbum_name = \"Keep\"\n\
         [collections.default]\nalbum_artist = \"Artist\"\n",
    );
    let success = build_observed_source_file_inventory(&spec, &source).unwrap();
    assert_requirements(
        &spec,
        success.inventory(),
        &[canonical(&track), canonical(&root)],
    );
}

#[test]
fn empty_domains_have_no_requirements() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::create_dir(source.join("Empty")).unwrap();
    for declarations in [
        "[collections.meta]\ndirectory = \"Empty\"\nalbum_name = \"Keep\"\n",
        "[collections.meta]\nalbum_artist = \"Artist\"\n",
        "[collections.dir]\ndirectory = \"Empty\"\n",
        "",
    ] {
        let spec = parse_spec(declarations);
        let success = build_observed_source_file_inventory(&spec, &source).unwrap();
        assert_requirements(&spec, success.inventory(), &[]);
    }
}

#[test]
fn removed_file_still_requires_metadata_from_inventory_observation() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let track = write_file(&source, "track.flac");
    let retained = canonical(&track);
    let spec = parse_spec("[collections.meta]\nalbum_name = \"Keep\"\n");
    let success = build_observed_source_file_inventory(&spec, &source).unwrap();
    fs::remove_file(&track).expect("remove disposable fixture after inventory construction");
    assert!(!track.exists());
    assert_requirements(&spec, success.inventory(), &[retained]);
}

#[test]
fn distinct_hard_link_pathnames_are_not_collapsed() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let first = write_file(&source, "first.flac");
    let second = source.join("second.flac");
    fs::hard_link(&first, &second).expect("create physical-file alias in disposable fixture");
    let spec = parse_spec("[collections.meta]\ntrack_artist = \"Artist\"\n");
    let success = build_observed_source_file_inventory(&spec, &source).unwrap();
    assert_ne!(first, second);
    assert_requirements(
        &spec,
        success.inventory(),
        &[canonical(&first), canonical(&second)],
    );
}

#[cfg(unix)]
#[test]
fn non_utf8_basename_is_borrowed_unchanged() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let track = source.join(OsString::from_vec(b"track-\xff.FlaC".to_vec()));
    fs::write(&track, b"fixture").unwrap();
    let spec = parse_spec("[collections.meta]\nalbum_name = \"Keep\"\n");
    let success = build_observed_source_file_inventory(&spec, &source).unwrap();
    assert!(track.to_str().is_none());
    assert_requirements(&spec, success.inventory(), &[canonical(&track)]);
}

#[test]
fn requirements_equal_unresolved_membership_union_for_mixed_configurations() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let meta = write_file(&source, "Meta/track.flac");
    let shared = write_file(&source, "Meta/Nested/shared.FlaC");
    let other = write_file(&source, "Other/only.flac");
    let root = write_file(&source, "root.flac");
    let external = write_file(sandbox.path(), "external/outside.flac");
    write_file(&source, "Meta/cover.jpg");
    fs::create_dir(source.join("Empty")).unwrap();

    for include_default in [false, true] {
        let mut declarations = String::from(
            "[collections.meta]\ndirectories = [\"Meta\", \"Meta/Nested\"]\nalbum_name = \"\"\n\
             [collections.shared]\ndirectory = \"Meta/Nested\"\ntrack_artist = \"Artist\"\n\
             [collections.dir]\ndirectory = \"Meta\"\n\
             [collections.other]\ndirectory = \"Other\"\n\
             [collections.external]\ndirectory = \"../external\"\nalbum_artist = \"Artist\"\n\
             [collections.empty]\ndirectory = \"Empty\"\nalbum_name = \"Empty\"\n",
        );
        let mut expected = vec![canonical(&meta), canonical(&shared), canonical(&external)];
        if include_default {
            declarations.push_str(
                "[collections.default]\nalbum_artist = \"Artist\"\n\
                 [collections.default_two]\ntrack_artist = \"Artist\"\n",
            );
            expected.extend([canonical(&other), canonical(&root)]);
        }
        let spec = parse_spec(&declarations);
        let success = build_observed_source_file_inventory(&spec, &source).unwrap();
        let inventory = success.inventory();
        assert_requirements(&spec, inventory, &expected);

        let empty_outcomes: MetadataExtractionOutcomes<()> = Default::default();
        let membership = evaluate_collection_membership(&spec, inventory, &empty_outcomes);
        let unresolved: HashSet<&Path> = membership
            .collections()
            .iter()
            .flat_map(|collection| collection.unresolved().iter().map(|file| file.path()))
            .collect();
        let required = required_metadata_paths(&spec, inventory);
        assert_eq!(required.len(), expected.len());
        assert_eq!(required.into_iter().collect::<HashSet<&Path>>(), unresolved);
    }
}
