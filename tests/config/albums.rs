/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! The `[albums.<handle>]` tables: optional metadata, filesystem selectors,
//! and their per-album validation.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use scarab::{Album, InvalidLibraryBuildSpec, LibraryBuildSpec, LibraryBuildSpecError, parse};

use super::{album_config, invalid, rejects_toml, valid};

fn album_directories(config: &LibraryBuildSpec, handle: &str) -> Vec<PathBuf> {
    config.albums()[handle]
        .directories
        .clone()
        .expect("album should have a filesystem scope")
}

#[test]
fn album_declaration_order_is_preserved() {
    // Declared out of alphabetical order: z before a
    let text =
        "codec = \"opus\"\nbitrate = 128\n[albums.z]\nname = \"Z\"\n[albums.a]\nname = \"A\"\n";
    let config = valid(text);

    let handles: Vec<&str> = config.albums().keys().map(String::as_str).collect();
    assert_eq!(handles, ["z", "a"]);
}

#[test]
fn invalid_albums_are_reported_in_declaration_order() {
    // Both albums are independently malformed.
    // z is declared first but sorts last,
    // so declaration order must select its error.
    let text = "codec = \"opus\"\nbitrate = 128\n[albums.z]\n[albums.a]\ndirectory = \"\"\n";
    assert_eq!(
        invalid(text),
        InvalidLibraryBuildSpec::MissingAlbumSelector {
            album_handle: "z".into()
        }
    );
}

#[test]
fn later_album_additions_retain_first_introduction_position() {
    // Dotted declarations keep z first and `a` second even though z is
    // given a name after `a` is introduced.
    let text = "codec = \"opus\"\nbitrate = 128\n\
                albums.z.directory = \"z\"\n\
                albums.a.directory = \"a\"\n\
                albums.z.name = \"Z\"\n";
    let config = valid(text);

    let handles: Vec<&str> = config.albums().keys().map(String::as_str).collect();
    assert_eq!(handles, ["z", "a"]);
    assert_eq!(config.albums()["z"].name.as_deref(), Some("Z"));
}

#[test]
fn album_rule_references_do_not_establish_album_order() {
    // The rule references `a` before its declaration, album iteration still
    // follows the declaration order of z then a.
    let text = "codec = \"opus\"\nbitrate = 128\n\
                [[album_rules]]\nalbums = [\"a\"]\nbitrate = 96\n\
                [albums.z]\nname = \"Z\"\n\
                [albums.a]\nname = \"A\"\n";
    let config = valid(text);

    let handles: Vec<&str> = config.albums().keys().map(String::as_str).collect();
    assert_eq!(handles, ["z", "a"]);
}

#[test]
fn parses_optional_metadata_selectors() {
    let name_only = valid(&album_config("name_only", "name = \"Ænima\"\n"));
    assert_eq!(
        name_only.albums()["name_only"],
        Album {
            name: Some("Ænima".into()),
            artist: None,
            directories: None,
        }
    );

    let artist_only = valid(&album_config("artist_only", "artist = \"Tool\"\n"));
    assert_eq!(
        artist_only.albums()["artist_only"],
        Album {
            name: None,
            artist: Some("Tool".into()),
            directories: None,
        }
    );

    let empty_name = valid(&album_config("empty_name", "name = \"\"\n"));
    assert_eq!(
        empty_name.albums()["empty_name"],
        Album {
            name: Some(String::new()),
            artist: None,
            directories: None,
        }
    );

    let empty_artist = valid(&album_config("empty_artist", "artist = \"\"\n"));
    assert_eq!(
        empty_artist.albums()["empty_artist"],
        Album {
            name: None,
            artist: Some(String::new()),
            directories: None,
        }
    );
}

