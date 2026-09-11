/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Deterministic parse and validation rules for Scarab's TOML configuration.

use scarab::{
    Album, AlbumRule, Codec, EncodingProfile, Files, InvalidLibraryBuildSpec, LibraryBuildSpec,
    LibraryBuildSpecError, SizeMode, TrackAction, TrackRule, TrackRuleGroup, TrackTarget, parse,
};

/// Valid top-level keys followed by one declared album. Top-level keys must
/// precede tables in TOML, so tests append only tables to this prefix.
const PREFIX: &str = "codec = \"opus\"\nbitrate = 128\n[albums.debut]\nname = \"Debut\"\nartist = \"Example Artist\"\n";

fn prefixed(body: &str) -> String {
    format!("{PREFIX}{body}")
}

fn valid(text: &str) -> LibraryBuildSpec {
    match parse(text) {
        Ok(config) => config,
        Err(error) => panic!("expected valid configuration, got: {error}"),
    }
}

fn invalid(text: &str) -> InvalidLibraryBuildSpec {
    match parse(text) {
        Err(LibraryBuildSpecError::Invalid(error)) => error,
        Err(LibraryBuildSpecError::Toml(error)) => {
            panic!("expected validation error, got TOML error: {error}")
        }
        Ok(config) => panic!("expected validation error, got: {config:?}"),
    }
}

fn rejects_toml(text: &str) {
    match parse(text) {
        Err(LibraryBuildSpecError::Toml(_)) => {}
        Err(other) => panic!("expected TOML error, got: {other}"),
        Ok(config) => panic!("expected TOML error, got: {config:?}"),
    }
}

fn track_rule(entry: &str) -> String {
    prefixed(&format!(
        "[[track_rules]]\nalbum = \"debut\"\nrules = [{{ {entry} }}]\n"
    ))
}

fn track_rule_error(entry: &str) -> InvalidLibraryBuildSpec {
    invalid(&track_rule(entry))
}

/// Accepted prototype example: target-size mode and a multi-album rule.
const ACCEPTED_EXAMPLE: &str = r#"codec = "opus"
encoding_profile = "music"
target_size = "20 GiB"

[files]
album_art = true
include = ["lrc"]

[albums.lateralus]
name = "Lateralus"
artist = "Tool"

[albums.ten_thousand_days]
name = "10,000 Days"
artist = "Tool"

[[album_rules]]
albums = ["lateralus", "ten_thousand_days"]
bitrate = 160

[[track_rules]]
album = "lateralus"
rules = [
    { track = "Faaip de Oiad", exclude = true },
    { tracks = ["Parabol", "Parabola"], bitrate = 192 },
]
"#;

#[test]
fn parses_accepted_toml_example() {
    let config = valid(ACCEPTED_EXAMPLE);

    assert_eq!(config.codec, Codec::Opus);
    assert_eq!(config.encoding_profile, EncodingProfile::Music);
    assert_eq!(config.size_mode, SizeMode::TargetSize("20 GiB".into()));
    assert_eq!(config.files.album_art, Some(true));
    assert_eq!(config.files.include, Some(vec!["lrc".to_string()]));
    assert_eq!(config.files.exclude, None);
    assert_eq!(
        config.albums["lateralus"],
        Album {
            name: "Lateralus".into(),
            artist: "Tool".into(),
        }
    );
    assert_eq!(
        config.albums["ten_thousand_days"],
        Album {
            name: "10,000 Days".into(),
            artist: "Tool".into(),
        }
    );
    assert_eq!(
        config.album_rules,
        vec![AlbumRule {
            albums: vec!["lateralus".into(), "ten_thousand_days".into()],
            bitrate: 160,
        }]
    );
    assert_eq!(
        config.track_rules,
        vec![TrackRuleGroup {
            album: "lateralus".into(),
            rules: vec![
                TrackRule {
                    target: TrackTarget::Track("Faaip de Oiad".into()),
                    action: TrackAction::Exclude,
                },
                TrackRule {
                    target: TrackTarget::Tracks(vec!["Parabol".into(), "Parabola".into()]),
                    action: TrackAction::Bitrate(192),
                },
            ],
        }]
    );
}

