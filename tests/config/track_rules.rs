/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! The `[[track_rules]]` arrays: track targeting, actions, and per-group
//! validation.

use scarab::{InvalidLibraryBuildSpec, TrackAction, TrackRuleGroup, TrackTarget};

use super::{invalid, prefixed, valid};

fn track_rule(entry: &str) -> String {
    prefixed(&format!(
        "[[track_rules]]\ncollection = \"aenima\"\nrules = [{{ {entry} }}]\n"
    ))
}

fn track_rule_error(entry: &str) -> InvalidLibraryBuildSpec {
    invalid(&track_rule(entry))
}

#[test]
fn parses_track_rules() {
    let text = prefixed(
        "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ track = \"Stinkfist\", exclude = true }, { tracks = [\"Eulogy\", \"H.\"], bitrate = 64 }]\n",
    );
    let config = valid(&text);

    let groups = config.track_rules();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].collection_handle, "aenima");

    let rules = &groups[0].rules;
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].target, TrackTarget::Track("Stinkfist".into()));
    assert_eq!(rules[0].action, TrackAction::Exclude);
    assert_eq!(
        rules[1].target,
        TrackTarget::Tracks(vec!["Eulogy".into(), "H.".into()])
    );
    assert_eq!(rules[1].action, TrackAction::Bitrate(64));
}

#[test]
fn same_track_name_is_allowed_in_different_collections() {
    let text = "codec = \"opus\"\nbitrate = 128\n[collections.aenima]\nname = \"Ænima\"\nartist = \"Tool\"\n[collections.salival]\nname = \"Salival\"\nartist = \"Tool\"\n[[track_rules]]\ncollection = \"aenima\"\nrules = [{ track = \"Pushit\", bitrate = 64 }]\n[[track_rules]]\ncollection = \"salival\"\nrules = [{ track = \"Pushit\", bitrate = 64 }]\n";
    assert_eq!(valid(text).track_rules().len(), 2);
}

#[test]
fn track_entries_require_exactly_one_target() {
    for entry in [
        "track = \"Stinkfist\", tracks = [\"Eulogy\"], bitrate = 64",
        "bitrate = 64",
    ] {
        assert_eq!(
            track_rule_error(entry),
            InvalidLibraryBuildSpec::TrackTargetNotExclusive {
                collection_handle: "aenima".into()
            }
        );
    }
    assert_eq!(
        track_rule_error("tracks = [], bitrate = 64"),
        InvalidLibraryBuildSpec::EmptyTracks {
            collection_handle: "aenima".into()
        }
    );
}

#[test]
fn track_entries_require_exactly_one_action() {
    for entry in [
        "track = \"Stinkfist\"",
        "track = \"Stinkfist\", exclude = true, bitrate = 64",
        "track = \"Stinkfist\", exclude = false",
        "track = \"Stinkfist\", exclude = false, bitrate = 64",
    ] {
        assert_eq!(
            track_rule_error(entry),
            InvalidLibraryBuildSpec::TrackActionNotExclusive {
                collection_handle: "aenima".into()
            }
        );
    }
}

#[test]
fn track_rule_group_requires_at_least_one_rule() {
    let omitted = prefixed("[[track_rules]]\ncollection = \"aenima\"\n");
    let empty = prefixed("[[track_rules]]\ncollection = \"aenima\"\nrules = []\n");

    for text in [&omitted, &empty] {
        assert_eq!(
            invalid(text),
            InvalidLibraryBuildSpec::EmptyTrackRules {
                collection_handle: "aenima".into()
            }
        );
    }
}

#[test]
fn track_cannot_receive_two_rules() {
    let within_group = "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ track = \"Swamp Song\", bitrate = 64 }, { track = \"Swamp Song\", exclude = true }]\n";
    let within_group_reversed = "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ track = \"Swamp Song\", exclude = true }, { track = \"Swamp Song\", bitrate = 64 }]\n";
    let across_groups = "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ track = \"Swamp Song\", bitrate = 64 }]\n[[track_rules]]\ncollection = \"aenima\"\nrules = [{ track = \"Swamp Song\", exclude = true }]\n";
    let through_tracks_list = "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ tracks = [\"Swamp Song\", \"Eulogy\"], bitrate = 64 }, { track = \"Swamp Song\", exclude = true }]\n";
    let within_tracks_list = "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ tracks = [\"Swamp Song\", \"Swamp Song\"], bitrate = 64 }]\n";

    for body in [
        within_group,
        within_group_reversed,
        across_groups,
        through_tracks_list,
        within_tracks_list,
    ] {
        assert_eq!(
            invalid(&prefixed(body)),
            InvalidLibraryBuildSpec::DuplicateTrackRule {
                collection_handle: "aenima".into(),
                track_name: "Swamp Song".into()
            }
        );
    }
}

#[test]
fn track_rule_group_rejects_undeclared_collection_handle() {
    let track_rule = "codec = \"opus\"\nbitrate = 128\n[[track_rules]]\ncollection = \"missing\"\nrules = [{ track = \"Stinkfist\", exclude = true }]\n";
    assert_eq!(
        invalid(track_rule),
        InvalidLibraryBuildSpec::UnknownCollectionHandle {
            collection_handle: "missing".into()
        }
    );
}

#[test]
fn track_rule_group_references_directory_only_collection() {
    let text = "codec = \"opus\"\nbitrate = 128\n[collections.fs_only]\ndirectory = \"Undertow\"\n[[track_rules]]\ncollection = \"fs_only\"\nrules = [{ track = \"Intolerance\", exclude = true }]\n";

    let config = valid(text);
    let groups = config.track_rules();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].collection_handle, "fs_only");
    assert_eq!(groups[0].rules.len(), 1);
    assert_eq!(
        groups[0].rules[0].target,
        TrackTarget::Track("Intolerance".into())
    );
    assert_eq!(groups[0].rules[0].action, TrackAction::Exclude);
}

#[test]
fn track_rule_group_remains_externally_literal_constructible() {
    let text = prefixed(
        "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ track = \"Stinkfist\", exclude = true }]\n",
    );
    let config = valid(&text);

    // Downstream code can construct the group and mutate cloned rules.
    let mut rule = config.track_rules()[0].rules[0].clone();
    rule.action = TrackAction::Bitrate(96);
    let regrouped = TrackRuleGroup {
        collection_handle: "aenima".to_string(),
        rules: vec![rule],
    };

    assert_eq!(regrouped.collection_handle, "aenima");
    assert_eq!(regrouped.rules.len(), 1);
    assert_eq!(
        regrouped.rules[0].target,
        TrackTarget::Track("Stinkfist".into())
    );
    assert_eq!(regrouped.rules[0].action, TrackAction::Bitrate(96));
}

#[test]
fn inline_and_nested_track_rules_deserialize_identically() {
    let inline = prefixed(
        "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ track = \"Stinkfist\", exclude = true }, { tracks = [\"Eulogy\", \"H.\"], bitrate = 64 }]\n",
    );
    let nested = prefixed(
        "[[track_rules]]\ncollection = \"aenima\"\n[[track_rules.rules]]\ntrack = \"Stinkfist\"\nexclude = true\n[[track_rules.rules]]\ntracks = [\"Eulogy\", \"H.\"]\nbitrate = 64\n",
    );

    assert_eq!(valid(&inline), valid(&nested));
}