#[test]
fn album_metadata_values_are_preserved_exactly() {
    let whitespace = valid(&album_config("blank", "name = \"   \"\n"));
    assert_eq!(whitespace.albums()["blank"].name.as_deref(), Some("   "));

    let with_artist = valid(&album_config("blank", "name = \"\"\nartist = \"  \"\n"));
    assert_eq!(
        with_artist.albums()["blank"],
        Album {
            name: Some(String::new()),
            artist: Some("  ".into()),
            directories: None,
        }
    );

    let with_directory = valid(&album_config(
        "blank",
        "name = \"\"\ndirectory = \"Undertow\"\n",
    ));
    assert_eq!(
        with_directory.albums()["blank"],
        Album {
            name: Some(String::new()),
            artist: None,
            directories: Some(vec![PathBuf::from("Undertow")]),
        }
    );
}

#[test]
fn parses_filesystem_only_albums() {
    let singular = valid(&album_config("fs_only", "directory = \"Undertow\"\n"));
    assert_eq!(
        singular.albums()["fs_only"],
        Album {
            name: None,
            artist: None,
            directories: Some(vec![PathBuf::from("Undertow")]),
        }
    );

    let plural = valid(&album_config(
        "fs_only",
        "directories = [\"Disc 1\", \"Disc 2\"]\n",
    ));
    assert_eq!(
        plural.albums()["fs_only"],
        Album {
            name: None,
            artist: None,
            directories: Some(vec![PathBuf::from("Disc 1"), PathBuf::from("Disc 2")]),
        }
    );
}

#[test]
fn parses_metadata_with_filesystem_selectors() {
    let config = valid(&album_config(
        "mixed",
        "name = \"Lateralus\"\nartist = \"Tool\"\ndirectories = [\"Disc 1\", \"Disc 2\"]\n",
    ));
    assert_eq!(
        config.albums()["mixed"],
        Album {
            name: Some("Lateralus".into()),
            artist: Some("Tool".into()),
            directories: Some(vec![PathBuf::from("Disc 1"), PathBuf::from("Disc 2")]),
        }
    );
}

#[test]
fn singular_and_one_element_plural_forms_are_equivalent() {
    let singular = valid(&album_config("undertow", "directory = \"Undertow\"\n"));
    let plural = valid(&album_config("undertow", "directories = [\"Undertow\"]\n"));

    assert_eq!(singular, plural);
    assert_eq!(
        singular.albums()["undertow"],
        Album {
            name: None,
            artist: None,
            directories: Some(vec![PathBuf::from("Undertow")]),
        }
    );
}

#[test]
fn accepts_structurally_varied_album_paths() {
    let explicit_root = valid(&album_config("root", "directory = \".\"\n"));
    assert_eq!(
        album_directories(&explicit_root, "root"),
        vec![PathBuf::from(".")]
    );

    let parent_relative = valid(&album_config("up", "directory = \"../elsewhere\"\n"));
    assert_eq!(
        album_directories(&parent_relative, "up"),
        vec![PathBuf::from("../elsewhere")]
    );

    let inner_relative = valid(&album_config(
        "inner",
        "directories = [\"a/../a\", \"b/../c\"]\n",
    ));
    assert_eq!(
        album_directories(&inner_relative, "inner"),
        vec![PathBuf::from("a/../a"), PathBuf::from("b/../c")]
    );

    #[cfg(target_os = "windows")]
    let absolute = "C:\\Music\\Lateralus";
    #[cfg(not(target_os = "windows"))]
    let absolute = "/music/lateralus";

    // Lexical check only, no filesystem access
    assert!(Path::new(absolute).is_absolute());

    // Encoded as a TOML literal string so a configured backslash survives
    // document encoding unchanged
    let host_absolute = valid(&album_config(
        "absolute",
        &format!("directory = '{absolute}'\n"),
    ));
    let directories = album_directories(&host_absolute, "absolute");
    assert_eq!(directories.len(), 1);
    assert_eq!(directories[0].as_os_str(), OsStr::new(absolute));

    let whitespace = valid(&album_config("spaced", "directory = \"   \"\n"));
    assert_eq!(
        album_directories(&whitespace, "spaced"),
        vec![PathBuf::from("   ")]
    );
}