#[test]
fn minimal_config_defaults_to_music_profile() {
    let config = valid(&prefixed(""));

    assert_eq!(config.codec, Codec::Opus);
    assert_eq!(config.encoding_profile, EncodingProfile::Music);
    assert_eq!(config.size_mode, SizeMode::Bitrate(128));
    assert_eq!(config.files, Files::default());
    assert_eq!(
        config.albums["debut"],
        Album {
            name: "Debut".into(),
            artist: "Example Artist".into(),
        }
    );
    assert!(config.album_rules.is_empty());
    assert!(config.track_rules.is_empty());
}

#[test]
fn encoding_profile_none_is_valid() {
    let text = "codec = \"opus\"\nbitrate = 128\nencoding_profile = \"none\"\n";
    assert_eq!(valid(text).encoding_profile, EncodingProfile::None);
}

#[test]
fn target_size_is_preserved_verbatim() {
    let text = "codec = \"opus\"\ntarget_size = \"1.5 GiB\"\n";
    assert_eq!(
        valid(text).size_mode,
        SizeMode::TargetSize("1.5 GiB".into())
    );
}

#[test]
fn rejects_missing_or_unsupported_values() {
    rejects_toml("codec = \"flac\"\nbitrate = 128\n");
    rejects_toml("bitrate = 128\n");
    rejects_toml("codec = \"opus\"\nbitrate = 128\nencoding_profile = \"podcast\"\n");
    rejects_toml("codec = \"opus\"\nbitrate = 128\n[albums.debut]\nname = \"Debut\"\n");
}

#[test]
fn rejects_unknown_keys_at_every_boundary() {
    // top-level config
    rejects_toml("codec = \"opus\"\nbitrate = 128\nbogus = true\n");
    // [files]
    rejects_toml("codec = \"opus\"\nbitrate = 128\n[files]\nbogus = true\n");
    // [albums.<handle>]
    rejects_toml(
        "codec = \"opus\"\nbitrate = 128\n[albums.debut]\nname = \"Debut\"\nartist = \"Tool\"\nyear = 2001\n",
    );
    // [[album_rules]]
    rejects_toml(&prefixed(
        "[[album_rules]]\nalbums = [\"debut\"]\nbitrate = 96\nbogus = true\n",
    ));
    // [[track_rules]]
    rejects_toml(&prefixed(
        "[[track_rules]]\nalbum = \"debut\"\nrules = [{ track = \"One\", bitrate = 64 }]\nbogus = true\n",
    ));
    // nested track-rule entry
    rejects_toml(&prefixed(
        "[[track_rules]]\nalbum = \"debut\"\nrules = [{ track = \"One\", bitrate = 64, bogus = true }]\n",
    ));
}

#[test]
fn requires_exactly_one_global_size_mode() {
    assert_eq!(
        invalid("codec = \"opus\"\n"),
        InvalidLibraryBuildSpec::MissingSizeMode
    );
    assert_eq!(
        invalid("codec = \"opus\"\nbitrate = 128\ntarget_size = \"1G\"\n"),
        InvalidLibraryBuildSpec::ConflictingSizeMode
    );
}

#[test]
fn parses_files_selection() {
    let included = "codec = \"opus\"\nbitrate = 128\n[files]\nalbum_art = true\ninclude = [\"jpg\", \"png\"]\n";
    let files = valid(included).files;

    assert_eq!(files.album_art, Some(true));
    assert_eq!(
        files.include,
        Some(vec!["jpg".to_string(), "png".to_string()])
    );
    assert_eq!(files.exclude, None);

    let excluded = "codec = \"opus\"\nbitrate = 128\n[files]\nexclude = [\"cue\"]\n";
    let files = valid(excluded).files;

    assert_eq!(files.album_art, None);
    assert_eq!(files.include, None);
    assert_eq!(files.exclude, Some(vec!["cue".to_string()]));
}

#[test]
fn parses_files_include_and_exclude_together() {
    let text = "codec = \"opus\"\nbitrate = 128\n[files]\ninclude = [\"jpg\", \"png\"]\nexclude = [\"cue\"]\n";
    let files = valid(text).files;

    assert_eq!(
        files.include,
        Some(vec!["jpg".to_string(), "png".to_string()])
    );
    assert_eq!(files.exclude, Some(vec!["cue".to_string()]));
}

