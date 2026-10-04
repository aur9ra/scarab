/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Reader-independent comparison of supplied metadata values.
//!
//! This module compares only supplied values. An absent metadata field is not
//! equivalent to a supplied empty value.

use unicode_normalization::UnicodeNormalization;

/// Reports whether two supplied metadata values match under Scarab's
/// metadata-selector comparison rule.
///
/// It removes all trailing U+0000s from each value, NFC-normalizes both results,
/// and compares them in full with case-sensitive equality. The comparison is
/// symmetric and requires a whole-value match. Whitespace, punctuation
/// (including semicolons), embedded and leading U+0000, and compatibility
/// differences remain significant. Neither input is modified.
// No production consumer exists yet. This comparison is staged for later
// metadata-selection consumers.
#[allow(dead_code)]
pub(crate) fn metadata_values_match(left: &str, right: &str) -> bool {
    left.trim_end_matches('\0')
        .nfc()
        .eq(right.trim_end_matches('\0').nfc())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
