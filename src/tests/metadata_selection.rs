/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::*;

fn observation(
    album_names: &[&str],
    album_artists: &[&str],
    track_artists: &[&str],
) -> MetadataSelectorObservation {
    MetadataSelectorObservation::new(
        to_observation_values(album_names),
        to_observation_values(album_artists),
        to_observation_values(track_artists),
    )
}

fn predicate(
    album_names: Option<&[&str]>,
    album_artists: Option<&[&str]>,
    track_artists: Option<&[&str]>,
) -> MetadataSelectorPredicate {
    MetadataSelectorPredicate::new(
        to_predicate_values(album_names),
        to_predicate_values(album_artists),
        to_predicate_values(track_artists),
    )
}

fn to_observation_values(observation_values: &[&str]) -> BTreeSet<String> {
    observation_values
        .iter()
        .map(|observation_value| (*observation_value).to_owned())
        .collect()
}

fn to_predicate_values(predicate_values: Option<&[&str]>) -> Option<Vec<String>> {
    predicate_values.map(|predicate_value_slice| {
        predicate_value_slice
            .iter()
            .map(|predicate_value| (*predicate_value).to_owned())
            .collect()
    })
}

#[test]
fn identical_values_match() {
    assert!(metadata_values_match("Kind of Blue", "Kind of Blue"));
    assert!(metadata_values_match("Bj\u{00F6}rk", "Bj\u{00F6}rk"));
}

#[test]
fn differing_case_does_not_match() {
    assert!(!metadata_values_match("Kind of Blue", "kind of blue"));
    assert!(!metadata_values_match("RADIOHEAD", "Radiohead"));
}

#[test]
fn canonically_equivalent_values_match() {
    // ö (U+00F6 LATIN SMALL LETTER O WITH DIAERESIS) versus o + U+0308
    // COMBINING DIAERESIS:  ̈
    let composed = "Bj\u{00F6}rk";
    let decomposed = "Bjo\u{0308}rk";
    assert_ne!(composed, decomposed);
    assert!(metadata_values_match(composed, decomposed));
    assert!(metadata_values_match(decomposed, composed));

    // U+AC00 HANGUL SYLLABLE GA: 가
    // vs
    // U+1100 HANGUL CHOSEONG KIYEOK: ᄀ
    // followed by
    // U+1161 HANGUL JUNGSEONG A: ᅡ
    assert!(metadata_values_match("\u{AC00}", "\u{1100}\u{1161}"));
}

#[test]
fn genuinely_different_unicode_remains_distinct() {
    // ö (U+00F6) is distinct from o. Å (U+00C5) is distinct from A.
    assert!(!metadata_values_match("Bj\u{00F6}rk", "Bjork"));
    assert!(!metadata_values_match("\u{00C5}ngstr\u{00F6}m", "Angstrom"));
    // Hangul 가 (U+AC00) is distinct from 각 (U+AC01).
    assert!(!metadata_values_match("\u{AC00}", "\u{AC01}"));
}

#[test]
fn single_and_multiple_trailing_nuls_are_ignored() {
    assert!(metadata_values_match("Track\0", "Track"));
    assert!(metadata_values_match("Track\0\0\0", "Track"));
    assert!(metadata_values_match("Track\0", "Track\0\0"));
    assert!(metadata_values_match("Track", "Track\0\0\0"));
}

#[test]
fn trailing_nul_handling_is_symmetric() {
    assert!(metadata_values_match("Track\0", "Track"));
    assert!(metadata_values_match("Track", "Track\0"));
    assert!(metadata_values_match("Track\0\0", "Track\0"));
    assert!(metadata_values_match("Track\0", "Track\0\0"));
}

#[test]
fn empty_and_nul_only_values_match() {
    assert!(metadata_values_match("", ""));
    assert!(metadata_values_match("", "\0"));
    assert!(metadata_values_match("\0", ""));
    assert!(metadata_values_match("", "\0\0\0"));
    assert!(metadata_values_match("\0\0", "\0"));
}

