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

#[test]
fn collection_declaration_order_is_preserved() {
    // Declared out of alphabetical order: z before a
    let text = "codec = \"opus\"\nbitrate = 128\n[collections.z]\nname = \"Z\"\n[collections.a]\nname = \"A\"\n";
    let config = valid(text);

    let handles: Vec<&str> = config
        .collection_declarations()
        .map(|(handle, _)| handle)
        .collect();
    assert_eq!(handles, ["z", "a"]);
}

#[test]
fn collection_declarations_yield_ordered_handle_declaration_pairs() {
    // Different declaration fields catch mismatched pairs.
    let text = "codec = \"opus\"\nbitrate = 128\n\
                [collections.z]\nname = \"Z\"\ndirectory = \"z\"\n\
                [collections.a]\nartist = \"A\"\n\
                [collections.m]\ndirectories = [\"m1\", \"m2\"]\n";
    let config = valid(text);

    let declared: Vec<(&str, &CollectionDeclaration)> = config.collection_declarations().collect();
    assert_eq!(declared.len(), 3);

    let (handle, declaration) = declared[0];
    assert_eq!(handle, "z");
    assert_eq!(declaration.name.as_deref(), Some("Z"));
    assert_eq!(declaration.artist, None);
    assert_eq!(declaration.directories, Some(vec![PathBuf::from("z")]));

    let (handle, declaration) = declared[1];
    assert_eq!(handle, "a");
    assert_eq!(declaration.name, None);
    assert_eq!(declaration.artist.as_deref(), Some("A"));
    assert_eq!(declaration.directories, None);

    let (handle, declaration) = declared[2];
    assert_eq!(handle, "m");
    assert_eq!(declaration.name, None);
    assert_eq!(declaration.artist, None);
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
    // Dotted declarations keep z first and `a` second even though z is
    // given a name after `a` is introduced.
    let text = "codec = \"opus\"\nbitrate = 128\n\
                collections.z.directory = \"z\"\n\
                collections.a.directory = \"a\"\n\
                collections.z.name = \"Z\"\n";
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
            .name
            .as_deref(),
        Some("Z")
    );
}

#[test]
fn collection_rule_references_do_not_establish_declaration_order() {
    // The rule references `a` before its declaration, iteration still
    // follows the declaration order of z then a.
    let text = "codec = \"opus\"\nbitrate = 128\n\
                [[collection_rules]]\ncollections = [\"a\"]\nbitrate = 96\n\
                [collections.z]\nname = \"Z\"\n\
                [collections.a]\nname = \"A\"\n";
    let config = valid(text);

    let handles: Vec<&str> = config
        .collection_declarations()
        .map(|(handle, _)| handle)
        .collect();
    assert_eq!(handles, ["z", "a"]);
}

#[test]
fn collection_declaration_lookup_is_exact() {
    let config = valid(&collection_config("lateralus", "name = \"Lateralus\"\n"));

    assert_eq!(
        config
            .collection_declaration("lateralus")
            .expect("lateralus must be declared")
            .name
            .as_deref(),
        Some("Lateralus")
    );

    // Missing, differently cased, and padded handles do not match.
    assert!(config.collection_declaration("missing").is_none());
    assert!(config.collection_declaration("Lateralus").is_none());
    assert!(config.collection_declaration(" lateralus").is_none());
    assert!(config.collection_declaration("lateralus ").is_none());
}

