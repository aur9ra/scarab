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
mod tests {
    use super::*;
    use crate::metadata_selection::{MetadataSelectorPredicate, observation_matches_predicate};

    fn values(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }

    fn expected_observation(
        album_names: &[&str],
        album_artists: &[&str],
        track_artists: &[&str],
    ) -> MetadataSelectorObservation {
        MetadataSelectorObservation::new(
            values(album_names),
            values(album_artists),
            values(track_artists),
        )
    }

    fn project(entries: &[(&str, &str)]) -> MetadataSelectorObservation {
        project_metadata_selector_observation(entries.iter().copied())
    }

    #[test]
    fn every_alias_maps_to_its_intended_family() {
        let cases = [
            (
                "ALBUM",
                &["White Pony"] as &[&str],
                &[] as &[&str],
                &[] as &[&str],
            ),
            (
                "ARTIST",
                &[] as &[&str],
                &[] as &[&str],
                &["Deftones"] as &[&str],
            ),
            (
                "ARTISTS",
                &[] as &[&str],
                &[] as &[&str],
                &["Deftones"] as &[&str],
            ),
            (
                "ALBUMARTIST",
                &[] as &[&str],
                &["Failure"] as &[&str],
                &[] as &[&str],
            ),
            (
                "ALBUM_ARTIST",
                &[] as &[&str],
                &["Failure"] as &[&str],
                &[] as &[&str],
            ),
            (
                "ALBUM ARTIST",
                &[] as &[&str],
                &["Failure"] as &[&str],
                &[] as &[&str],
            ),
            (
                "ALBUMARTISTS",
                &[] as &[&str],
                &["Failure"] as &[&str],
                &[] as &[&str],
            ),
            (
                "ALBUM_ARTISTS",
                &[] as &[&str],
                &["Failure"] as &[&str],
                &[] as &[&str],
            ),
            (
                "ALBUM ARTISTS",
                &[] as &[&str],
                &["Failure"] as &[&str],
                &[] as &[&str],
            ),
        ];
        for (field_name, album_names, album_artists, track_artists) in cases {
            let value = if field_name == "ALBUM" {
                "White Pony"
            } else if field_name == "ARTIST" || field_name == "ARTISTS" {
                "Deftones"
            } else {
                "Failure"
            };
            assert_eq!(
                project(&[(field_name, value)]),
                expected_observation(album_names, album_artists, track_artists),
                "field {field_name} must map to its intended family only"
            );
        }
    }

    #[test]
    fn every_alias_is_recognized_in_upper_lower_and_mixed_ascii_case() {
        let canonical: &[&str] = &[
            "ALBUM",
            "ARTIST",
            "ARTISTS",
            "ALBUMARTIST",
            "ALBUM_ARTIST",
            "ALBUM ARTIST",
            "ALBUMARTISTS",
            "ALBUM_ARTISTS",
            "ALBUM ARTISTS",
        ];
        let mixed: &[&str] = &[
            "aLbUm",
            "aRtIsT",
            "aRtIsTs",
            "aLbUmArTiSt",
            "aLbUm_aRtIsT",
            "aLbUm aRtIsT",
            "aLbUmArTiStS",
            "aLbUm_aRtIsTs",
            "aLbUm aRtIsTs",
        ];
        for (upper, mixed_spelling) in canonical.iter().zip(mixed.iter()) {
            let lower = upper.to_lowercase();
            assert_ne!(upper.to_string(), lower);
            assert_ne!(upper.to_string(), mixed_spelling.to_string());
            assert_ne!(lower, mixed_spelling.to_string());
            for spelling in [upper.to_string(), lower, (*mixed_spelling).to_owned()] {
                let value = "Deftones";
                let observed = project(&[(spelling.as_str(), value)]);
                let expected = if upper == &"ALBUM" {
                    expected_observation(&[value], &[], &[])
                } else if upper == &"ARTIST" || upper == &"ARTISTS" {
                    expected_observation(&[], &[], &[value])
                } else {
                    expected_observation(&[], &[value], &[])
                };
                assert_eq!(
                    observed, expected,
                    "spelling {spelling:?} of {upper} must be recognized"
                );
            }
        }
    }