#[test]
fn embedded_and_leading_nuls_remain_significant() {
    assert!(!metadata_values_match("A\0B", "AB"));
    assert!(!metadata_values_match("Track\0B", "TrackB"));
    assert!(!metadata_values_match("\0Track", "Track"));
    assert!(!metadata_values_match("Track", "\0Track"));
    assert!(metadata_values_match("A\0B", "A\0B"));
}

#[test]
fn whitespace_differences_remain_significant() {
    assert!(!metadata_values_match("The Wall", "The  Wall"));
    assert!(!metadata_values_match("The Wall", "The Wall "));
    assert!(!metadata_values_match("The Wall", " The Wall"));
    assert!(!metadata_values_match("The Wall", "The\tWall"));
}

#[test]
fn punctuation_and_semicolons_receive_no_special_treatment() {
    assert!(!metadata_values_match("AC/DC", "ACDC"));
    assert!(!metadata_values_match(
        "Earth, Wind & Fire",
        "Earth Wind & Fire"
    ));
    assert!(!metadata_values_match("Artist A; Artist B", "Artist A"));
    assert!(!metadata_values_match(
        "Artist A; Artist B",
        "Artist A;Artist B"
    ));
    assert!(metadata_values_match(
        "Artist A; Artist B",
        "Artist A; Artist B"
    ));
}

#[test]
fn compatibility_equivalent_unicode_remains_distinct() {
    // U+FB01 LATIN SMALL LIGATURE FI: ﬁ
    assert!(!metadata_values_match("\u{FB01}", "fi"));
    // U+2460 CIRCLED DIGIT ONE: ①
    assert!(!metadata_values_match("\u{2460}", "1"));
    // U+00B5 MICRO SIGN: µ versus U+03BC GREEK SMALL LETTER MU: μ
    assert!(!metadata_values_match("\u{00B5}", "\u{03BC}"));
    // U+FF21 FULLWIDTH LATIN CAPITAL LETTER A: Ａ
    assert!(!metadata_values_match("\u{FF21}", "A"));
}

#[test]
fn matching_is_whole_value() {
    assert!(!metadata_values_match("Kind of Blue", "Kind"));
    assert!(!metadata_values_match("Kind of Blue", "of Blue"));
    assert!(metadata_values_match("Kind of Blue", "Kind of Blue"));
}

#[test]
fn canonical_equivalence_composes_with_trailing_nul_removal() {
    // e + U+0301 COMBINING ACUTE ACCENT: ◌́ (dotted circle is a display aid)
    // versus U+00E9 LATIN SMALL LETTER E WITH ACUTE: é
    assert!(metadata_values_match("e\u{0301}\0", "\u{00E9}"));
    assert!(metadata_values_match("e\u{0301}\0\0", "\u{00E9}\0"));
    // Hangul ᄀ + ᅡ (U+1100 + U+1161) versus 가 (U+AC00), with a trailing NUL.
    assert!(metadata_values_match("\u{1100}\u{1161}\0", "\u{AC00}"));
    assert!(!metadata_values_match("e\0\u{0301}", "\u{00E9}"));
}

#[test]
fn comparison_is_symmetric() {
    let pairs = [
        ("Track", "Track"),
        ("Track\0", "Track"),
        ("Bjo\u{0308}rk\0", "Bj\u{00F6}rk"),
        ("Track", "track"),
        ("A\0B", "AB"),
    ];

    for (left, right) in pairs {
        assert_eq!(
            metadata_values_match(left, right),
            metadata_values_match(right, left),
            "comparison disagreed for {left:?} and {right:?}"
        );
    }
}

#[test]
fn omitted_families_impose_no_constraint() {
    let all_omitted = predicate(None, None, None);
    assert!(observation_matches_predicate(
        &observation(&[], &[], &[]),
        &all_omitted
    ));
    assert!(observation_matches_predicate(
        &observation(&["Kind of Blue"], &["Miles Davis"], &["John Coltrane"]),
        &all_omitted
    ));

    // The omitted families do not restrict their observation values.
    let names_only = predicate(Some(&["Kind of Blue"]), None, None);
    assert!(observation_matches_predicate(
        &observation(&["Kind of Blue"], &["Anything"], &["Anything Else"]),
        &names_only
    ));
}

