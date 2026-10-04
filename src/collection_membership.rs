/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Directory-only membership for configured collections from a completed
//! observed source file inventory.
//!
//! A collection declaration supplies membership criteria. For a directory-only
//! declaration, an observed source file is a member when it is classified as
//! source audio and one of the declaration's configured directory scopes reports it.
//!
//! An observed source file appears at most once in each collection but may
//! belong to several collections. Membership copies its retained observed
//! pathname unchanged and does not probe files, inspect metadata, access the
//! filesystem, or decide output suitability.
//!
//! Declarations with metadata selectors remain unevaluated. They are
//! reported separately, not as collections with empty membership.

// This module is used only by its tests. Keep it private until the build
// pipeline uses it.
#![allow(dead_code)]

use std::path::PathBuf;

use crate::config::{CollectionDeclaration, LibraryBuildSpec};
use crate::observed_source_file_inventory::{
    ObservedSourceFile, ObservedSourceFileInventory, RequiredScope,
};
use crate::source_audio::classify_source_audio;

/// Directory-only membership results for one validated build specification.
#[derive(Debug)]
pub(crate) struct CollectionMembership {
    collections: Vec<CollectionMembers>,
    unevaluated_collection_handles: Vec<String>,
}

/// Established directory-only membership for one evaluated collection.
#[derive(Debug)]
pub(crate) struct CollectionMembers {
    collection_handle: String,
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
    pub(crate) fn unevaluated_collection_handles(&self) -> &[String] {
        &self.unevaluated_collection_handles
    }
}

impl CollectionMembers {
    /// The configuration handle of this collection.
    pub(crate) fn collection_handle(&self) -> &str {
        &self.collection_handle
    }

    /// Pathnames of the collection's members, in no specified order.
    pub(crate) fn members(&self) -> &[PathBuf] {
        &self.members
    }
}

/// Builds directory-only membership for `spec` using a completed `inventory`.
///
/// Declarations with a supplied `name` or `artist` metadata selector are reported as
/// unevaluated without checking their directory associations. Other declarations
/// are evaluated even when none of the files reported by their configured
/// directory scopes are recognized as source audio.
///
/// For an evaluated collection, an observed source file is a member when
/// [`classify_source_audio`] recognizes its pathname and one of the collection's
/// configured directory scopes reported it. The file appears only once in that
/// collection even if multiple of its configured directory scopes reported it.
/// Its retained observed pathname is copied unchanged into the membership result.
/// Distinct observed source files remain distinct.
pub(crate) fn directory_only_membership(
    spec: &LibraryBuildSpec,
    inventory: &ObservedSourceFileInventory,
) -> CollectionMembership {
    let mut collections = Vec::new();
    let mut unevaluated_collection_handles = Vec::new();

    for (collection_handle, declaration) in spec.collection_declarations() {
        if !directory_only_eligible(declaration) {
            unevaluated_collection_handles.push(collection_handle.to_owned());
            continue;
        }

        let members: Vec<PathBuf> = inventory
            .files()
            .filter(|observed_file| classify_source_audio(observed_file.path()).is_some())
            .filter(|observed_file| {
                file_reported_by_configured_scope(observed_file, collection_handle)
            })
            .map(|observed_file| observed_file.path().to_path_buf())
            .collect();

        collections.push(CollectionMembers {
            collection_handle: collection_handle.to_owned(),
            members,
        });
    }

    CollectionMembership {
        collections,
        unevaluated_collection_handles,
    }
}

/// Whether directory-only membership may evaluate `declaration`.
fn directory_only_eligible(declaration: &CollectionDeclaration) -> bool {
    declaration.name.is_none() && declaration.artist.is_none()
}

