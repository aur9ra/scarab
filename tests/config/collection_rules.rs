/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! The `[[collection_rules]]` arrays: collection-handle targeting and validation.

use std::path::PathBuf;

use scarab::InvalidLibraryBuildSpec;

use super::{collection_config, invalid, prefixed, rejects_toml, valid};

#[test]
fn parses_collection_rule_with_one_collection() {
    let config = valid(&prefixed(
        "[[collection_rules]]\ncollections = [\"aenima\"]\nbitrate = 96\n",
    ));

    let rules = config.collection_rules();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].collection_handles, ["aenima"]);
    assert_eq!(rules[0].bitrate, 96);
}

#[test]
fn collection_rule_preserves_listed_handles() {
    let text = "codec = \"opus\"\nbitrate = 128\n[collections.aenima]\nname = \"Ænima\"\nartist = \"Tool\"\n[collections.lateralus]\nname = \"Lateralus\"\nartist = \"Tool\"\n[[collection_rules]]\ncollections = [\"aenima\", \"lateralus\"]\nbitrate = 96\n";

    let config = valid(text);
    let rules = config.collection_rules();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].collection_handles, ["aenima", "lateralus"]);
    assert_eq!(rules[0].bitrate, 96);
}

#[test]
fn collection_rule_rejects_singular_collection_alias() {
    rejects_toml(&prefixed(
        "[[collection_rules]]\ncollection = \"aenima\"\nbitrate = 96\n",
    ));
    rejects_toml(&prefixed(
        "[[collection_rules]]\ncollections = [\"aenima\"]\ncollection = \"aenima\"\nbitrate = 96\n",
    ));
}

#[test]
fn collection_rule_requires_nonempty_targets() {
    let text = prefixed("[[collection_rules]]\ncollections = []\nbitrate = 96\n");
    assert_eq!(
        invalid(&text),
        InvalidLibraryBuildSpec::EmptyCollectionRuleTargets
    );
}

#[test]
fn collection_rule_rejects_duplicate_handles_across_rules() {
    let first_then_second = prefixed(
        "[[collection_rules]]\ncollections = [\"aenima\"]\nbitrate = 96\n[[collection_rules]]\ncollections = [\"aenima\"]\nbitrate = 64\n",
    );
    let second_then_first = prefixed(
        "[[collection_rules]]\ncollections = [\"aenima\"]\nbitrate = 64\n[[collection_rules]]\ncollections = [\"aenima\"]\nbitrate = 96\n",
    );

    for text in [&first_then_second, &second_then_first] {
        assert_eq!(
            invalid(text),
            InvalidLibraryBuildSpec::DuplicateCollectionRule {
                collection_handle: "aenima".into()
            }
        );
    }

    let partial_overlap = "codec = \"opus\"\nbitrate = 128\n[collections.aenima]\nname = \"Ænima\"\nartist = \"Tool\"\n[collections.lateralus]\nname = \"Lateralus\"\nartist = \"Tool\"\n[[collection_rules]]\ncollections = [\"aenima\", \"lateralus\"]\nbitrate = 96\n[[collection_rules]]\ncollections = [\"lateralus\"]\nbitrate = 64\n";
    assert_eq!(
        invalid(partial_overlap),
        InvalidLibraryBuildSpec::DuplicateCollectionRule {
            collection_handle: "lateralus".into()
        }
    );
}

#[test]
fn collection_rule_rejects_duplicate_handles_within_one_rule() {
    let text =
        prefixed("[[collection_rules]]\ncollections = [\"aenima\", \"aenima\"]\nbitrate = 96\n");
    assert_eq!(
        invalid(&text),
        InvalidLibraryBuildSpec::DuplicateCollectionRule {
            collection_handle: "aenima".into()
        }
    );
}

#[test]
fn collection_rule_rejects_undeclared_collection_handle() {
    let collection_rule = "codec = \"opus\"\nbitrate = 128\n[[collection_rules]]\ncollections = [\"missing\"]\nbitrate = 96\n";
    assert_eq!(
        invalid(collection_rule),
        InvalidLibraryBuildSpec::UnknownCollectionHandle {
            collection_handle: "missing".into()
        }
    );
}

#[test]
fn collection_rule_can_reference_directory_only_collection() {
    let text = collection_config("fs_only", "directory = \"Undertow\"\n")
        + "[[collection_rules]]\ncollections = [\"fs_only\"]\nbitrate = 96\n";

    let config = valid(&text);
    let declaration = config
        .collection_declaration("fs_only")
        .expect("fs_only must be declared");

    assert_eq!(declaration.name, None);
    assert_eq!(declaration.artist, None);
    assert_eq!(
        declaration.directories,
        Some(vec![PathBuf::from("Undertow")])
    );

    let rules = config.collection_rules();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].collection_handles, ["fs_only"]);
    assert_eq!(rules[0].bitrate, 96);
}