#[test]
fn later_predicate_values_and_observation_values_establish_a_match() {
    let complete_observation = observation(&["Kind of Blue", "A Love Supreme"], &[], &[]);

    // A later predicate value can match when the first does not.
    assert!(observation_matches_predicate(
        &complete_observation,
        &predicate(Some(&["Bitches Brew", "A Love Supreme"]), None, None)
    ));

    // A single predicate value can match an observation value.
    assert!(observation_matches_predicate(
        &complete_observation,
        &predicate(Some(&["A Love Supreme"]), None, None)
    ));

    // Matching must also reach an observation value after the first one
    // visited in set iteration order.
    assert!(observation_matches_predicate(
        &complete_observation,
        &predicate(Some(&["Kind of Blue"]), None, None)
    ));

    assert!(!observation_matches_predicate(
        &complete_observation,
        &predicate(Some(&["Bitches Brew", "Mingus Ah Um"]), None, None)
    ));
}

#[test]
fn supplied_families_must_all_match() {
    let complete_observation = observation(&["Kind of Blue"], &["Miles Davis"], &["John Coltrane"]);

    assert!(observation_matches_predicate(
        &complete_observation,
        &predicate(
            Some(&["Kind of Blue"]),
            Some(&["Miles Davis"]),
            Some(&["John Coltrane"]),
        )
    ));

    let failing_compound_predicates = [
        predicate(
            Some(&["Bitches Brew"]),
            Some(&["Miles Davis"]),
            Some(&["John Coltrane"]),
        ),
        predicate(
            Some(&["Kind of Blue"]),
            Some(&["Bill Evans"]),
            Some(&["John Coltrane"]),
        ),
        predicate(
            Some(&["Kind of Blue"]),
            Some(&["Miles Davis"]),
            Some(&["Cannonball Adderley"]),
        ),
    ];
    for failing in &failing_compound_predicates {
        assert!(!observation_matches_predicate(
            &complete_observation,
            failing
        ));
    }
}

#[test]
fn an_observation_value_only_matches_predicate_values_in_its_family() {
    let complete_observation = observation(&["Kind of Blue"], &["Miles Davis"], &["John Coltrane"]);
    let own_predicate = predicate(
        Some(&["Kind of Blue"]),
        Some(&["Miles Davis"]),
        Some(&["John Coltrane"]),
    );
    assert!(observation_matches_predicate(
        &complete_observation,
        &own_predicate
    ));

    // These observation values occur, but only in other families.
    assert!(!observation_matches_predicate(
        &complete_observation,
        &predicate(Some(&["Miles Davis"]), None, None)
    ));
    assert!(!observation_matches_predicate(
        &complete_observation,
        &predicate(Some(&["John Coltrane"]), None, None)
    ));
    assert!(!observation_matches_predicate(
        &complete_observation,
        &predicate(None, Some(&["Kind of Blue"]), None)
    ));
    assert!(!observation_matches_predicate(
        &complete_observation,
        &predicate(None, Some(&["John Coltrane"]), None)
    ));
    assert!(!observation_matches_predicate(
        &complete_observation,
        &predicate(None, None, Some(&["Kind of Blue"]))
    ));
    assert!(!observation_matches_predicate(
        &complete_observation,
        &predicate(None, None, Some(&["Miles Davis"]))
    ));
}

#[test]
fn supplied_empty_predicate_value_lists_are_unsatisfiable() {
    let complete_observation = observation(&["Kind of Blue"], &["Miles Davis"], &["John Coltrane"]);

    // An empty list is a supplied constraint, unlike an omitted family.
    assert!(observation_matches_predicate(
        &complete_observation,
        &predicate(None, None, None)
    ));
    assert!(!observation_matches_predicate(
        &complete_observation,
        &predicate(Some(&[]), None, None)
    ));
    assert!(!observation_matches_predicate(
        &complete_observation,
        &predicate(None, Some(&[]), None)
    ));
    assert!(!observation_matches_predicate(
        &complete_observation,
        &predicate(None, None, Some(&[]))
    ));

    assert!(!observation_matches_predicate(
        &observation(&[], &[], &[]),
        &predicate(Some(&[]), Some(&[]), Some(&[]))
    ));
}