    #[test]
    fn unknown_and_near_miss_field_names_are_rejected() {
        let rejected: &[&str] = &[
            "TITLE",
            "GENRE",
            "DATE",
            "ALBUMS",
            "ALBUM  ARTISTS",
            " ALBUM",
            "ALBUM ",
            "  ALBUM  ",
            "\tARTIST",
            "ARTIST\n",
            " ALBUMARTIST ",
            "ALBUM-ARTIST",
            "ALBUM.ARTIST",
            "ALBUM/ARTIST",
            "ALBUM:ARTIST",
            "ALBUM__ARTIST",
            "ALBUM_ARTIST_",
            "_ALBUM_ARTIST",
            "XALBUM",
            "ALBUMX",
            "ARTISTX",
            "XARTIST",
            "ALBUMARTISTX",
            "ALBUM ARTIST ",
            " ALBUM ARTIST",
            "ALBUM  ARTIST",
            "ALBUMARTISTS ",
            "\u{FF21}LBUM",
            "ART\u{0130}ST",
            "ALBUM\u{00E9}",
        ];
        for field_name in rejected {
            assert_eq!(
                project(&[(field_name, "Deftones")]),
                expected_observation(&[], &[], &[]),
                "field {field_name:?} must be ignored"
            );
        }
        assert_eq!(
            project(&[
                ("ALBUMS", "Deftones"),
                ("ALBUM  ARTISTS", "Deftones"),
                (" ALBUM", "Deftones"),
                ("TITLE", "Deftones"),
            ]),
            expected_observation(&[], &[], &[]),
            "unknown only input with near misses must yield empty families"
        );
    }

    #[test]
    fn families_are_isolated() {
        assert_eq!(
            project(&[
                ("ALBUM", "White Pony"),
                ("ALBUMARTIST", "Deftones"),
                ("ARTIST", "Terry Date"),
            ]),
            expected_observation(&["White Pony"], &["Deftones"], &["Terry Date"]),
            "each value must land in its own family only"
        );
        assert_eq!(
            project(&[("ALBUM", "Fantastic Planet")]),
            expected_observation(&["Fantastic Planet"], &[], &[]),
            "album entry must leave other families empty"
        );
        assert_eq!(
            project(&[("ALBUM_ARTIST", "Deftones")]),
            expected_observation(&[], &["Deftones"], &[]),
            "album artist entry must leave other families empty"
        );
        assert_eq!(
            project(&[("ARTISTS", "Hum")]),
            expected_observation(&[], &[], &["Hum"]),
            "track artist entry must leave other families empty"
        );
    }

    #[test]
    fn repeated_entries_accumulate() {
        assert_eq!(
            project(&[("ALBUM", "White Pony"), ("ALBUM", "Fantastic Planet")]),
            expected_observation(&["White Pony", "Fantastic Planet"], &[], &[]),
            "repeated album entries must accumulate distinct values"
        );
        assert_eq!(
            project(&[
                ("ALBUMARTIST", "Deftones"),
                ("ALBUMARTIST", "Failure"),
                ("ALBUMARTIST", "Hum"),
            ]),
            expected_observation(&[], &["Deftones", "Failure", "Hum"], &[]),
            "repeated album artist entries must accumulate distinct values"
        );
        assert_eq!(
            project(&[("ARTIST", "Deftones"), ("ARTIST", "Failure")]),
            expected_observation(&[], &[], &["Deftones", "Failure"]),
            "repeated track artist entries must accumulate distinct values"
        );
    }

    #[test]
    fn different_aliases_within_one_family_accumulate() {
        assert_eq!(
            project(&[
                ("ALBUMARTIST", "Deftones"),
                ("ALBUM_ARTIST", "Failure"),
                ("ALBUM ARTIST", "Hum"),
                ("ALBUMARTISTS", "Radiohead"),
                ("ALBUM_ARTISTS", "The Cure"),
                ("ALBUM ARTISTS", "Björk"),
            ]),
            expected_observation(
                &[],
                &[
                    "Deftones",
                    "Failure",
                    "Hum",
                    "Radiohead",
                    "The Cure",
                    "Björk"
                ],
                &[],
            ),
            "album artist aliases must accumulate rather than replace"
        );
        assert_eq!(
            project(&[("ARTIST", "Deftones"), ("ARTISTS", "Failure")]),
            expected_observation(&[], &[], &["Deftones", "Failure"]),
            "track artist aliases must accumulate rather than replace"
        );
    }