#[test]
fn parses_optional_metadata_selectors() {
    let name_only = valid(&collection_config("name_only", "name = \"Ænima\"\n"));
    let declaration = name_only
        .collection_declaration("name_only")
        .expect("name_only must be declared");
    assert_eq!(declaration.name.as_deref(), Some("Ænima"));
    assert_eq!(declaration.artist, None);
    assert_eq!(declaration.directories, None);

    let artist_only = valid(&collection_config("artist_only", "artist = \"Tool\"\n"));
    let declaration = artist_only
        .collection_declaration("artist_only")
        .expect("artist_only must be declared");
    assert_eq!(declaration.name, None);
    assert_eq!(declaration.artist.as_deref(), Some("Tool"));
    assert_eq!(declaration.directories, None);

    let empty_name = valid(&collection_config("empty_name", "name = \"\"\n"));
    let declaration = empty_name
        .collection_declaration("empty_name")
        .expect("empty_name must be declared");
    assert_eq!(declaration.name.as_deref(), Some(""));
    assert_eq!(declaration.artist, None);
    assert_eq!(declaration.directories, None);

    let empty_artist = valid(&collection_config("empty_artist", "artist = \"\"\n"));
    let declaration = empty_artist
        .collection_declaration("empty_artist")
        .expect("empty_artist must be declared");
    assert_eq!(declaration.name, None);
    assert_eq!(declaration.artist.as_deref(), Some(""));
    assert_eq!(declaration.directories, None);
}

#[test]
fn collection_metadata_values_are_preserved_exactly() {
    let whitespace = valid(&collection_config("blank", "name = \"   \"\n"));
    assert_eq!(
        whitespace
            .collection_declaration("blank")
            .expect("blank must be declared")
            .name
            .as_deref(),
        Some("   ")
    );

    let with_artist = valid(&collection_config(
        "blank",
        "name = \"\"\nartist = \"  \"\n",
    ));
    let declaration = with_artist
        .collection_declaration("blank")
        .expect("blank must be declared");
    assert_eq!(declaration.name.as_deref(), Some(""));
    assert_eq!(declaration.artist.as_deref(), Some("  "));
    assert_eq!(declaration.directories, None);

    let with_directory = valid(&collection_config(
        "blank",
        "name = \"\"\ndirectory = \"Undertow\"\n",
    ));
    let declaration = with_directory
        .collection_declaration("blank")
        .expect("blank must be declared");
    assert_eq!(declaration.name.as_deref(), Some(""));
    assert_eq!(declaration.artist, None);
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
    assert_eq!(declaration.name, None);
    assert_eq!(declaration.artist, None);
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
    assert_eq!(declaration.name, None);
    assert_eq!(declaration.artist, None);
    assert_eq!(
        declaration.directories,
        Some(vec![PathBuf::from("Disc 1"), PathBuf::from("Disc 2")])
    );
}

#[test]
fn parses_metadata_with_directory_selectors() {
    let config = valid(&collection_config(
        "mixed",
        "name = \"Lateralus\"\nartist = \"Tool\"\ndirectories = [\"Disc 1\", \"Disc 2\"]\n",
    ));
    let declaration = config
        .collection_declaration("mixed")
        .expect("mixed must be declared");
    assert_eq!(declaration.name.as_deref(), Some("Lateralus"));
    assert_eq!(declaration.artist.as_deref(), Some("Tool"));
    assert_eq!(
        declaration.directories,
        Some(vec![PathBuf::from("Disc 1"), PathBuf::from("Disc 2")])
    );
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
    assert_eq!(declaration.name, None);
    assert_eq!(declaration.artist, None);
    assert_eq!(
        declaration.directories,
        Some(vec![PathBuf::from("Undertow")])
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

    let list_with_metadata = collection_config("empty_list", "name = \"\"\ndirectories = []\n");
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

    let path_with_metadata = collection_config("empty_path", "artist = \"\"\ndirectory = \"\"\n");
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
    rejects_toml(&collection_config("typed", "name = true\n"));
    rejects_toml(&collection_config("typed", "artist = 2001\n"));
    rejects_toml(&collection_config("typed", "directory = 5\n"));
    rejects_toml(&collection_config("typed", "directories = \"Disc 1\"\n"));
    rejects_toml(&collection_config("typed", "directories = [\"a\", 2]\n"));
    rejects_toml(&collection_config("typed", "directories = [true]\n"));
}

#[test]
fn validation_messages_name_collection_identities_and_toml_keys() {
    assert_eq!(
        invalid(&collection_config("bare", "")).to_string(),
        "collection `bare` must set at least one of the TOML keys `name`, `artist`, `directory`, or `directories`"
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