#[test]
fn present_empty_observation_value_is_distinct_from_empty_observation_family() {
    let absent_observation = observation(&[], &[], &[]);

    // An empty family has no observation value to match, even for empty or
    // NUL-only predicate values.
    assert!(!observation_matches_predicate(
        &absent_observation,
        &predicate(Some(&[""]), None, None)
    ));
    assert!(!observation_matches_predicate(
        &absent_observation,
        &predicate(None, Some(&[""]), None)
    ));
    assert!(!observation_matches_predicate(
        &absent_observation,
        &predicate(None, None, Some(&[""]))
    ));
    assert!(!observation_matches_predicate(
        &absent_observation,
        &predicate(Some(&["\0"]), None, None)
    ));

    // A present empty observation value matches an empty or NUL-only
    // predicate value.
    let present_empty_observation = observation(&[""], &[], &[]);
    assert!(observation_matches_predicate(
        &present_empty_observation,
        &predicate(Some(&[""]), None, None)
    ));
    assert!(observation_matches_predicate(
        &present_empty_observation,
        &predicate(Some(&["\0"]), None, None)
    ));

    // A NUL-only observation value matches either predicate value and is
    // not absence.
    let nul_only_observation = observation(&["\0"], &[], &[]);
    assert!(observation_matches_predicate(
        &nul_only_observation,
        &predicate(Some(&[""]), None, None)
    ));
    assert!(observation_matches_predicate(
        &nul_only_observation,
        &predicate(Some(&["\0"]), None, None)
    ));
}

#[test]
fn exact_duplicates_collapse_while_raw_distinct_spellings_remain_stored() {
    let composed = "Bj\u{00F6}rk".to_owned();
    let decomposed = "Bjo\u{0308}rk".to_owned();
    let nul_terminated = "Bj\u{00F6}rk\0".to_owned();

    // These spellings match, but are not raw-equal.
    assert!(metadata_values_match(&composed, &decomposed));
    assert!(metadata_values_match(&composed, &nul_terminated));

    let observation = MetadataSelectorObservation::new(
        [
            composed.clone(),
            composed.clone(),
            decomposed.clone(),
            nul_terminated.clone(),
        ]
        .into_iter()
        .collect(),
        ["".to_owned(), "\0".to_owned()].into_iter().collect(),
        BTreeSet::new(),
    );

    let predicate_album_names = vec![
        "Bjo\u{0308}rk\0\0".to_owned(),
        composed.clone(),
        decomposed.clone(),
        composed.clone(),
    ];
    let predicate_album_artists = vec!["\0\0".to_owned(), "".to_owned()];
    let predicate = MetadataSelectorPredicate::new(
        Some(predicate_album_names.clone()),
        Some(predicate_album_artists.clone()),
        None,
    );

    // The set removes only exact duplicates, not matching-equivalent
    // observation values.
    assert_eq!(observation.album_names.len(), 3);
    assert!(observation.album_names.contains(&composed));
    assert!(observation.album_names.contains(&decomposed));
    assert!(observation.album_names.contains(&nul_terminated));

    // Empty and NUL-only observation values remain distinct.
    assert_eq!(observation.album_artists.len(), 2);
    assert!(observation.album_artists.contains(""));
    assert!(observation.album_artists.contains("\0"));
    assert!(observation.track_artists.is_empty());
    assert_eq!(predicate.album_names, Some(predicate_album_names));
    assert_eq!(predicate.album_artists, Some(predicate_album_artists));
    assert_eq!(predicate.track_artists, None);

    // Matching-equivalent raw-distinct spellings still match.
    assert!(observation_matches_predicate(&observation, &predicate));
}