#[test]
fn preserves_album_filesystem_paths_verbatim() {
    // The last entry is a TOML literal string holding a backslash, kept as
    // an opaque configured string with no absoluteness claim
    let text = album_config(
        "paths",
        "directories = [\"b\", \"a\", \"b\", \"a/../a\", \"\\u00C6nima\", 'a\\b']\n",
    );
    let directories = album_directories(&valid(&text), "paths");

    let expected = ["b", "a", "b", "a/../a", "Ænima", "a\\b"];
    assert_eq!(directories.len(), expected.len());
    for (path, expected) in directories.iter().zip(expected) {
        assert_eq!(path.as_os_str(), OsStr::new(expected));
    }
}

#[test]
fn album_without_any_selector_is_invalid() {
    let text = album_config("bare", "");
    assert_eq!(
        invalid(&text),
        InvalidLibraryBuildSpec::MissingAlbumSelector {
            album_handle: "bare".into()
        }
    );
}

#[test]
fn album_rejects_both_filesystem_forms() {
    let both = album_config("both", "directory = \"A\"\ndirectories = [\"B\"]\n");
    assert_eq!(
        invalid(&both),
        InvalidLibraryBuildSpec::ConflictingAlbumPaths {
            album_handle: "both".into()
        }
    );

    // Combined structural failures are rejected without freezing which
    // error is reported first
    let both_with_empty = album_config("both", "directory = \"\"\ndirectories = []\n");
    assert!(matches!(
        parse(&both_with_empty),
        Err(LibraryBuildSpecError::Invalid(_))
    ));
}

#[test]
fn album_requires_nonempty_directories_list() {
    let bare_list = album_config("empty_list", "directories = []\n");
    assert_eq!(
        invalid(&bare_list),
        InvalidLibraryBuildSpec::EmptyAlbumDirectories {
            album_handle: "empty_list".into()
        }
    );

    let list_with_metadata = album_config("empty_list", "name = \"\"\ndirectories = []\n");
    assert_eq!(
        invalid(&list_with_metadata),
        InvalidLibraryBuildSpec::EmptyAlbumDirectories {
            album_handle: "empty_list".into()
        }
    );
}

#[test]
fn album_requires_nonempty_directory_path() {
    let bare_path = album_config("empty_path", "directory = \"\"\n");
    assert_eq!(
        invalid(&bare_path),
        InvalidLibraryBuildSpec::EmptyAlbumDirectory {
            album_handle: "empty_path".into()
        }
    );

    let path_with_metadata = album_config("empty_path", "artist = \"\"\ndirectory = \"\"\n");
    assert_eq!(
        invalid(&path_with_metadata),
        InvalidLibraryBuildSpec::EmptyAlbumDirectory {
            album_handle: "empty_path".into()
        }
    );
}

#[test]
fn album_rejects_empty_directories_entry() {
    let first = album_config("entries", "directories = [\"\", \"ok\"]\n");
    assert_eq!(
        invalid(&first),
        InvalidLibraryBuildSpec::EmptyAlbumDirectoriesEntry {
            album_handle: "entries".into(),
            index: 0,
        }
    );

    let later = album_config("entries", "directories = [\"ok\", \"\", \"ok\"]\n");
    assert_eq!(
        invalid(&later),
        InvalidLibraryBuildSpec::EmptyAlbumDirectoriesEntry {
            album_handle: "entries".into(),
            index: 1,
        }
    );
}

#[test]
fn rejects_incorrect_album_field_types() {
    rejects_toml(&album_config("typed", "name = true\n"));
    rejects_toml(&album_config("typed", "artist = 2001\n"));
    rejects_toml(&album_config("typed", "directory = 5\n"));
    rejects_toml(&album_config("typed", "directories = \"Disc 1\"\n"));
    rejects_toml(&album_config("typed", "directories = [\"a\", 2]\n"));
    rejects_toml(&album_config("typed", "directories = [true]\n"));
}