    #[test]
    fn exact_duplicate_raw_values_collapse() {
        assert_eq!(
            project(&[
                ("ALBUM", "Deftones"),
                ("ALBUM", "Deftones"),
                ("ALBUM", "Deftones"),
            ]),
            expected_observation(&["Deftones"], &[], &[]),
            "exact duplicate album values must collapse"
        );
        assert_eq!(
            project(&[
                ("ALBUMARTIST", "Deftones"),
                ("ALBUM_ARTIST", "Deftones"),
                ("ALBUM ARTIST", "Deftones"),
            ]),
            expected_observation(&[], &["Deftones"], &[]),
            "duplicates through different album artist aliases must collapse"
        );
        assert_eq!(
            project(&[("ARTIST", "Deftones"), ("ARTISTS", "Deftones")]),
            expected_observation(&[], &[], &["Deftones"]),
            "duplicates through different track artist aliases must collapse"
        );
    }

    #[test]
    fn same_raw_value_is_independent_in_each_family() {
        assert_eq!(
            project(&[
                ("ALBUM", "Deftones"),
                ("ALBUMARTIST", "Deftones"),
                ("ARTIST", "Deftones"),
            ]),
            expected_observation(&["Deftones"], &["Deftones"], &["Deftones"]),
            "the same raw string must be present independently in each family"
        );
    }

    #[test]
    fn composed_and_decomposed_unicode_stay_raw_distinct() {
        let composed = "Bj\u{00F6}rk";
        let decomposed = "Bjo\u{0308}rk";
        assert_ne!(composed, decomposed);
        assert_eq!(
            project(&[("ALBUM", composed), ("ALBUM", decomposed)]),
            expected_observation(&[composed, decomposed], &[], &[]),
            "composed and decomposed album names must stay distinct"
        );
        assert_eq!(
            project(&[("ALBUMARTIST", composed), ("ALBUMARTIST", decomposed)]),
            expected_observation(&[], &[composed, decomposed], &[]),
            "composed and decomposed album artists must stay distinct"
        );
        assert_eq!(
            project(&[("ARTIST", composed), ("ARTIST", decomposed)]),
            expected_observation(&[], &[], &[composed, decomposed]),
            "composed and decomposed track artists must stay distinct"
        );
        let hangul_composed = "\u{AC00}";
        let hangul_decomposed = "\u{1100}\u{1161}";
        assert_eq!(
            project(&[("ALBUM", hangul_composed), ("ALBUM", hangul_decomposed)]),
            expected_observation(&[hangul_composed, hangul_decomposed], &[], &[]),
            "canonically equivalent Hangul spellings must stay distinct"
        );
    }

    #[test]
    fn trailing_nul_variants_stay_raw_distinct() {
        let cases: &[&[&str]] = &[&["Track", "Track\0"], &["Track", "Track\0", "Track\0\0"]];
        for stored in cases {
            assert_eq!(
                project(
                    &stored
                        .iter()
                        .map(|value| ("ALBUM", *value))
                        .collect::<Vec<(&str, &str)>>()
                ),
                expected_observation(stored, &[], &[]),
                "trailing NUL variants must stay distinct in album names"
            );
            assert_eq!(
                project(
                    &stored
                        .iter()
                        .map(|value| ("ALBUMARTIST", *value))
                        .collect::<Vec<(&str, &str)>>()
                ),
                expected_observation(&[], stored, &[]),
                "trailing NUL variants must stay distinct in album artists"
            );
            assert_eq!(
                project(
                    &stored
                        .iter()
                        .map(|value| ("ARTIST", *value))
                        .collect::<Vec<(&str, &str)>>()
                ),
                expected_observation(&[], &[], stored),
                "trailing NUL variants must stay distinct in track artists"
            );
        }
    }

    #[test]
    fn raw_values_are_preserved_in_all_three_families() {
        let corpus: &[&str] = &[
            "",
            " ",
            "   ",
            "  padded  ",
            "\t",
            "AC/DC",
            "Earth, Wind & Fire",
            "Artist A; Artist B",
            "Artist A;Artist B",
            "\0",
            "\0\0",
            "\0leading",
            "embed\0ded",
            "trailing\0",
            "trailing\0\0",
            "Bj\u{00F6}rk",
            "Bjo\u{0308}rk",
            "\u{00E9}",
            "e\u{0301}",
            "\u{1100}\u{1161}",
            "\u{AC00}",
        ];
        let album_entries: Vec<(&str, &str)> =
            corpus.iter().map(|value| ("ALBUM", *value)).collect();
        assert_eq!(
            project(&album_entries),
            expected_observation(corpus, &[], &[]),
            "album values must be preserved unchanged"
        );
        let album_artist_entries: Vec<(&str, &str)> =
            corpus.iter().map(|value| ("ALBUMARTIST", *value)).collect();
        assert_eq!(
            project(&album_artist_entries),
            expected_observation(&[], corpus, &[]),
            "album artist values must be preserved unchanged"
        );
        let track_artist_entries: Vec<(&str, &str)> =
            corpus.iter().map(|value| ("ARTISTS", *value)).collect();
        assert_eq!(
            project(&track_artist_entries),
            expected_observation(&[], &[], corpus),
            "track artist values must be preserved unchanged"
        );
        let preserved = project(&[
            ("ALBUM", "Artist A; Artist B"),
            ("ALBUMARTIST", "  padded  "),
            ("ARTIST", "embed\0ded"),
        ]);
        assert_eq!(
            preserved,
            expected_observation(&["Artist A; Artist B"], &["  padded  "], &["embed\0ded"]),
            "representative raw spellings must survive projection"
        );
    }