#[test]
fn family_matching_agrees_with_value_comparison() {
    let values = [
        "",
        "\0",
        "\0\0",
        "\u{00E9}",
        "e\u{0301}",
        "e\u{0301}\0\0",
        "Album",
        "Album\0",
        "Album\0\0",
        "album",
        " Album",
        "Album ",
        "Album\t",
        "\0Album",
        "A\0B",
        "AB",
        "e\0\u{0301}",
        "Artist A; Artist B",
        "Artist A;Artist B",
        "Artist A",
        "AC/DC",
        "ACDC",
        "\u{FB01}",
        "fi",
        "\u{FF21}",
        "A",
    ];

    for observed in values {
        let observation_values = to_observation_values(&[observed]);
        for alternative in values {
            let predicate_values = vec![alternative.to_owned()];
            assert_eq!(
                family_matches(&observation_values, Some(&predicate_values)),
                metadata_values_match(observed, alternative),
                "family comparison disagreed for {observed:?} and {alternative:?}"
            );
        }
    }
}

#[test]
fn predicate_value_order_and_repetition_do_not_affect_evaluation() {
    let complete_observation = observation(&["Kind of Blue"], &[], &[]);

    let forward = predicate(Some(&["Bitches Brew", "Kind of Blue"]), None, None);
    let reversed = predicate(Some(&["Kind of Blue", "Bitches Brew"]), None, None);
    let repeated = predicate(
        Some(&[
            "Bitches Brew",
            "Kind of Blue",
            "Kind of Blue",
            "Bitches Brew",
        ]),
        None,
        None,
    );

    assert!(observation_matches_predicate(
        &complete_observation,
        &forward
    ));
    assert!(observation_matches_predicate(
        &complete_observation,
        &reversed
    ));
    assert!(observation_matches_predicate(
        &complete_observation,
        &repeated
    ));

    let repeated_mismatch = predicate(Some(&["Mingus Ah Um", "Mingus Ah Um"]), None, None);
    assert!(!observation_matches_predicate(
        &complete_observation,
        &repeated_mismatch
    ));
}

#[test]
fn evaluation_uses_metadata_values_match() {
    // NFC normalization and trailing-NUL removal also apply during matching.
    let complete_observation = observation(&["Bjo\u{0308}rk\0"], &["e\u{0301}"], &[]);
    assert!(observation_matches_predicate(
        &complete_observation,
        &predicate(Some(&["Bj\u{00F6}rk"]), Some(&["\u{00E9}"]), None)
    ));

    // Case, whitespace, embedded NUL, whole-value boundaries, and literal
    // semicolons remain significant.
    let sensitive_observation = observation(&["Artist A; Artist B"], &["The Wall"], &[]);
    assert!(!observation_matches_predicate(
        &sensitive_observation,
        &predicate(Some(&["artist a; artist b"]), None, None)
    ));
    assert!(!observation_matches_predicate(
        &sensitive_observation,
        &predicate(Some(&["Artist A"]), None, None)
    ));
    assert!(!observation_matches_predicate(
        &sensitive_observation,
        &predicate(Some(&["Artist A;Artist B"]), None, None)
    ));
    assert!(!observation_matches_predicate(
        &sensitive_observation,
        &predicate(None, Some(&["The  Wall"]), None)
    ));
    assert!(!observation_matches_predicate(
        &sensitive_observation,
        &predicate(None, Some(&["The Wall "]), None)
    ));
    assert!(observation_matches_predicate(
        &sensitive_observation,
        &predicate(Some(&["Artist A; Artist B"]), Some(&["The Wall"]), None)
    ));

    let embedded_nul_observation = observation(&["A\0B"], &[], &[]);
    assert!(!observation_matches_predicate(
        &embedded_nul_observation,
        &predicate(Some(&["AB"]), None, None)
    ));
    assert!(observation_matches_predicate(
        &embedded_nul_observation,
        &predicate(Some(&["A\0B"]), None, None)
    ));
}

