/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Directory-only membership for configured collections from a completed
//! candidate inventory.
//!
//! An album declaration defines a user-configured collection. Each source-audio
//! candidate reported by one of the collection's configured directory scopes is
//! a member.
//!
//! Multiple scopes form a union. A candidate contributes at most once per
//! collection but may belong to several collections. Membership preserves
//! inventory pathname spelling and identity, and does not probe files,
//! inspect metadata, access the filesystem, or decide output suitability.
//!
//! Declarations with metadata selectors remain unevaluated. They are
//! reported separately, not as collections with empty membership.

// This module is used only by its tests. Keep it private until the build
// pipeline uses it.
#![allow(dead_code)]

use std::path::PathBuf;

use crate::candidate_inventory::{Candidate, CandidateInventory, RequiredScope};
use crate::config::{Album, LibraryBuildSpec};
use crate::source_audio::classify_source_audio;

/// Directory-only membership results for one validated build specification.
#[derive(Debug)]
pub(crate) struct CollectionMembership {
    collections: Vec<CollectionMembers>,
    unevaluated_album_handles: Vec<String>,
}

/// Member candidates for one evaluated collection.
#[derive(Debug)]
pub(crate) struct CollectionMembers {
    album_handle: String,
    members: Vec<PathBuf>,
}

impl CollectionMembership {
    /// Evaluated collections in declaration order.
    pub(crate) fn collections(&self) -> &[CollectionMembers] {
        &self.collections
    }

    /// Declarations with metadata predicates left unevaluated.
    ///
    /// These declarations have no established membership, unlike an evaluated
    /// collection with no members.
    pub(crate) fn unevaluated_album_handles(&self) -> &[String] {
        &self.unevaluated_album_handles
    }
}

impl CollectionMembers {
    /// The configuration handle of this collection.
    pub(crate) fn album_handle(&self) -> &str {
        &self.album_handle
    }

    /// The collection's member candidates, in no specified order.
    pub(crate) fn members(&self) -> &[PathBuf] {
        &self.members
    }
}

/// Builds directory-only membership for `spec` using a completed `inventory`.
///
/// Declarations with a supplied `name` or `artist` selector are reported as
/// unevaluated without checking their directory associations. Other declarations
/// are evaluated even when their scopes contain no eligible candidates.
///
/// A candidate contributes when one of the declaration's configured
/// directory scopes reported it and [`classify_source_audio`] recognizes its
/// inventory pathname. Multiple matching scopes contribute it only once.
/// Inventory pathnames are copied unchanged, and distinct candidates remain
/// distinct.
pub(crate) fn directory_only_membership(
    spec: &LibraryBuildSpec,
    inventory: &CandidateInventory,
) -> CollectionMembership {
    let mut collections = Vec::new();
    let mut unevaluated_album_handles = Vec::new();

    for (album_handle, album) in spec.albums() {
        if !directory_only_eligible(album) {
            unevaluated_album_handles.push(album_handle.to_owned());
            continue;
        }

        let members: Vec<PathBuf> = inventory
            .candidates()
            .filter(|candidate| classify_source_audio(candidate.path()).is_some())
            .filter(|candidate| candidate_belongs_to(candidate, album_handle))
            .map(|candidate| candidate.path().to_path_buf())
            .collect();

        collections.push(CollectionMembers {
            album_handle: album_handle.to_owned(),
            members,
        });
    }

    CollectionMembership {
        collections,
        unevaluated_album_handles,
    }
}

/// Whether directory-only membership may evaluate `album`.
fn directory_only_eligible(album: &Album) -> bool {
    album.name.is_none() && album.artist.is_none()
}

