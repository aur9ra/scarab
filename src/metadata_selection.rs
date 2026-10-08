/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Reader-independent metadata-selector matching.
//!
//! Callers provide an observation of all three metadata families and an
//! optional list of predicate values per family. This module compares them
//! without accessing configuration, metadata readers, filesystem state,
//! collection membership, or output policy.
//!
//! Predicate values combine by OR within each supplied family, and supplied
//! families combine by AND. An omitted family imposes no constraint, and a
//! supplied empty list never matches. Observation values in one family cannot
//! satisfy predicate values in another family.

// No non-test production entry point reaches the metadata-selection path yet.
// Its consumers in configuration, Vorbis projection, and collection membership
// are unreachable from production code, and `metadata_values_match` has no
// caller outside this module's tests. Remove this allowance when a production
// entry point uses the path.
#![allow(dead_code)]

use std::collections::{BTreeSet, HashSet};

use unicode_normalization::UnicodeNormalization;

fn comparison_chars(value: &str) -> impl Iterator<Item = char> + '_ {
    value.trim_end_matches('\0').nfc()
}

/// Reports whether two supplied metadata values match under Scarab's
/// metadata-selector comparison rule.
///
/// It removes all trailing U+0000s from each value, NFC-normalizes both results,
/// and compares them in full with case-sensitive equality. The comparison is
/// symmetric and requires a whole-value match. Whitespace, punctuation
/// (including semicolons), embedded and leading U+0000, and compatibility
/// differences remain significant. Neither input is modified.
pub(crate) fn metadata_values_match(left: &str, right: &str) -> bool {
    comparison_chars(left).eq(comparison_chars(right))
}

/// Represents a complete, successful observation of album names, album
/// artists, and track artists.
///
/// The caller must supply complete results. This type cannot detect failed or
/// incomplete extraction. Each family stores raw observation values under
/// exact string equality: exact duplicates collapse, while observation values
/// equivalent under [`metadata_values_match`] stay distinct. An empty set
/// means no observation values are stored; an empty or NUL-only observation
/// value, if present, is a member of the set. [`BTreeSet`] iteration order is
/// an implementation detail, not a domain ordering contract.
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub(crate) struct MetadataSelectorObservation {
    album_names: BTreeSet<String>,
    album_artists: BTreeSet<String>,
    track_artists: BTreeSet<String>,
}

impl MetadataSelectorObservation {
    /// Creates an observation from complete, successful results for all
    /// families.
    ///
    /// The caller must not pass failed or incomplete extraction. The sets of
    /// observation values and their raw strings are retained. Occurrence
    /// counts and insertion order are already lost when the sets are formed.
    pub(crate) fn new(
        album_names: BTreeSet<String>,
        album_artists: BTreeSet<String>,
        track_artists: BTreeSet<String>,
    ) -> Self {
        Self {
            album_names,
            album_artists,
            track_artists,
        }
    }
}

/// A complete compound predicate with an optional constraint for each of the
/// three metadata families.
///
/// `None` imposes no constraint. `Some` matches when any predicate value
/// matches an observation value in that family. Supplied families must all
/// match, and `Some([])` is unsatisfiable. Predicate-value order and repetition
/// are retained but do not affect evaluation.
pub(crate) struct MetadataSelectorPredicate {
    album_names: Option<Vec<String>>,
    album_artists: Option<Vec<String>>,
    track_artists: Option<Vec<String>>,
}

impl MetadataSelectorPredicate {
    /// Creates a predicate while retaining each supplied representation.
    ///
    /// Predicate values, their order and repetitions, and the distinction
    /// between `None` and `Some([])` are preserved. No validation or
    /// normalization is done.
    pub(crate) fn new(
        album_names: Option<Vec<String>>,
        album_artists: Option<Vec<String>>,
        track_artists: Option<Vec<String>>,
    ) -> Self {
        Self {
            album_names,
            album_artists,
            track_artists,
        }
    }
}

/// Creates an evaluation key under [`metadata_values_match`].
fn comparison_key(value: &str) -> String {
    comparison_chars(value).collect()
}

/// Returns whether any predicate value matches an observation value in this
/// family. `None` imposes no constraint.
fn family_matches(
    observation_values: &BTreeSet<String>,
    predicate_values: Option<&[String]>,
) -> bool {
    let Some(predicate_values) = predicate_values else {
        return true;
    };
    if predicate_values.is_empty() || observation_values.is_empty() {
        return false;
    }

    // Normalize each observation once without changing stored-value identity.
    let observation_keys: HashSet<String> = observation_values
        .iter()
        .map(|value| comparison_key(value))
        .collect();
    predicate_values
        .iter()
        .any(|value| observation_keys.contains(&comparison_key(value)))
}

/// Returns whether the compound predicate matches the complete observation.
///
/// Each supplied family matches when any predicate value matches any
/// observation value under [`metadata_values_match`]. Omitted families impose
/// no constraint. Neither argument is modified.
pub(crate) fn observation_matches_predicate(
    observation: &MetadataSelectorObservation,
    predicate: &MetadataSelectorPredicate,
) -> bool {
    let album_names_match: bool =
        family_matches(&observation.album_names, predicate.album_names.as_deref());
    let album_artists_match: bool = family_matches(
        &observation.album_artists,
        predicate.album_artists.as_deref(),
    );
    let track_artists_match: bool = family_matches(
        &observation.track_artists,
        predicate.track_artists.as_deref(),
    );

    album_names_match && album_artists_match && track_artists_match
}

#[cfg(test)]
#[path = "tests/metadata_selection.rs"]
mod tests;