#[test]
fn construction_preserves_supplied_representations() {
    let album_names: BTreeSet<String> = ["Kind of Blue".to_owned(), "Bitches Brew".to_owned()]
        .into_iter()
        .collect();
    let album_artists: BTreeSet<String> = ["Miles Davis".to_owned()].into_iter().collect();
    let track_artists: BTreeSet<String> = BTreeSet::new();
    // Decomposed Unicode and trailing NULs would change under forbidden
    // NFC normalization or trailing-NUL removal.
    let supplied_predicate_values = vec![
        "Bjo\u{0308}rk".to_owned(),
        "Track\0\0".to_owned(),
        "Bitches Brew".to_owned(),
        "Kind of Blue".to_owned(),
        "Kind of Blue".to_owned(),
    ];

    let observation = MetadataSelectorObservation::new(
        album_names.clone(),
        album_artists.clone(),
        track_artists.clone(),
    );
    let predicate = MetadataSelectorPredicate::new(
        Some(supplied_predicate_values.clone()),
        None,
        Some(Vec::new()),
    );

    // Construction retains the supplied observation sets.
    assert_eq!(observation.album_names, album_names);
    assert_eq!(observation.album_artists, album_artists);
    assert_eq!(observation.track_artists, track_artists);

    // Construction retains raw predicate values, their order and
    // repetitions, and the distinction between omitted and supplied
    // families.
    assert_eq!(predicate.album_names, Some(supplied_predicate_values));
    assert_eq!(predicate.album_artists, None);
    assert_eq!(predicate.track_artists, Some(Vec::new()));
}

fn parsed_declaration(body: &str) -> crate::config::CollectionDeclaration {
    let text = format!("codec = \"opus\"\nbitrate = 128\n[collections.item]\n{body}");
    let spec = crate::parse(&text).expect("test configuration must parse and validate");
    spec.collection_declaration("item")
        .expect("item must be declared")
        .clone()
}

#[test]
fn declaration_translation_preserves_omission_as_unconstrained() {
    let declaration = parsed_declaration("directory = \"Album\"\n");
    let predicate = declaration.metadata_selector_predicate();

    assert_eq!(predicate.album_names, None);
    assert_eq!(predicate.album_artists, None);
    assert_eq!(predicate.track_artists, None);

    let probe = observation(&["Kind of Blue"], &["Miles Davis"], &["Coltrane"]);
    assert!(observation_matches_predicate(&probe, &predicate));
    let empty = observation(&[], &[], &[]);
    assert!(observation_matches_predicate(&empty, &predicate));
}

#[test]
fn declaration_translation_maps_each_family_independently() {
    let names_only = parsed_declaration("album_name = \"Kind of Blue\"\n");
    let predicate = names_only.metadata_selector_predicate();
    assert_eq!(predicate.album_names, Some(vec!["Kind of Blue".to_owned()]));
    assert_eq!(predicate.album_artists, None);
    assert_eq!(predicate.track_artists, None);

    let artists_only = parsed_declaration("album_artists = [\"Miles Davis\", \"Coltrane\"]\n");
    let predicate = artists_only.metadata_selector_predicate();
    assert_eq!(predicate.album_names, None);
    assert_eq!(
        predicate.album_artists,
        Some(vec!["Miles Davis".to_owned(), "Coltrane".to_owned()])
    );
    assert_eq!(predicate.track_artists, None);

    let track_only = parsed_declaration("track_artist = \"Coltrane\"\n");
    let predicate = track_only.metadata_selector_predicate();
    assert_eq!(predicate.album_names, None);
    assert_eq!(predicate.album_artists, None);
    assert_eq!(predicate.track_artists, Some(vec!["Coltrane".to_owned()]));

    let combined = parsed_declaration(
        "album_name = \"Kind of Blue\"\nalbum_artist = \"Miles Davis\"\ntrack_artists = [\"Coltrane\", \"Evans\"]\n",
    );
    let predicate = combined.metadata_selector_predicate();
    assert_eq!(predicate.album_names, Some(vec!["Kind of Blue".to_owned()]));
    assert_eq!(
        predicate.album_artists,
        Some(vec!["Miles Davis".to_owned()])
    );
    assert_eq!(
        predicate.track_artists,
        Some(vec!["Coltrane".to_owned(), "Evans".to_owned()])
    );
}

#[test]
fn declaration_translation_preserves_order_repetition_and_empty_strings() {
    let declaration = parsed_declaration(
        "album_names = [\"\", \"B\", \"A\", \"B\", \"   \"]\nalbum_artists = [\"X\", \"X\"]\ntrack_artist = \"\"\n",
    );
    let predicate = declaration.metadata_selector_predicate();

    assert_eq!(
        predicate.album_names,
        Some(vec![
            "".to_owned(),
            "B".to_owned(),
            "A".to_owned(),
            "B".to_owned(),
            "   ".to_owned(),
        ])
    );
    assert_eq!(
        predicate.album_artists,
        Some(vec!["X".to_owned(), "X".to_owned()])
    );
    assert_eq!(predicate.track_artists, Some(vec!["".to_owned()]));
}

