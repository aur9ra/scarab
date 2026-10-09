/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! The `[collections.<handle>]` tables: metadata predicates, directory
//! selectors, and declaration validation.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use scarab::{
    CollectionDeclaration, InvalidLibraryBuildSpec, LibraryBuildSpec, LibraryBuildSpecError, parse,
};

use super::{collection_config, invalid, rejects_toml, valid};

fn collection_directories(config: &LibraryBuildSpec, handle: &str) -> Vec<PathBuf> {
    config
        .collection_declaration(handle)
        .expect("collection should be declared")
        .directories
        .clone()
        .expect("collection should have directory selectors")
}

fn declaration(handle: &str, body: &str) -> CollectionDeclaration {
    valid(&collection_config(handle, body))
        .collection_declaration(handle)
        .expect("collection must be declared")
        .clone()
}

#[test]
fn collection_declarations_yield_ordered_handle_declaration_pairs() {
    // Different declaration fields catch mismatched pairs.
    let text = "codec = \"opus\"\nbitrate = 128\n\
                [collections.z]\nalbum_name = \"Z\"\ndirectory = \"z\"\n\
                [collections.a]\nalbum_artist = \"A\"\n\
                [collections.m]\ndirectories = [\"m1\", \"m2\"]\n";
    let config = valid(text);

    let declared: Vec<(&str, &CollectionDeclaration)> = config.collection_declarations().collect();
    assert_eq!(declared.len(), 3);

    let (handle, declaration) = declared[0];
    assert_eq!(handle, "z");
    assert_eq!(declaration.album_names, Some(vec!["Z".to_owned()]));
    assert_eq!(declaration.album_artists, None);
    assert_eq!(declaration.track_artists, None);
    assert_eq!(declaration.directories, Some(vec![PathBuf::from("z")]));

    let (handle, declaration) = declared[1];
    assert_eq!(handle, "a");
    assert_eq!(declaration.album_names, None);
    assert_eq!(declaration.album_artists, Some(vec!["A".to_owned()]));
    assert_eq!(declaration.track_artists, None);
    assert_eq!(declaration.directories, None);

    let (handle, declaration) = declared[2];
    assert_eq!(handle, "m");
    assert_eq!(declaration.album_names, None);
    assert_eq!(declaration.album_artists, None);
    assert_eq!(declaration.track_artists, None);
    assert_eq!(
        declaration.directories,
        Some(vec![PathBuf::from("m1"), PathBuf::from("m2")])
    );
}

#[test]
fn invalid_declarations_are_reported_in_declaration_order() {
    // Both declarations are independently malformed.
    // z is declared first but sorts last,
    // so declaration order must select its error.
    let text =
        "codec = \"opus\"\nbitrate = 128\n[collections.z]\n[collections.a]\ndirectory = \"\"\n";
    assert_eq!(
        invalid(text),
        InvalidLibraryBuildSpec::MissingCollectionSelector {
            collection_handle: "z".into()
        }
    );
}

#[test]
fn adding_declaration_fields_preserves_handle_order() {
    // Adding z's selector after a is declared must not change declaration order.
    let text = "codec = \"opus\"\nbitrate = 128\n\
                collections.z.directory = \"z\"\n\
                collections.a.directory = \"a\"\n\
                collections.z.album_name = \"Z\"\n";
    let config = valid(text);

    let handles: Vec<&str> = config
        .collection_declarations()
        .map(|(handle, _)| handle)
        .collect();
    assert_eq!(handles, ["z", "a"]);
    assert_eq!(
        config
            .collection_declaration("z")
            .expect("z must be declared")
            .album_names,
        Some(vec!["Z".to_owned()])
    );
}

#[test]
fn collection_rule_references_do_not_establish_declaration_order() {
    // The rule references `a` before its declaration, iteration still
    // follows the declaration order of z then a.
    let text = "codec = \"opus\"\nbitrate = 128\n\
                [[collection_rules]]\ncollections = [\"a\"]\nbitrate = 96\n\
                [collections.z]\nalbum_name = \"Z\"\n\
                [collections.a]\nalbum_name = \"A\"\n";
    let config = valid(text);

    let handles: Vec<&str> = config
        .collection_declarations()
        .map(|(handle, _)| handle)
        .collect();
    assert_eq!(handles, ["z", "a"]);
}

