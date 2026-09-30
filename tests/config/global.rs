/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Whole-document configuration: top-level keys, size modes, and the
//! accepted prototype example.

use scarab::{
    Codec, EncodingProfile, Files, InvalidLibraryBuildSpec, SizeMode, TrackAction, TrackTarget,
};

use super::{invalid, prefixed, rejects_toml, valid};

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

    assert_eq!(config.codec(), Codec::Opus);
    assert_eq!(config.encoding_profile(), EncodingProfile::Music);
    assert_eq!(config.size_mode(), &SizeMode::TargetSize("20 GiB".into()));
    assert_eq!(config.files().album_art, Some(true));
    assert_eq!(config.files().include, Some(vec!["lrc".to_string()]));
    assert_eq!(config.files().exclude, None);

    let lateralus = config
        .album("lateralus")
        .expect("lateralus must be declared");
    assert_eq!(lateralus.name.as_deref(), Some("Lateralus"));
    assert_eq!(lateralus.artist.as_deref(), Some("Tool"));
    assert_eq!(lateralus.directories, None);

    let ten_thousand_days = config
        .album("ten_thousand_days")
        .expect("ten_thousand_days must be declared");
    assert_eq!(ten_thousand_days.name.as_deref(), Some("10,000 Days"));
    assert_eq!(ten_thousand_days.artist.as_deref(), Some("Tool"));
    assert_eq!(ten_thousand_days.directories, None);

    let album_rules = config.album_rules();
    assert_eq!(album_rules.len(), 1);
    assert_eq!(
        album_rules[0].album_handles,
        ["lateralus", "ten_thousand_days"]
    );
    assert_eq!(album_rules[0].bitrate, 160);

    let groups = config.track_rules();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].album_handle, "lateralus");

    let rules = &groups[0].rules;
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[0].target, TrackTarget::Track("Faaip de Oiad".into()));
    assert_eq!(rules[0].action, TrackAction::Exclude);
    assert_eq!(
        rules[1].target,
        TrackTarget::Tracks(vec!["Parabol".into(), "Parabola".into()])
    );
    assert_eq!(rules[1].action, TrackAction::Bitrate(192));
}

#[test]
fn minimal_config_defaults_to_music_profile() {
    let config = valid(&prefixed(""));

    assert_eq!(config.codec(), Codec::Opus);
    assert_eq!(config.encoding_profile(), EncodingProfile::Music);
    assert_eq!(config.size_mode(), &SizeMode::Bitrate(128));
    assert_eq!(config.files(), &Files::default());

    let album = config.album("aenima").expect("aenima must be declared");
    assert_eq!(album.name.as_deref(), Some("Ænima"));
    assert_eq!(album.artist.as_deref(), Some("Tool"));
    assert_eq!(album.directories, None);

    assert!(config.album_rules().is_empty());
    assert!(config.track_rules().is_empty());
}

#[test]
fn encoding_profile_none_is_valid() {
    let text = "codec = \"opus\"\nbitrate = 128\nencoding_profile = \"none\"\n";
    assert_eq!(valid(text).encoding_profile(), EncodingProfile::None);
}

#[test]
fn target_size_is_preserved_verbatim() {
    let text = "codec = \"opus\"\ntarget_size = \"1.5 GiB\"\n";
    assert_eq!(
        valid(text).size_mode(),
        &SizeMode::TargetSize("1.5 GiB".into())
    );
}

#[test]
fn rejects_missing_or_unsupported_values() {
    rejects_toml("codec = \"flac\"\nbitrate = 128\n");
    rejects_toml("bitrate = 128\n");
    rejects_toml("codec = \"opus\"\nbitrate = 128\nencoding_profile = \"podcast\"\n");
}

#[test]
fn rejects_unknown_keys_at_every_boundary() {
    // top-level config
    rejects_toml("codec = \"opus\"\nbitrate = 128\nbogus = true\n");
    // [files]
    rejects_toml("codec = \"opus\"\nbitrate = 128\n[files]\nbogus = true\n");
    // [albums.<handle>]
    rejects_toml(
        "codec = \"opus\"\nbitrate = 128\n[albums.aenima]\nname = \"Ænima\"\nartist = \"Tool\"\nyear = 1996\n",
    );
    // [[album_rules]]
    rejects_toml(&prefixed(
        "[[album_rules]]\nalbums = [\"aenima\"]\nbitrate = 96\nbogus = true\n",
    ));
    // [[track_rules]]
    rejects_toml(&prefixed(
        "[[track_rules]]\nalbum = \"aenima\"\nrules = [{ track = \"Stinkfist\", bitrate = 64 }]\nbogus = true\n",
    ));
    // nested track-rule entry
    rejects_toml(&prefixed(
        "[[track_rules]]\nalbum = \"aenima\"\nrules = [{ track = \"Stinkfist\", bitrate = 64, bogus = true }]\n",
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