#[test]
fn declaration_translation_preserves_matching_sensitive_spellings() {
    // Translation must preserve decomposed Unicode and trailing NULs.
    // The test covers all three selector families.
    let declaration = parsed_declaration(
        "album_names = [\"Bjo\\u0308rk\", \"Track\\u0000\\u0000\"]\nalbum_artists = [\"e\\u0301\"]\ntrack_artists = [\"\\u1100\\u1161\\u0000\"]\n",
    );
    let predicate = declaration.metadata_selector_predicate();

    assert_eq!(
        predicate.album_names,
        Some(vec!["Bjo\u{0308}rk".to_owned(), "Track\0\0".to_owned()])
    );
    assert_eq!(predicate.album_artists, Some(vec!["e\u{0301}".to_owned()]));
    assert_eq!(
        predicate.track_artists,
        Some(vec!["\u{1100}\u{1161}\0".to_owned()])
    );
    assert_ne!(
        predicate.album_names,
        Some(vec!["Bj\u{00F6}rk".to_owned(), "Track".to_owned()]),
        "translation must not normalize or trim"
    );
}

#[test]
fn declaration_translation_keeps_detached_empty_lists_supplied() {
    let families: [fn(crate::config::CollectionDeclaration) -> bool; 3] = [
        |declaration| declaration.metadata_selector_predicate().album_names == Some(Vec::new()),
        |declaration| declaration.metadata_selector_predicate().album_artists == Some(Vec::new()),
        |declaration| declaration.metadata_selector_predicate().track_artists == Some(Vec::new()),
    ];
    let empty_vectors = [
        crate::config::CollectionDeclaration {
            album_names: Some(Vec::new()),
            album_artists: None,
            track_artists: None,
            directories: None,
        },
        crate::config::CollectionDeclaration {
            album_names: None,
            album_artists: Some(Vec::new()),
            track_artists: None,
            directories: None,
        },
        crate::config::CollectionDeclaration {
            album_names: None,
            album_artists: None,
            track_artists: Some(Vec::new()),
            directories: None,
        },
    ];
    for (declaration, check) in empty_vectors.into_iter().zip(families) {
        assert!(
            check(declaration),
            "detached empty list must stay supplied-empty"
        );
    }

    let probe = observation(&["Kind of Blue"], &["Miles Davis"], &["Coltrane"]);
    for declaration in [
        crate::config::CollectionDeclaration {
            album_names: Some(Vec::new()),
            album_artists: None,
            track_artists: None,
            directories: None,
        },
        crate::config::CollectionDeclaration {
            album_names: None,
            album_artists: Some(Vec::new()),
            track_artists: None,
            directories: None,
        },
        crate::config::CollectionDeclaration {
            album_names: None,
            album_artists: None,
            track_artists: Some(Vec::new()),
            directories: None,
        },
    ] {
        let predicate = declaration.metadata_selector_predicate();
        assert!(
            !observation_matches_predicate(&probe, &predicate),
            "supplied-empty family must stay unsatisfiable"
        );
    }
}

#[test]
fn declaration_translation_ignores_directories() {
    let without_directories = parsed_declaration("album_name = \"Kind of Blue\"\n");
    let with_directories = parsed_declaration(
        "album_name = \"Kind of Blue\"\ndirectories = [\"Disc 1\", \"Disc 2\"]\n",
    );
    let singular_directory =
        parsed_declaration("album_name = \"Kind of Blue\"\ndirectory = \"Disc 1\"\n");

    let baseline = without_directories.metadata_selector_predicate();
    for declaration in [&with_directories, &singular_directory] {
        let predicate = declaration.metadata_selector_predicate();
        assert_eq!(predicate.album_names, baseline.album_names);
        assert_eq!(predicate.album_artists, baseline.album_artists);
        assert_eq!(predicate.track_artists, baseline.track_artists);
    }
}
