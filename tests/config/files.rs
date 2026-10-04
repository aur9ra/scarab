/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! The `[files]` table: file-handling options `album_art`, `include`, and `exclude`.

use scarab::FilesConfig;

use super::valid;

#[test]
fn parses_file_handling_options() {
    let included = "codec = \"opus\"\nbitrate = 128\n[files]\nalbum_art = true\ninclude = [\"jpg\", \"png\"]\n";
    let config = valid(included);
    let files = config.files();

    assert_eq!(files.album_art, Some(true));
    assert_eq!(
        files.include,
        Some(vec!["jpg".to_string(), "png".to_string()])
    );
    assert_eq!(files.exclude, None);

    let excluded = "codec = \"opus\"\nbitrate = 128\n[files]\nexclude = [\"cue\"]\n";
    let config = valid(excluded);
    let files = config.files();

    assert_eq!(files.album_art, None);
    assert_eq!(files.include, None);
    assert_eq!(files.exclude, Some(vec!["cue".to_string()]));
}

#[test]
fn parses_files_include_and_exclude_together() {
    let text = "codec = \"opus\"\nbitrate = 128\n[files]\ninclude = [\"jpg\", \"png\"]\nexclude = [\"cue\"]\n";
    let config = valid(text);
    let files = config.files();

    assert_eq!(
        files.include,
        Some(vec!["jpg".to_string(), "png".to_string()])
    );
    assert_eq!(files.exclude, Some(vec!["cue".to_string()]));
}

#[test]
fn default_files_support_ordinary_field_mutation() {
    let mut files = FilesConfig::default();
    assert_eq!(files.album_art, None);
    assert_eq!(files.include, None);
    assert_eq!(files.exclude, None);

    files.album_art = Some(true);
    files.include = Some(vec!["jpg".to_string()]);
    files.exclude = Some(vec!["cue".to_string()]);

    assert_eq!(files.album_art, Some(true));
    assert_eq!(files.include, Some(vec!["jpg".to_string()]));
    assert_eq!(files.exclude, Some(vec!["cue".to_string()]));
}