#[test]
fn collection_declaration_lookup_is_exact() {
    let config = valid(&collection_config(
        "lateralus",
        "album_name = \"Lateralus\"\n",
    ));

    assert_eq!(
        config
            .collection_declaration("lateralus")
            .expect("lateralus must be declared")
            .album_names,
        Some(vec!["Lateralus".to_owned()])
    );

    // Missing, differently cased, and padded handles do not match.
    assert!(config.collection_declaration("missing").is_none());
    assert!(config.collection_declaration("Lateralus").is_none());
    assert!(config.collection_declaration(" lateralus").is_none());
    assert!(config.collection_declaration("lateralus ").is_none());
}

#[test]
fn metadata_families_accept_singular_and_plural_forms() {
    let cases = [
        ("album_name", "album_names", "album_names"),
        ("album_artist", "album_artists", "album_artists"),
        ("track_artist", "track_artists", "track_artists"),
    ];
    for (singular, plural, family) in cases {
        let singular_declaration = declaration("single", &format!("{singular} = \"Value\"\n"));
        let plural_declaration = declaration("single", &format!("{plural} = [\"Value\"]\n"));
        for candidate in [&singular_declaration, &plural_declaration] {
            assert_eq!(candidate.directories, None, "family {family}");
            match family {
                "album_names" => {
                    assert_eq!(
                        candidate.album_names,
                        Some(vec!["Value".to_owned()]),
                        "family {family}"
                    );
                    assert_eq!(candidate.album_artists, None, "family {family}");
                    assert_eq!(candidate.track_artists, None, "family {family}");
                }
                "album_artists" => {
                    assert_eq!(candidate.album_names, None, "family {family}");
                    assert_eq!(
                        candidate.album_artists,
                        Some(vec!["Value".to_owned()]),
                        "family {family}"
                    );
                    assert_eq!(candidate.track_artists, None, "family {family}");
                }
                "track_artists" => {
                    assert_eq!(candidate.album_names, None, "family {family}");
                    assert_eq!(candidate.album_artists, None, "family {family}");
                    assert_eq!(
                        candidate.track_artists,
                        Some(vec!["Value".to_owned()]),
                        "family {family}"
                    );
                }
                _ => unreachable!(),
            }
        }

        let plural_multi = declaration("multi", &format!("{plural} = [\"A\", \"B\"]\n"));
        match family {
            "album_names" => assert_eq!(
                plural_multi.album_names,
                Some(vec!["A".to_owned(), "B".to_owned()])
            ),
            "album_artists" => assert_eq!(
                plural_multi.album_artists,
                Some(vec!["A".to_owned(), "B".to_owned()])
            ),
            "track_artists" => assert_eq!(
                plural_multi.track_artists,
                Some(vec!["A".to_owned(), "B".to_owned()])
            ),
            _ => unreachable!(),
        }
    }
}

#[test]
fn singular_and_one_element_plural_metadata_forms_are_canonically_equal() {
    let cases = [
        ("album_name", "album_names"),
        ("album_artist", "album_artists"),
        ("track_artist", "track_artists"),
    ];
    for (singular, plural) in cases {
        let singular_text = collection_config("item", &format!("{singular} = \"Value\"\n"));
        let plural_text = collection_config("item", &format!("{plural} = [\"Value\"]\n"));
        assert_eq!(
            valid(&singular_text),
            valid(&plural_text),
            "family {singular} and {plural} must canonicalize identically"
        );
    }
}

#[test]
fn metadata_family_conflicts_report_family_conflict() {
    let cases = [
        (
            "album_name",
            "album_names",
            InvalidLibraryBuildSpec::ConflictingCollectionAlbumNameForms {
                collection_handle: "both".into(),
            },
        ),
        (
            "album_artist",
            "album_artists",
            InvalidLibraryBuildSpec::ConflictingCollectionAlbumArtistForms {
                collection_handle: "both".into(),
            },
        ),
        (
            "track_artist",
            "track_artists",
            InvalidLibraryBuildSpec::ConflictingCollectionTrackArtistForms {
                collection_handle: "both".into(),
            },
        ),
    ];
    for (singular, plural, expected) in cases {
        // Distinct values conflict.
        let distinct =
            collection_config("both", &format!("{singular} = \"A\"\n{plural} = [\"B\"]\n"));
        assert_eq!(invalid(&distinct), expected);

        // Equivalent values still conflict.
        let equivalent =
            collection_config("both", &format!("{singular} = \"A\"\n{plural} = [\"A\"]\n"));
        assert_eq!(invalid(&equivalent), expected);

        // Singular plus empty plural reports the conflict, not the empty list.
        let with_empty = collection_config("both", &format!("{singular} = \"A\"\n{plural} = []\n"));
        assert_eq!(invalid(&with_empty), expected);
    }
}