/// Whether a configured directory scope for `collection_handle` reported `file`.
///
/// Reporting by the default source-root scope does not satisfy this check.
/// A matching configured scope alone does not establish membership, which also
/// requires source-audio recognition.
fn file_reported_by_configured_scope(
    file: &ObservedSourceFile<'_>,
    collection_handle: &str,
) -> bool {
    file.reporting_scopes().any(|scope| match scope {
        RequiredScope::ConfiguredDirectory {
            collection_handle: scope_collection_handle,
            ..
        } => scope_collection_handle == collection_handle,
        RequiredScope::DefaultSourceRoot { .. } => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observed_source_file_inventory::build_observed_source_file_inventory;
    use crate::test_support::{
        SourceTreeSnapshot, TempSandbox, canonical, create_source, parse_spec,
    };
    use std::fs;
    use std::path::Path;

    /// Builds a completed observed source file inventory and derives membership.
    fn membership_for(spec: &LibraryBuildSpec, source: &Path) -> CollectionMembership {
        let success =
            build_observed_source_file_inventory(spec, source).expect("inventory must succeed");
        directory_only_membership(spec, success.inventory())
    }

    /// Returns the sorted members of `collection_handle`, which must be evaluated.
    ///
    /// Sorting keeps assertions independent of unspecified member order.
    fn sorted_members(membership: &CollectionMembership, collection_handle: &str) -> Vec<PathBuf> {
        let collection = membership
            .collections()
            .iter()
            .find(|collection| collection.collection_handle() == collection_handle)
            .unwrap_or_else(|| panic!("collection {collection_handle} must be evaluated"));
        let mut members = collection.members().to_vec();
        members.sort();
        members
    }

    #[test]
    fn directory_only_membership_does_not_inspect_source_audio_contents() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let album = source.join("Fear of a Blank Planet");
        fs::create_dir_all(album.join("bonus")).expect("create album directories");
        // the contents are not valid media, but directory-only membership does not probe them
        fs::write(album.join("01 - Anesthetize.flac"), b"not a media file").expect("write track");
        fs::write(album.join("bonus/02 - My Ashes.FLAC"), b"").expect("write empty track");
        let spec = parse_spec(
            "[collections.fear-of-a-blank-planet]\ndirectory = \"Fear of a Blank Planet\"\n",
        );

        let snapshot = SourceTreeSnapshot::capture(&source);
        let result = build_observed_source_file_inventory(&spec, &source);
        snapshot.assert_unchanged();
        let success = result.expect("inventory must succeed");

        let membership = directory_only_membership(&spec, success.inventory());

        assert!(membership.unevaluated_collection_handles().is_empty());
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
    fn files_not_recognized_as_source_audio_are_not_members() {
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
        let spec = parse_spec("[collections.tool]\ndirectory = \"Album\"\n");

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
        // the nested file is reported by both the parent and nested scopes
        let spec = parse_spec(
            "[collections.collection]\ndirectories = [\"disc one\", \"disc two\", \"disc one/nested\"]\n",
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
            "[collections.alpha]\ndirectories = [\"shared\", \"alpha-only\"]\n\
             [collections.beta]\ndirectory = \"shared\"\n",
            "[collections.beta]\ndirectory = \"shared\"\n\
             [collections.alpha]\ndirectories = [\"shared\", \"alpha-only\"]\n",
        ] {
            let spec = parse_spec(declarations);
            let membership = membership_for(&spec, &source);

            assert!(
                membership.unevaluated_collection_handles().is_empty(),
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
    fn covered_directory_scopes_without_source_audio_yield_empty_membership() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let empty = source.join("empty");
        let non_audio = source.join("non-audio");
        fs::create_dir(&empty).expect("create empty directory");
        fs::create_dir(&non_audio).expect("create non-audio directory");
        fs::write(non_audio.join("notes.txt"), b"notes").expect("write notes");
        let spec = parse_spec(
            "[collections.empty]\ndirectory = \"empty\"\n\
             [collections.non-audio]\ndirectory = \"non-audio\"\n",
        );

        let membership = membership_for(&spec, &source);

        assert!(membership.unevaluated_collection_handles().is_empty());
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
            "[collections.pure]\ndirectory = \"pure\"\n\
             [collections.with-name]\nname = \"Named\"\ndirectory = \"with-name\"\n\
             [collections.with-artist]\nartist = \"Artist\"\ndirectory = \"with-artist\"\n\
             [collections.empty-name]\nname = \"\"\ndirectory = \"empty-name\"\n\
             [collections.empty-artist]\nartist = \"\"\ndirectory = \"empty-artist\"\n",
        );

        let membership = membership_for(&spec, &source);

        let expected_unevaluated: Vec<String> =
            ["with-name", "with-artist", "empty-name", "empty-artist"]
                .map(str::to_owned)
                .to_vec();
        assert_eq!(
            membership.unevaluated_collection_handles(),
            expected_unevaluated
        );
        assert_eq!(membership.collections().len(), 1);
        assert_eq!(membership.collections()[0].collection_handle(), "pure");

        // these files are reported only by other declarations' configured directory
        // scopes, so they are not members of `pure`
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
                "file reported only by {handle}'s configured scope must not be a member of pure"
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
            "[collections.configured]\ndirectory = \"collection\"\n\
             [collections.default-name]\nname = \"Named\"\n\
             [collections.default-artist]\nartist = \"\"\n",
        );

        let membership = membership_for(&spec, &source);

        let expected_unevaluated: Vec<String> = ["default-name", "default-artist"]
            .map(str::to_owned)
            .to_vec();
        assert_eq!(
            membership.unevaluated_collection_handles(),
            expected_unevaluated
        );
        assert_eq!(membership.collections().len(), 1);
        assert_eq!(
            membership.collections()[0].collection_handle(),
            "configured"
        );
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
        let spec = parse_spec("[collections.tool]\ndirectory = \"Album\"\n");

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
            "[collections.audio]\ndirectory = \"Album\"\n\
             [collections.with-name]\nname = \"Named\"\ndirectory = \"Album\"\n",
        );

        let snapshot = SourceTreeSnapshot::capture(&source);
        let result = build_observed_source_file_inventory(&spec, &source);
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