#[test]
fn rejects_undeclared_album_handles() {
    let album_rule =
        "codec = \"opus\"\nbitrate = 128\n[[album_rules]]\nalbums = [\"missing\"]\nbitrate = 96\n";
    assert_eq!(
        invalid(album_rule),
        InvalidLibraryBuildSpec::UnknownAlbum {
            handle: "missing".into()
        }
    );

    let track_rule = "codec = \"opus\"\nbitrate = 128\n[[track_rules]]\nalbum = \"missing\"\nrules = [{ track = \"One\", exclude = true }]\n";
    assert_eq!(
        invalid(track_rule),
        InvalidLibraryBuildSpec::UnknownAlbum {
            handle: "missing".into()
        }
    );
}

#[test]
fn parses_album_rule_with_one_album() {
    let config = valid(&prefixed(
        "[[album_rules]]\nalbums = [\"debut\"]\nbitrate = 96\n",
    ));
    assert_eq!(
        config.album_rules,
        vec![AlbumRule {
            albums: vec!["debut".into()],
            bitrate: 96,
        }]
    );
}

#[test]
fn album_rule_applies_to_every_listed_album() {
    let text = "codec = \"opus\"\nbitrate = 128\n[albums.one]\nname = \"One\"\nartist = \"A\"\n[albums.two]\nname = \"Two\"\nartist = \"B\"\n[[album_rules]]\nalbums = [\"one\", \"two\"]\nbitrate = 96\n";
    assert_eq!(
        valid(text).album_rules,
        vec![AlbumRule {
            albums: vec!["one".into(), "two".into()],
            bitrate: 96,
        }]
    );
}

