/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! The `[files]` table: file selection via `album_art`, `include`,
//! and `exclude`.

use super::valid;

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
