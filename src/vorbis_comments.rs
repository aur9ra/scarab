/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Converts decoded Vorbis-comment entries into reader-independent
//! metadata-selector observations.
//!
//! Each entry is an already decoded field-name/value pair, not a raw
//! `FIELD=value` record. This module does not read media, extract metadata, or
//! evaluate selectors. Selector matching is implemented in
//! `metadata_selection`.

use std::collections::BTreeSet;

use crate::metadata_selection::MetadataSelectorObservation;

/// Metadata family selected by a Vorbis-comment field name.
enum SelectorFamily {
    AlbumNames,
    AlbumArtists,
    TrackArtists,
}

/// Returns the selector family for a recognized field name.
///
/// Comparison ignores ASCII case only. All other field names return `None`
/// and are ignored by the projector.
fn classify_field(field_name: &str) -> Option<SelectorFamily> {
    if field_name.eq_ignore_ascii_case("ALBUM") {
        Some(SelectorFamily::AlbumNames)
    } else if field_name.eq_ignore_ascii_case("ARTIST")
        || field_name.eq_ignore_ascii_case("ARTISTS")
    {
        Some(SelectorFamily::TrackArtists)
    } else if field_name.eq_ignore_ascii_case("ALBUMARTIST")
        || field_name.eq_ignore_ascii_case("ALBUM_ARTIST")
        || field_name.eq_ignore_ascii_case("ALBUM ARTIST")
        || field_name.eq_ignore_ascii_case("ALBUMARTISTS")
        || field_name.eq_ignore_ascii_case("ALBUM_ARTISTS")
        || field_name.eq_ignore_ascii_case("ALBUM ARTISTS")
    {
        Some(SelectorFamily::AlbumArtists)
    } else {
        None
    }
}

/// Projects a complete sequence of decoded entries into a selector observation.
///
/// Field names must exactly match a recognized alias apart from ASCII case.
/// Unknown names are ignored. Values are stored unchanged. Exact duplicates
/// collapse within each family, while distinct strings stay separate even if
/// selector comparison treats them as equal.
///
/// The caller must provide all entries from successful extraction. This
/// function cannot detect failed or incomplete extraction. Do not use
/// shortened or empty input to represent either condition. A complete empty
/// sequence yields three empty families.
// No production code calls this projector yet.
#[allow(dead_code)]
pub(crate) fn project_metadata_selector_observation<'a>(
    entries: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> MetadataSelectorObservation {
    let mut album_names: BTreeSet<String> = BTreeSet::new();
    let mut album_artists: BTreeSet<String> = BTreeSet::new();
    let mut track_artists: BTreeSet<String> = BTreeSet::new();

    for (field_name, value) in entries {
        match classify_field(field_name) {
            Some(SelectorFamily::AlbumNames) => {
                album_names.insert(value.to_owned());
            }
            Some(SelectorFamily::AlbumArtists) => {
                album_artists.insert(value.to_owned());
            }
            Some(SelectorFamily::TrackArtists) => {
                track_artists.insert(value.to_owned());
            }
            None => {}
        }
    }

    MetadataSelectorObservation::new(album_names, album_artists, track_artists)
}

#[cfg(test)]
#[path = "tests/vorbis_comments.rs"]
mod tests;