#[test]
fn empty_plural_metadata_lists_are_invalid() {
    let cases = [
        (
            "album_names",
            InvalidLibraryBuildSpec::EmptyCollectionAlbumNames {
                collection_handle: "empty".into(),
            },
        ),
        (
            "album_artists",
            InvalidLibraryBuildSpec::EmptyCollectionAlbumArtists {
                collection_handle: "empty".into(),
            },
        ),
        (
            "track_artists",
            InvalidLibraryBuildSpec::EmptyCollectionTrackArtists {
                collection_handle: "empty".into(),
            },
        ),
    ];
    for (plural, expected) in cases {
        let text = collection_config("empty", &format!("{plural} = []\n"));
        assert_eq!(invalid(&text), expected);
    }
}

#[test]
fn empty_and_whitespace_singular_metadata_strings_are_valid() {
    let cases = [
        ("album_name", "album_names"),
        ("album_artist", "album_artists"),
        ("track_artist", "track_artists"),
    ];
    for (singular, family) in cases {
        for value in ["", "   "] {
            let candidate = declaration("item", &format!("{singular} = \"{value}\"\n"));
            let expected = Some(vec![value.to_owned()]);
            match family {
                "album_names" => {
                    assert_eq!(candidate.album_names, expected);
                    assert_eq!(candidate.album_artists, None);
                    assert_eq!(candidate.track_artists, None);
                }
                "album_artists" => {
                    assert_eq!(candidate.album_names, None);
                    assert_eq!(candidate.album_artists, expected);
                    assert_eq!(candidate.track_artists, None);
                }
                "track_artists" => {
                    assert_eq!(candidate.album_names, None);
                    assert_eq!(candidate.album_artists, None);
                    assert_eq!(candidate.track_artists, expected);
                }
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn plural_metadata_values_preserve_order_repetition_and_empty_strings() {
    let cases = ["album_names", "album_artists", "track_artists"];
    for plural in cases {
        let body = format!("{plural} = [\"\", \"B\", \"A\", \"B\", \"   \"]\n");
        let candidate = declaration("item", &body);
        let expected = Some(vec![
            "".to_owned(),
            "B".to_owned(),
            "A".to_owned(),
            "B".to_owned(),
            "   ".to_owned(),
        ]);
        match plural {
            "album_names" => assert_eq!(candidate.album_names, expected),
            "album_artists" => assert_eq!(candidate.album_artists, expected),
            "track_artists" => assert_eq!(candidate.track_artists, expected),
            _ => unreachable!(),
        }
    }
}

#[test]
fn metadata_values_preserve_matching_sensitive_raw_spellings() {
    // NFC normalization or trailing-NUL removal would alter these values.
    let decomposed = "Bjo\\u0308rk";
    let nul_terminated = "Track\\u0000\\u0000";
    let cases = ["album_names", "album_artists", "track_artists"];
    for plural in cases {
        let body = format!("{plural} = [\"{decomposed}\", \"{nul_terminated}\", \"B\"]\n");
        let candidate = declaration("item", &body);
        let expected = Some(vec![
            "Bjo\u{0308}rk".to_owned(),
            "Track\0\0".to_owned(),
            "B".to_owned(),
        ]);
        match plural {
            "album_names" => assert_eq!(candidate.album_names, expected),
            "album_artists" => assert_eq!(candidate.album_artists, expected),
            "track_artists" => assert_eq!(candidate.track_artists, expected),
            _ => unreachable!(),
        }
    }

    for singular in ["album_name", "album_artist", "track_artist"] {
        let candidate = declaration("item", &format!("{singular} = \"{decomposed}\"\n"));
        let expected = Some(vec!["Bjo\u{0308}rk".to_owned()]);
        match singular {
            "album_name" => assert_eq!(candidate.album_names, expected),
            "album_artist" => assert_eq!(candidate.album_artists, expected),
            "track_artist" => assert_eq!(candidate.track_artists, expected),
            _ => unreachable!(),
        }
    }
}

#[test]
fn collection_metadata_values_are_preserved_exactly() {
    let whitespace = valid(&collection_config("blank", "album_name = \"   \"\n"));
    assert_eq!(
        whitespace
            .collection_declaration("blank")
            .expect("blank must be declared")
            .album_names,
        Some(vec!["   ".to_owned()])
    );

    let combined = valid(&collection_config(
        "blank",
        "album_name = \"\"\nalbum_artist = \"  \"\n",
    ));
    let declaration = combined
        .collection_declaration("blank")
        .expect("blank must be declared");
    assert_eq!(declaration.album_names, Some(vec!["".to_owned()]));
    assert_eq!(declaration.album_artists, Some(vec!["  ".to_owned()]));
    assert_eq!(declaration.track_artists, None);
    assert_eq!(declaration.directories, None);

    let with_directory = valid(&collection_config(
        "blank",
        "album_name = \"\"\ndirectory = \"Undertow\"\n",
    ));
    let declaration = with_directory
        .collection_declaration("blank")
        .expect("blank must be declared");
    assert_eq!(declaration.album_names, Some(vec!["".to_owned()]));
    assert_eq!(declaration.album_artists, None);
    assert_eq!(declaration.track_artists, None);
    assert_eq!(
        declaration.directories,
        Some(vec![PathBuf::from("Undertow")])
    );
}

#[test]
fn parses_directory_only_collections() {
    let singular = valid(&collection_config("fs_only", "directory = \"Undertow\"\n"));
    let declaration = singular
        .collection_declaration("fs_only")
        .expect("fs_only must be declared");
    assert_eq!(declaration.album_names, None);
    assert_eq!(declaration.album_artists, None);
    assert_eq!(declaration.track_artists, None);
    assert_eq!(
        declaration.directories,
        Some(vec![PathBuf::from("Undertow")])
    );

    let plural = valid(&collection_config(
        "fs_only",
        "directories = [\"Disc 1\", \"Disc 2\"]\n",
    ));
    let declaration = plural
        .collection_declaration("fs_only")
        .expect("fs_only must be declared");
    assert_eq!(declaration.album_names, None);
    assert_eq!(declaration.album_artists, None);
    assert_eq!(declaration.track_artists, None);
    assert_eq!(
        declaration.directories,
        Some(vec![PathBuf::from("Disc 1"), PathBuf::from("Disc 2")])
    );
}

#[test]
fn parses_metadata_with_directory_selectors() {
    let config = valid(&collection_config(
        "mixed",
        "album_name = \"Lateralus\"\nalbum_artist = \"Tool\"\ntrack_artist = \"Maynard\"\ndirectories = [\"Disc 1\", \"Disc 2\"]\n",
    ));
    let declaration = config
        .collection_declaration("mixed")
        .expect("mixed must be declared");
    assert_eq!(declaration.album_names, Some(vec!["Lateralus".to_owned()]));
    assert_eq!(declaration.album_artists, Some(vec!["Tool".to_owned()]));
    assert_eq!(declaration.track_artists, Some(vec!["Maynard".to_owned()]));
    assert_eq!(
        declaration.directories,
        Some(vec![PathBuf::from("Disc 1"), PathBuf::from("Disc 2")])
    );
}

#[test]
fn metadata_only_declarations_remain_valid() {
    for body in [
        "album_name = \"A\"\n",
        "album_names = [\"A\", \"B\"]\n",
        "album_artist = \"A\"\n",
        "album_artists = [\"A\"]\n",
        "track_artist = \"A\"\n",
        "track_artists = [\"A\", \"A\"]\n",
        "album_name = \"A\"\nalbum_artist = \"B\"\ntrack_artist = \"C\"\n",
    ] {
        valid(&collection_config("meta", body));
    }
}

#[test]
fn singular_and_one_element_plural_forms_are_equivalent() {
    let singular = valid(&collection_config("undertow", "directory = \"Undertow\"\n"));
    let plural = valid(&collection_config(
        "undertow",
        "directories = [\"Undertow\"]\n",
    ));

    assert_eq!(singular, plural);
    let declaration = singular
        .collection_declaration("undertow")
        .expect("undertow must be declared");
    assert_eq!(declaration.album_names, None);
    assert_eq!(declaration.album_artists, None);
    assert_eq!(declaration.track_artists, None);
    assert_eq!(
        declaration.directories,
        Some(vec![PathBuf::from("Undertow")])
    );
}

#[test]
fn removed_metadata_keys_are_unknown_fields() {
    rejects_toml(&collection_config("legacy", "name = \"Ænima\"\n"));
    rejects_toml(&collection_config("legacy", "artist = \"Tool\"\n"));
    rejects_toml(&collection_config(
        "legacy",
        "name = \"Ænima\"\nartist = \"Tool\"\n",
    ));
    // The old `name` and `artist` keys are not aliases. TOML rejects them even
    // alongside a valid key.
    rejects_toml(&collection_config(
        "legacy",
        "album_name = \"Ænima\"\nname = \"Ænima\"\n",
    ));
    rejects_toml(&collection_config(
        "legacy",
        "album_artist = \"Tool\"\nartist = \"Tool\"\n",
    ));
}

#[test]
fn rejects_unknown_and_nested_table_selectors() {
    // Nested tables are not selectors.
    rejects_toml(&collection_config("typed", "metadata = \"A\"\n"));
    rejects_toml(&collection_config("typed", "selectors = [\"A\"]\n"));
    rejects_toml(
        "codec = \"opus\"\nbitrate = 128\n[collections.typed]\nalbum_name = \"A\"\n[collections.typed.metadata]\nalbum_name = \"A\"\n",
    );
    rejects_toml(
        "codec = \"opus\"\nbitrate = 128\n[collections.typed]\nalbum_name = \"A\"\n[collections.typed.selectors]\nalbum_names = [\"A\"]\n",
    );
}

#[test]
fn accepts_structurally_varied_collection_directories() {
    let explicit_root = valid(&collection_config("root", "directory = \".\"\n"));
    assert_eq!(
        collection_directories(&explicit_root, "root"),
        vec![PathBuf::from(".")]
    );

    let parent_relative = valid(&collection_config("up", "directory = \"../elsewhere\"\n"));
    assert_eq!(
        collection_directories(&parent_relative, "up"),
        vec![PathBuf::from("../elsewhere")]
    );

    let inner_relative = valid(&collection_config(
        "inner",
        "directories = [\"a/../a\", \"b/../c\"]\n",
    ));
    assert_eq!(
        collection_directories(&inner_relative, "inner"),
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
    let host_absolute = valid(&collection_config(
        "absolute",
        &format!("directory = '{absolute}'\n"),
    ));
    let directories = collection_directories(&host_absolute, "absolute");
    assert_eq!(directories.len(), 1);
    assert_eq!(directories[0].as_os_str(), OsStr::new(absolute));

    let whitespace = valid(&collection_config("spaced", "directory = \"   \"\n"));
    assert_eq!(
        collection_directories(&whitespace, "spaced"),
        vec![PathBuf::from("   ")]
    );
}

#[test]
fn preserves_collection_directory_paths_verbatim() {
    // The last entry is a TOML literal string holding a backslash, kept as
    // an opaque configured string with no absoluteness claim
    let text = collection_config(
        "paths",
        "directories = [\"b\", \"a\", \"b\", \"a/../a\", \"\\u00C6nima\", 'a\\b']\n",
    );
    let directories = collection_directories(&valid(&text), "paths");

    let expected = ["b", "a", "b", "a/../a", "Ænima", "a\\b"];
    assert_eq!(directories.len(), expected.len());
    for (path, expected) in directories.iter().zip(expected) {
        assert_eq!(path.as_os_str(), OsStr::new(expected));
    }
}

#[test]
fn collection_without_any_selector_is_invalid() {
    let text = collection_config("bare", "");
    assert_eq!(
        invalid(&text),
        InvalidLibraryBuildSpec::MissingCollectionSelector {
            collection_handle: "bare".into()
        }
    );
}

#[test]
fn collection_rejects_both_directory_forms() {
    let both = collection_config("both", "directory = \"A\"\ndirectories = [\"B\"]\n");
    assert_eq!(
        invalid(&both),
        InvalidLibraryBuildSpec::ConflictingCollectionDirectoryForms {
            collection_handle: "both".into()
        }
    );

    // Combined structural failures are rejected without freezing which
    // error is reported first
    let both_with_empty = collection_config("both", "directory = \"\"\ndirectories = []\n");
    assert!(matches!(
        parse(&both_with_empty),
        Err(LibraryBuildSpecError::Invalid(_))
    ));
}

#[test]
fn collection_requires_nonempty_directories_list() {
    let bare_list = collection_config("empty_list", "directories = []\n");
    assert_eq!(
        invalid(&bare_list),
        InvalidLibraryBuildSpec::EmptyCollectionDirectories {
            collection_handle: "empty_list".into()
        }
    );

    let list_with_metadata =
        collection_config("empty_list", "album_name = \"\"\ndirectories = []\n");
    assert_eq!(
        invalid(&list_with_metadata),
        InvalidLibraryBuildSpec::EmptyCollectionDirectories {
            collection_handle: "empty_list".into()
        }
    );
}

#[test]
fn collection_requires_nonempty_directory_path() {
    let bare_path = collection_config("empty_path", "directory = \"\"\n");
    assert_eq!(
        invalid(&bare_path),
        InvalidLibraryBuildSpec::EmptyCollectionDirectory {
            collection_handle: "empty_path".into()
        }
    );

    let path_with_metadata =
        collection_config("empty_path", "album_artist = \"\"\ndirectory = \"\"\n");
    assert_eq!(
        invalid(&path_with_metadata),
        InvalidLibraryBuildSpec::EmptyCollectionDirectory {
            collection_handle: "empty_path".into()
        }
    );
}

#[test]
fn collection_rejects_empty_directories_entry() {
    let first = collection_config("entries", "directories = [\"\", \"ok\"]\n");
    assert_eq!(
        invalid(&first),
        InvalidLibraryBuildSpec::EmptyCollectionDirectoriesEntry {
            collection_handle: "entries".into(),
            index: 0,
        }
    );

    let later = collection_config("entries", "directories = [\"ok\", \"\", \"ok\"]\n");
    assert_eq!(
        invalid(&later),
        InvalidLibraryBuildSpec::EmptyCollectionDirectoriesEntry {
            collection_handle: "entries".into(),
            index: 1,
        }
    );
}

#[test]
fn rejects_incorrect_collection_field_types() {
    rejects_toml(&collection_config("typed", "album_name = true\n"));
    rejects_toml(&collection_config("typed", "album_name = 2001\n"));
    rejects_toml(&collection_config("typed", "album_name = [\"A\"]\n"));
    rejects_toml(&collection_config("typed", "album_names = \"A\"\n"));
    rejects_toml(&collection_config("typed", "album_names = 5\n"));
    rejects_toml(&collection_config("typed", "album_names = [\"a\", 2]\n"));
    rejects_toml(&collection_config("typed", "album_names = [true]\n"));
    rejects_toml(&collection_config("typed", "album_artist = true\n"));
    rejects_toml(&collection_config("typed", "album_artist = [\"A\"]\n"));
    rejects_toml(&collection_config("typed", "album_artists = \"A\"\n"));
    rejects_toml(&collection_config("typed", "album_artists = [\"a\", 2]\n"));
    rejects_toml(&collection_config("typed", "track_artist = 2001\n"));
    rejects_toml(&collection_config("typed", "track_artist = [\"A\"]\n"));
    rejects_toml(&collection_config("typed", "track_artists = \"A\"\n"));
    rejects_toml(&collection_config("typed", "track_artists = [\"a\", 2]\n"));
    rejects_toml(&collection_config("typed", "track_artists = [true]\n"));
    rejects_toml(&collection_config("typed", "directory = 5\n"));
    rejects_toml(&collection_config("typed", "directories = \"Disc 1\"\n"));
    rejects_toml(&collection_config("typed", "directories = [\"a\", 2]\n"));
    rejects_toml(&collection_config("typed", "directories = [true]\n"));
}

#[test]
fn validation_messages_name_collection_identities_and_toml_keys() {
    assert_eq!(
        invalid(&collection_config("bare", "")).to_string(),
        "collection `bare` must set at least one of the TOML keys `album_name`, `album_names`, `album_artist`, `album_artists`, `track_artist`, `track_artists`, `directory`, or `directories`"
    );
    assert_eq!(
        invalid(&collection_config(
            "both_names",
            "album_name = \"A\"\nalbum_names = [\"B\"]\n"
        ))
        .to_string(),
        "collection `both_names` must set at most one of the TOML keys `album_name` or `album_names`"
    );
    assert_eq!(
        invalid(&collection_config(
            "both_artists",
            "album_artist = \"A\"\nalbum_artists = [\"B\"]\n"
        ))
        .to_string(),
        "collection `both_artists` must set at most one of the TOML keys `album_artist` or `album_artists`"
    );
    assert_eq!(
        invalid(&collection_config(
            "both_tracks",
            "track_artist = \"A\"\ntrack_artists = [\"B\"]\n"
        ))
        .to_string(),
        "collection `both_tracks` must set at most one of the TOML keys `track_artist` or `track_artists`"
    );
    assert_eq!(
        invalid(&collection_config("empty", "album_names = []\n")).to_string(),
        "collection `empty` has an empty `album_names` list"
    );
    assert_eq!(
        invalid(&collection_config("empty", "album_artists = []\n")).to_string(),
        "collection `empty` has an empty `album_artists` list"
    );
    assert_eq!(
        invalid(&collection_config("empty", "track_artists = []\n")).to_string(),
        "collection `empty` has an empty `track_artists` list"
    );
    assert_eq!(
        invalid(&collection_config(
            "both",
            "directory = \"A\"\ndirectories = [\"B\"]\n"
        ))
        .to_string(),
        "collection `both` must set at most one of the TOML keys `directory` or `directories`"
    );
    assert_eq!(
        invalid(&collection_config("empty", "directory = \"\"\n")).to_string(),
        "collection `empty` has an empty `directory` path"
    );
    assert_eq!(
        invalid(&collection_config("empty", "directories = []\n")).to_string(),
        "collection `empty` has an empty `directories` list"
    );
    assert_eq!(
        invalid(&collection_config("empty", "directories = [\"\"]\n")).to_string(),
        "collection `empty` has an empty `directories` entry at index 0"
    );
    assert_eq!(
        invalid(&super::prefixed(
            "[[collection_rules]]\ncollections = [\"missing\"]\nbitrate = 96\n"
        ))
        .to_string(),
        "rule references undeclared collection handle `missing`"
    );
    assert_eq!(
        invalid(&super::prefixed(
            "[[collection_rules]]\ncollections = []\nbitrate = 96\n"
        ))
        .to_string(),
        "collection rule must list at least one collection handle in `collections`"
    );
    assert_eq!(
        invalid(&super::prefixed(
            "[[collection_rules]]\ncollections = [\"aenima\", \"aenima\"]\nbitrate = 96\n"
        ))
        .to_string(),
        "collection handle `aenima` is targeted more than once by `collection_rules`"
    );
    assert_eq!(
        invalid(&super::prefixed(
            "[[track_rules]]\ncollection = \"aenima\"\nrules = [\
             { track = \"Track\", bitrate = 96 }, { track = \"Track\", exclude = true }]\n"
        ))
        .to_string(),
        "track `Track` in collection `aenima` has more than one rule"
    );
    assert_eq!(
        invalid(&super::prefixed(
            "[[track_rules]]\ncollection = \"aenima\"\n"
        ))
        .to_string(),
        "track rule group for collection `aenima` has no nested `rules`"
    );
    assert_eq!(
        invalid(&super::prefixed(
            "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ bitrate = 96 }]\n"
        ))
        .to_string(),
        "track rule for collection `aenima` must set exactly one of the TOML keys `track` or `tracks`"
    );
    assert_eq!(
        invalid(&super::prefixed(
            "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ tracks = [], bitrate = 96 }]\n"
        ))
        .to_string(),
        "track rule for collection `aenima` has an empty `tracks` list"
    );
    assert_eq!(
        invalid(&super::prefixed(
            "[[track_rules]]\ncollection = \"aenima\"\nrules = [{ track = \"Track\", exclude = false }]\n"
        ))
        .to_string(),
        "track rule for collection `aenima` must set exactly one of the TOML keys `exclude` or `bitrate`"
    );
}
