/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::*;

#[test]
fn has_metadata_selectors_checks_presence_not_alternatives() {
    let omitted = CollectionDeclaration {
        album_names: None,
        album_artists: None,
        track_artists: None,
        directories: Some(vec![PathBuf::from("Album")]),
    };
    assert!(!omitted.has_metadata_selectors());
    let mut no_directories = omitted.clone();
    no_directories.directories = None;
    assert!(!no_directories.has_metadata_selectors());

    for family in 0..3 {
        for alternatives in [vec!["value".to_owned()], vec![], vec![String::new()]] {
            let mut declaration = omitted.clone();
            let selector = match family {
                0 => &mut declaration.album_names,
                1 => &mut declaration.album_artists,
                2 => &mut declaration.track_artists,
                _ => unreachable!(),
            };
            *selector = Some(alternatives);
            assert!(declaration.has_metadata_selectors(), "{declaration:?}");
        }
    }
}