/// Whether a configured directory scope for `album_handle` reported `candidate`.
///
/// A default source-root scope may still be required to build the inventory for
/// other declarations. It never establishes directory-only membership.
fn candidate_belongs_to(candidate: &Candidate<'_>, album_handle: &str) -> bool {
    candidate.scopes().any(|scope| match scope {
        RequiredScope::ConfiguredDirectory {
            album_handle: scope_album_handle,
            ..
        } => scope_album_handle == album_handle,
        RequiredScope::DefaultSourceRoot { .. } => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate_inventory::build_candidate_inventory;
    use crate::test_support::{
        SourceTreeSnapshot, TempSandbox, canonical, create_source, parse_spec,
    };
    use std::fs;
    use std::path::Path;

    /// Builds a completed inventory and derives its directory-only membership.
    fn membership_for(spec: &LibraryBuildSpec, source: &Path) -> CollectionMembership {
        let success = build_candidate_inventory(spec, source).expect("inventory must succeed");
        directory_only_membership(spec, success.inventory())
    }

    /// Returns the sorted members of `album_handle`, which must be evaluated.
    ///
    /// Sorting keeps assertions independent of unspecified member order.
    fn sorted_members(membership: &CollectionMembership, album_handle: &str) -> Vec<PathBuf> {
        let collection = membership
            .collections()
            .iter()
            .find(|collection| collection.album_handle() == album_handle)
            .unwrap_or_else(|| panic!("collection {album_handle} must be evaluated"));
        let mut members = collection.members().to_vec();
        members.sort();
        members
    }

    #[test]
    fn every_source_audio_candidate_contributes_without_inspecting_contents() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let album = source.join("Fear of a Blank Planet");
        fs::create_dir_all(album.join("bonus")).expect("create album directories");
        // The contents are not valid media, so contribution must not probe them.
        fs::write(album.join("01 - Anesthetize.flac"), b"not a media file").expect("write track");
        fs::write(album.join("bonus/02 - My Ashes.FLAC"), b"").expect("write empty track");
        let spec =
            parse_spec("[albums.fear-of-a-blank-planet]\ndirectory = \"Fear of a Blank Planet\"\n");

        let snapshot = SourceTreeSnapshot::capture(&source);
        let result = build_candidate_inventory(&spec, &source);
        snapshot.assert_unchanged();
        let success = result.expect("inventory must succeed");

        let membership = directory_only_membership(&spec, success.inventory());

        assert!(membership.unevaluated_album_handles().is_empty());
        let mut expected = vec![
            canonical(&album).join("01 - Anesthetize.flac"),
            canonical(&album).join("bonus/02 - My Ashes.FLAC"),
        ];
        expected.sort();
        assert_eq!(
            sorted_members(&membership, "fear-of-a-blank-planet"),
            expected
        );
    }

    #[test]
    fn unrecognized_candidates_do_not_contribute() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let album = source.join("Album");
        fs::create_dir(&album).expect("create album");
        fs::write(album.join("real.flac"), b"audio").expect("write track");
        for name in [
            "notes.txt",
            "cover.jpg",
            "song.mp3",
            "take.flac.bak",
            "no-extension",
        ] {
            fs::write(album.join(name), b"other").expect("write non-audio file");
        }
        let spec = parse_spec("[albums.tool]\ndirectory = \"Album\"\n");

        let membership = membership_for(&spec, &source);

        assert_eq!(
            sorted_members(&membership, "tool"),
            vec![canonical(&album).join("real.flac")]
        );
    }

    #[test]
    fn multiple_scopes_contribute_a_union_without_duplicate_membership() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let one = source.join("disc one");
        let nested = one.join("nested");
        let two = source.join("disc two");
        fs::create_dir_all(&nested).expect("create nested directory");
        fs::create_dir(&two).expect("create second directory");
        fs::write(one.join("01.flac"), b"one").expect("write first track");
        fs::write(nested.join("02.flac"), b"nested").expect("write nested track");
        fs::write(two.join("03.flac"), b"two").expect("write second track");
        // The nested candidate is reported by both the parent and nested scopes.
        let spec = parse_spec(
            "[albums.collection]\ndirectories = [\"disc one\", \"disc two\", \"disc one/nested\"]\n",
        );

        let membership = membership_for(&spec, &source);

        let mut expected = vec![
            canonical(&one).join("01.flac"),
            canonical(&nested).join("02.flac"),
            canonical(&two).join("03.flac"),
        ];
        expected.sort();
        assert_eq!(sorted_members(&membership, "collection"), expected);
    }

    #[test]
    fn overlapping_collections_share_members_regardless_of_declaration_order() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let shared = source.join("shared");
        let alpha_only = source.join("alpha-only");
        fs::create_dir(&shared).expect("create shared directory");
        fs::create_dir(&alpha_only).expect("create alpha-only directory");
        fs::write(shared.join("shared.flac"), b"shared").expect("write shared track");
        fs::write(alpha_only.join("alpha.flac"), b"alpha").expect("write alpha track");

        for declarations in [
            "[albums.alpha]\ndirectories = [\"shared\", \"alpha-only\"]\n\
             [albums.beta]\ndirectory = \"shared\"\n",
            "[albums.beta]\ndirectory = \"shared\"\n\
             [albums.alpha]\ndirectories = [\"shared\", \"alpha-only\"]\n",
        ] {
            let spec = parse_spec(declarations);
            let membership = membership_for(&spec, &source);

            assert!(
                membership.unevaluated_album_handles().is_empty(),
                "declarations: {declarations}"
            );
            let mut alpha_members = vec![
                canonical(&shared).join("shared.flac"),
                canonical(&alpha_only).join("alpha.flac"),
            ];
            alpha_members.sort();
            assert_eq!(
                sorted_members(&membership, "alpha"),
                alpha_members,
                "declarations: {declarations}"
            );
            assert_eq!(
                sorted_members(&membership, "beta"),
                vec![canonical(&shared).join("shared.flac")],
                "declarations: {declarations}"
            );
        }
    }

    #[test]
    fn covered_scopes_without_eligible_candidates_establish_empty_membership() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let empty = source.join("empty");
        let non_audio = source.join("non-audio");
        fs::create_dir(&empty).expect("create empty directory");
        fs::create_dir(&non_audio).expect("create non-audio directory");
        fs::write(non_audio.join("notes.txt"), b"notes").expect("write notes");
        let spec = parse_spec(
            "[albums.empty]\ndirectory = \"empty\"\n\
             [albums.non-audio]\ndirectory = \"non-audio\"\n",
        );

        let membership = membership_for(&spec, &source);

        assert!(membership.unevaluated_album_handles().is_empty());
        assert_eq!(membership.collections().len(), 2);
        assert_eq!(sorted_members(&membership, "empty"), Vec::<PathBuf>::new());
        assert_eq!(
            sorted_members(&membership, "non-audio"),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn metadata_selector_declarations_stay_unevaluated() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        for name in [
            "pure",
            "with-name",
            "with-artist",
            "empty-name",
            "empty-artist",
        ] {
            let directory = source.join(name);
            fs::create_dir(&directory).expect("create collection directory");
            fs::write(directory.join("track.flac"), b"audio").expect("write track");
        }
        let spec = parse_spec(
            "[albums.pure]\ndirectory = \"pure\"\n\
             [albums.with-name]\nname = \"Named\"\ndirectory = \"with-name\"\n\
             [albums.with-artist]\nartist = \"Artist\"\ndirectory = \"with-artist\"\n\
             [albums.empty-name]\nname = \"\"\ndirectory = \"empty-name\"\n\
             [albums.empty-artist]\nartist = \"\"\ndirectory = \"empty-artist\"\n",
        );

        let membership = membership_for(&spec, &source);

        let expected_unevaluated: Vec<String> =
            ["with-name", "with-artist", "empty-name", "empty-artist"]
                .map(str::to_owned)
                .to_vec();
        assert_eq!(membership.unevaluated_album_handles(), expected_unevaluated);
        assert_eq!(membership.collections().len(), 1);
        assert_eq!(membership.collections()[0].album_handle(), "pure");

        // Candidates found only through metadata-selector scopes are excluded from this collection.
        let pure_members = sorted_members(&membership, "pure");
        assert_eq!(
            pure_members,
            vec![canonical(&source.join("pure")).join("track.flac")]
        );
        for handle in ["with-name", "with-artist", "empty-name", "empty-artist"] {
            let selector_prefix = canonical(&source.join(handle));
            assert!(
                pure_members
                    .iter()
                    .all(|member| !member.starts_with(&selector_prefix)),
                "candidate under {handle} must not join an evaluated collection"
            );
        }
    }

    #[test]
    fn default_scope_dependents_stay_unevaluated() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let collection = source.join("collection");
        fs::create_dir(&collection).expect("create collection directory");
        fs::write(collection.join("configured.flac"), b"configured")
            .expect("write configured track");
        fs::write(source.join("root-only.flac"), b"root").expect("write default-root track");
        let spec = parse_spec(
            "[albums.configured]\ndirectory = \"collection\"\n\
             [albums.default-name]\nname = \"Named\"\n\
             [albums.default-artist]\nartist = \"\"\n",
        );

        let membership = membership_for(&spec, &source);

        let expected_unevaluated: Vec<String> = ["default-name", "default-artist"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(membership.unevaluated_album_handles(), expected_unevaluated);
        assert_eq!(membership.collections().len(), 1);
        assert_eq!(membership.collections()[0].album_handle(), "configured");
        // The shared default-root scope does not contribute to any evaluated
        // collection. Only the configured directory scope contributes.
        assert_eq!(
            sorted_members(&membership, "configured"),
            vec![canonical(&collection).join("configured.flac")]
        );
    }

    #[test]
    fn inventory_pathnames_stay_distinct_and_keep_their_spelling() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let album = source.join("Album");
        fs::create_dir_all(album.join("disc 1")).expect("create disc 1");
        fs::create_dir_all(album.join("disc 2")).expect("create disc 2");
        fs::write(album.join("disc 1/01 - Song (Live).flac"), b"one").expect("write disc 1 track");
        fs::write(album.join("disc 2/01 - Song (Live).flac"), b"two").expect("write disc 2 track");
        fs::write(album.join("Björk - Homogenic.flac"), b"unicode").expect("write unicode track");
        let spec = parse_spec("[albums.tool]\ndirectory = \"Album\"\n");

        let membership = membership_for(&spec, &source);

        let mut expected = vec![
            canonical(&album).join("disc 1/01 - Song (Live).flac"),
            canonical(&album).join("disc 2/01 - Song (Live).flac"),
            canonical(&album).join("Björk - Homogenic.flac"),
        ];
        expected.sort();
        assert_eq!(sorted_members(&membership, "tool"), expected);
    }

    #[test]
    fn membership_construction_does_not_mutate_the_source_tree() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let album = source.join("Album");
        fs::create_dir(&album).expect("create album");
        fs::write(album.join("track.flac"), b"track").expect("write track");
        fs::write(album.join("notes.txt"), b"notes").expect("write notes");
        let spec = parse_spec(
            "[albums.audio]\ndirectory = \"Album\"\n\
             [albums.with-name]\nname = \"Named\"\ndirectory = \"Album\"\n",
        );

        let snapshot = SourceTreeSnapshot::capture(&source);
        let result = build_candidate_inventory(&spec, &source);
        snapshot.assert_unchanged();
        let success = result.expect("inventory must succeed");

        let membership = directory_only_membership(&spec, success.inventory());
        snapshot.assert_unchanged();

        assert_eq!(
            sorted_members(&membership, "audio"),
            vec![canonical(&album).join("track.flac")]
        );
    }
}