    #[test]
    fn absent_values_differ_from_present_empty_string() {
        assert_eq!(
            project(&[]),
            expected_observation(&[], &[], &[]),
            "empty input is the absent baseline"
        );
        assert_eq!(
            project(&[("ALBUM", "")]),
            expected_observation(&[""], &[], &[]),
            "present empty album name must differ from absence"
        );
        assert_eq!(
            project(&[("ALBUMARTIST", "")]),
            expected_observation(&[], &[""], &[]),
            "present empty album artist must differ from absence"
        );
        assert_eq!(
            project(&[("ARTIST", "")]),
            expected_observation(&[], &[], &[""]),
            "present empty track artist must differ from absence"
        );
        assert_ne!(
            project(&[]),
            project(&[("ALBUM", "")]),
            "absent album names must differ from a present empty string"
        );
    }

    #[test]
    fn empty_input_yields_three_empty_sets() {
        assert_eq!(
            project(&[]),
            expected_observation(&[], &[], &[]),
            "complete empty input must yield three empty families"
        );
        assert_eq!(
            project_metadata_selector_observation(Vec::new().iter().copied()),
            expected_observation(&[], &[], &[]),
            "empty owned input must yield three empty families"
        );
    }

    #[test]
    fn unknown_only_input_yields_three_empty_sets() {
        assert_eq!(
            project(&[
                ("TITLE", "White Pony"),
                ("GENRE", "Alternative metal"),
                ("ALBUMS", "Failure"),
                ("ALBUM  ARTISTS", "Failure"),
            ]),
            expected_observation(&[], &[], &[]),
            "unknown only input must yield three empty families"
        );
    }

    #[test]
    fn entry_order_does_not_change_the_observation() {
        let forward: &[(&str, &str)] = &[
            ("ALBUM", "Blue"),
            ("ARTIST", "Coltrane"),
            ("ALBUMARTIST", "Davis"),
            ("ALBUM", "Blue"),
            ("TITLE", "Bitches Brew"),
            ("ARTISTS", "Evans"),
        ];
        let reverse: &[(&str, &str)] = &[
            ("ARTISTS", "Evans"),
            ("TITLE", "Bitches Brew"),
            ("ALBUM", "Blue"),
            ("ALBUMARTIST", "Davis"),
            ("ARTIST", "Coltrane"),
            ("ALBUM", "Blue"),
        ];
        assert_eq!(
            project(forward),
            project(reverse),
            "reordering entries must not change the observation"
        );
        assert_eq!(
            project(forward),
            expected_observation(&["Blue"], &["Davis"], &["Coltrane", "Evans"]),
            "reordered input must match the explicit raw expectation"
        );
    }

    #[test]
    fn projected_observation_supports_selector_matching() {
        let observation = project(&[
            ("ALBUM", "Kind of Blue"),
            ("ALBUMARTIST", "Miles Davis"),
            ("ARTIST", "John Coltrane"),
        ]);
        assert_eq!(
            observation,
            expected_observation(&["Kind of Blue"], &["Miles Davis"], &["John Coltrane"]),
            "composition input must project to the explicit raw expectation"
        );
        let matching = MetadataSelectorPredicate::new(
            Some(vec!["Kind of Blue".to_owned()]),
            Some(vec!["Miles Davis".to_owned()]),
            Some(vec!["John Coltrane".to_owned()]),
        );
        assert!(observation_matches_predicate(&observation, &matching));
        let mismatching =
            MetadataSelectorPredicate::new(Some(vec!["Bitches Brew".to_owned()]), None, None);
        assert!(!observation_matches_predicate(&observation, &mismatching));
    }
}
