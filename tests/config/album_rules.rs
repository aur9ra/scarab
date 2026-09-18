/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! The `[[album_rules]]` arrays: handle targeting and per-rule validation.

use std::path::PathBuf;

use scarab::{Album, AlbumRule, InvalidLibraryBuildSpec};

use super::{album_config, invalid, prefixed, rejects_toml, valid};

#[test]
fn parses_album_rule_with_one_album() {
    let config = valid(&prefixed(
        "[[album_rules]]\nalbums = [\"aenima\"]\nbitrate = 96\n",
    ));
    assert_eq!(
        config.album_rules(),
        &[AlbumRule {
            album_handles: vec!["aenima".into()],
            bitrate: 96,
        }]
    );
}

#[test]
fn album_rule_applies_to_every_listed_album() {
    let text = "codec = \"opus\"\nbitrate = 128\n[albums.aenima]\nname = \"Ænima\"\nartist = \"Tool\"\n[albums.lateralus]\nname = \"Lateralus\"\nartist = \"Tool\"\n[[album_rules]]\nalbums = [\"aenima\", \"lateralus\"]\nbitrate = 96\n";
    assert_eq!(
        valid(text).album_rules(),
        &[AlbumRule {
            album_handles: vec!["aenima".into(), "lateralus".into()],
            bitrate: 96,
        }]
    );
}

#[test]
fn album_rule_rejects_singular_album_alias() {
    rejects_toml(&prefixed(
        "[[album_rules]]\nalbum = \"aenima\"\nbitrate = 96\n",
    ));
    rejects_toml(&prefixed(
        "[[album_rules]]\nalbums = [\"aenima\"]\nalbum = \"aenima\"\nbitrate = 96\n",
    ));
}

#[test]
fn album_rule_requires_a_non_empty_album_list() {
    let text = prefixed("[[album_rules]]\nalbums = []\nbitrate = 96\n");
    assert_eq!(invalid(&text), InvalidLibraryBuildSpec::EmptyAlbums);
}

#[test]
fn album_rule_rejects_duplicate_handles_across_rules() {
    let first_then_second = prefixed(
        "[[album_rules]]\nalbums = [\"aenima\"]\nbitrate = 96\n[[album_rules]]\nalbums = [\"aenima\"]\nbitrate = 64\n",
    );
    let second_then_first = prefixed(
        "[[album_rules]]\nalbums = [\"aenima\"]\nbitrate = 64\n[[album_rules]]\nalbums = [\"aenima\"]\nbitrate = 96\n",
    );

    for text in [&first_then_second, &second_then_first] {
        assert_eq!(
            invalid(text),
            InvalidLibraryBuildSpec::DuplicateAlbumRule {
                album_handle: "aenima".into()
            }
        );
    }

    let partial_overlap = "codec = \"opus\"\nbitrate = 128\n[albums.aenima]\nname = \"Ænima\"\nartist = \"Tool\"\n[albums.lateralus]\nname = \"Lateralus\"\nartist = \"Tool\"\n[[album_rules]]\nalbums = [\"aenima\", \"lateralus\"]\nbitrate = 96\n[[album_rules]]\nalbums = [\"lateralus\"]\nbitrate = 64\n";
    assert_eq!(
        invalid(partial_overlap),
        InvalidLibraryBuildSpec::DuplicateAlbumRule {
            album_handle: "lateralus".into()
        }
    );
}

#[test]
fn album_rule_rejects_duplicate_handles_within_one_rule() {
    let text = prefixed("[[album_rules]]\nalbums = [\"aenima\", \"aenima\"]\nbitrate = 96\n");
    assert_eq!(
        invalid(&text),
        InvalidLibraryBuildSpec::DuplicateAlbumRule {
            album_handle: "aenima".into()
        }
    );
}

#[test]
fn album_rule_rejects_undeclared_album_handle() {
    let album_rule =
        "codec = \"opus\"\nbitrate = 128\n[[album_rules]]\nalbums = [\"missing\"]\nbitrate = 96\n";
    assert_eq!(
        invalid(album_rule),
        InvalidLibraryBuildSpec::UnknownAlbum {
            album_handle: "missing".into()
        }
    );
}

#[test]
fn album_rule_can_reference_filesystem_only_album() {
    let text = album_config("fs_only", "directory = \"Undertow\"\n")
        + "[[album_rules]]\nalbums = [\"fs_only\"]\nbitrate = 96\n";

    let config = valid(&text);
    assert_eq!(
        config.albums()["fs_only"],
        Album {
            name: None,
            artist: None,
            directories: Some(vec![PathBuf::from("Undertow")]),
        }
    );
    assert_eq!(
        config.album_rules(),
        &[AlbumRule {
            album_handles: vec!["fs_only".into()],
            bitrate: 96,
        }]
    );
}