#[test]
fn album_rule_rejects_singular_album_alias() {
    rejects_toml(&prefixed(
        "[[album_rules]]\nalbum = \"debut\"\nbitrate = 96\n",
    ));
    rejects_toml(&prefixed(
        "[[album_rules]]\nalbums = [\"debut\"]\nalbum = \"debut\"\nbitrate = 96\n",
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
        "[[album_rules]]\nalbums = [\"debut\"]\nbitrate = 96\n[[album_rules]]\nalbums = [\"debut\"]\nbitrate = 64\n",
    );
    let second_then_first = prefixed(
        "[[album_rules]]\nalbums = [\"debut\"]\nbitrate = 64\n[[album_rules]]\nalbums = [\"debut\"]\nbitrate = 96\n",
    );

    for text in [&first_then_second, &second_then_first] {
        assert_eq!(
            invalid(text),
            InvalidLibraryBuildSpec::DuplicateAlbumRule {
                handle: "debut".into()
            }
        );
    }

    let partial_overlap = "codec = \"opus\"\nbitrate = 128\n[albums.one]\nname = \"One\"\nartist = \"A\"\n[albums.two]\nname = \"Two\"\nartist = \"B\"\n[[album_rules]]\nalbums = [\"one\", \"two\"]\nbitrate = 96\n[[album_rules]]\nalbums = [\"two\"]\nbitrate = 64\n";
    assert_eq!(
        invalid(partial_overlap),
        InvalidLibraryBuildSpec::DuplicateAlbumRule {
            handle: "two".into()
        }
    );
}

#[test]
fn album_rule_rejects_duplicate_handles_within_one_rule() {
    let text = prefixed("[[album_rules]]\nalbums = [\"debut\", \"debut\"]\nbitrate = 96\n");
    assert_eq!(
        invalid(&text),
        InvalidLibraryBuildSpec::DuplicateAlbumRule {
            handle: "debut".into()
        }
    );
}

#[test]
fn parses_track_rules() {
    let text = prefixed(
        "[[track_rules]]\nalbum = \"debut\"\nrules = [{ track = \"One\", exclude = true }, { tracks = [\"Two\", \"Three\"], bitrate = 64 }]\n",
    );
    assert_eq!(
        valid(&text).track_rules,
        vec![TrackRuleGroup {
            album: "debut".into(),
            rules: vec![
                TrackRule {
                    target: TrackTarget::Track("One".into()),
                    action: TrackAction::Exclude,
                },
                TrackRule {
                    target: TrackTarget::Tracks(vec!["Two".into(), "Three".into()]),
                    action: TrackAction::Bitrate(64),
                },
            ],
        }]
    );
}

#[test]
fn same_track_name_is_allowed_in_different_albums() {
    let text = "codec = \"opus\"\nbitrate = 128\n[albums.one]\nname = \"One\"\nartist = \"A\"\n[albums.two]\nname = \"Two\"\nartist = \"B\"\n[[track_rules]]\nalbum = \"one\"\nrules = [{ track = \"Song\", bitrate = 64 }]\n[[track_rules]]\nalbum = \"two\"\nrules = [{ track = \"Song\", bitrate = 64 }]\n";
    assert_eq!(valid(text).track_rules.len(), 2);
}

#[test]
fn track_entries_require_exactly_one_target() {
    for entry in [
        "track = \"One\", tracks = [\"Two\"], bitrate = 64",
        "bitrate = 64",
    ] {
        assert_eq!(
            track_rule_error(entry),
            InvalidLibraryBuildSpec::TrackTargetNotExclusive {
                album: "debut".into()
            }
        );
    }
    assert_eq!(
        track_rule_error("tracks = [], bitrate = 64"),
        InvalidLibraryBuildSpec::EmptyTracks {
            album: "debut".into()
        }
    );
}

#[test]
fn track_entries_require_exactly_one_action() {
    for entry in [
        "track = \"One\"",
        "track = \"One\", exclude = true, bitrate = 64",
        "track = \"One\", exclude = false",
        "track = \"One\", exclude = false, bitrate = 64",
    ] {
        assert_eq!(
            track_rule_error(entry),
            InvalidLibraryBuildSpec::TrackActionNotExclusive {
                album: "debut".into()
            }
        );
    }
}

#[test]
fn track_rule_group_requires_at_least_one_rule() {
    let omitted = prefixed("[[track_rules]]\nalbum = \"debut\"\n");
    let empty = prefixed("[[track_rules]]\nalbum = \"debut\"\nrules = []\n");

    for text in [&omitted, &empty] {
        assert_eq!(
            invalid(text),
            InvalidLibraryBuildSpec::EmptyTrackRules {
                album: "debut".into()
            }
        );
    }
}

#[test]
fn track_cannot_receive_two_rules() {
    let within_group = "[[track_rules]]\nalbum = \"debut\"\nrules = [{ track = \"Same\", bitrate = 64 }, { track = \"Same\", exclude = true }]\n";
    let within_group_reversed = "[[track_rules]]\nalbum = \"debut\"\nrules = [{ track = \"Same\", exclude = true }, { track = \"Same\", bitrate = 64 }]\n";
    let across_groups = "[[track_rules]]\nalbum = \"debut\"\nrules = [{ track = \"Same\", bitrate = 64 }]\n[[track_rules]]\nalbum = \"debut\"\nrules = [{ track = \"Same\", exclude = true }]\n";
    let through_tracks_list = "[[track_rules]]\nalbum = \"debut\"\nrules = [{ tracks = [\"Same\", \"Two\"], bitrate = 64 }, { track = \"Same\", exclude = true }]\n";
    let within_tracks_list = "[[track_rules]]\nalbum = \"debut\"\nrules = [{ tracks = [\"Same\", \"Same\"], bitrate = 64 }]\n";

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
                album: "debut".into(),
                track: "Same".into()
            }
        );
    }
}

#[test]
fn inline_and_nested_track_rules_deserialize_identically() {
    let inline = prefixed(
        "[[track_rules]]\nalbum = \"debut\"\nrules = [{ track = \"One\", exclude = true }, { tracks = [\"Two\", \"Three\"], bitrate = 64 }]\n",
    );
    let nested = prefixed(
        "[[track_rules]]\nalbum = \"debut\"\n[[track_rules.rules]]\ntrack = \"One\"\nexclude = true\n[[track_rules.rules]]\ntracks = [\"Two\", \"Three\"]\nbitrate = 64\n",
    );

    assert_eq!(valid(&inline), valid(&nested));
}
