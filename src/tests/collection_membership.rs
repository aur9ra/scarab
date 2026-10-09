/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::*;
use crate::observed_source_file_inventory::build_observed_source_file_inventory;
use crate::test_support::{SourceTreeSnapshot, TempSandbox, canonical, create_source, parse_spec};
use std::collections::BTreeSet;
use std::fs;

/// Builds a completed source-file inventory and evaluates membership.
fn membership_for<'metadata, MetadataFailure>(
    spec: &LibraryBuildSpec,
    source: &Path,
    metadata_extraction_outcomes: &'metadata MetadataExtractionOutcomes<MetadataFailure>,
) -> CollectionMembership<'metadata, MetadataFailure> {
    let success =
        build_observed_source_file_inventory(spec, source).expect("inventory must succeed");
    evaluate_collection_membership(spec, success.inventory(), metadata_extraction_outcomes)
}

/// Builds a complete observation from raw family values.
fn observation(
    album_names: &[&str],
    album_artists: &[&str],
    track_artists: &[&str],
) -> MetadataSelectorObservation {
    fn values(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|item| (*item).to_owned()).collect()
    }
    MetadataSelectorObservation::new(
        values(album_names),
        values(album_artists),
        values(track_artists),
    )
}

/// Returns the collection named by `collection_handle`.
fn collection<'a, 'metadata, MetadataFailure>(
    membership: &'a CollectionMembership<'metadata, MetadataFailure>,
    collection_handle: &str,
) -> &'a CollectionMembers<'metadata, MetadataFailure> {
    membership
        .collections()
        .iter()
        .find(|collection| collection.collection_handle() == collection_handle)
        .unwrap_or_else(|| panic!("collection {collection_handle} must be evaluated"))
}

/// Returns the sorted members of `collection_handle`.
///
/// Sorting keeps assertions independent of unspecified member order.
fn sorted_members<'metadata, MetadataFailure>(
    membership: &CollectionMembership<'metadata, MetadataFailure>,
    collection_handle: &str,
) -> Vec<PathBuf> {
    let mut members = collection(membership, collection_handle).members().to_vec();
    members.sort();
    members
}

/// Returns the sorted unresolved pathnames of `collection_handle`.
fn sorted_unresolved_paths<'metadata, MetadataFailure>(
    membership: &CollectionMembership<'metadata, MetadataFailure>,
    collection_handle: &str,
) -> Vec<PathBuf> {
    let mut paths = collection(membership, collection_handle)
        .unresolved()
        .iter()
        .map(|entry| entry.path().to_path_buf())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

/// Returns the reason recorded for `path` in `collection_handle`.
fn reason_for<'a, 'metadata, MetadataFailure>(
    membership: &'a CollectionMembership<'metadata, MetadataFailure>,
    collection_handle: &str,
    path: &Path,
) -> &'a UnresolvedMetadataReason<'metadata, MetadataFailure> {
    collection(membership, collection_handle)
        .unresolved()
        .iter()
        .find(|entry| entry.path() == path)
        .unwrap_or_else(|| {
            panic!(
                "{} must be unresolved in {collection_handle}",
                path.display()
            )
        })
        .reason()
}

/// Escapes `path` for use as a TOML basic string.
fn toml_string(path: &Path) -> String {
    let mut escaped = String::from("\"");
    for character in path.display().to_string().chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            character if character.is_control() => {
                escaped.push_str(&format!("\\u{:04X}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

#[test]
fn directory_only_membership_ignores_supplied_metadata_and_stays_complete() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Album");
    fs::create_dir(&album).expect("create album");
    fs::write(album.join("one.flac"), b"one").expect("write first track");
    fs::write(album.join("two.flac"), b"two").expect("write second track");
    let spec = parse_spec("[collections.dir]\ndirectory = \"Album\"\n");

    let one = canonical(&album).join("one.flac");
    let two = canonical(&album).join("two.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<String> = HashMap::new();
    metadata_extraction_outcomes.insert(one.clone(), Err("failure".to_owned()));

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    assert_eq!(membership.collections().len(), 1);
    assert!(membership.is_complete());
    assert!(collection(&membership, "dir").is_complete());
    let mut expected = vec![one, two];
    expected.sort();
    assert_eq!(sorted_members(&membership, "dir"), expected);
    assert!(sorted_unresolved_paths(&membership, "dir").is_empty());
}

#[test]
fn directory_only_membership_does_not_inspect_source_audio_contents() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Fear of a Blank Planet");
    fs::create_dir_all(album.join("bonus")).expect("create album directories");
    // these `.flac` paths qualify even though their contents are invalid media
    fs::write(album.join("01 - Anesthetize.flac"), b"not a media file").expect("write track");
    fs::write(album.join("bonus/02 - My Ashes.FLAC"), b"").expect("write empty track");
    let spec = parse_spec(
        "[collections.fear-of-a-blank-planet]\ndirectory = \"Fear of a Blank Planet\"\n",
    );

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_observed_source_file_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("inventory must succeed");

    let metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    let membership =
        evaluate_collection_membership(&spec, success.inventory(), &metadata_extraction_outcomes);

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

    let metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

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

    let metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

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
        let metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
        let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

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
fn covered_directory_scopes_without_source_audio_yield_complete_empty_membership() {
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

    let metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    assert!(membership.is_complete());
    assert_eq!(membership.collections().len(), 2);
    assert_eq!(sorted_members(&membership, "empty"), Vec::<PathBuf>::new());
    assert_eq!(
        sorted_members(&membership, "non-audio"),
        Vec::<PathBuf>::new()
    );
}

#[test]
fn metadata_only_domain_is_the_default_root_not_every_inventory_file() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("local.flac"), b"local").expect("write local track");
    let outside = sandbox.path().join("outside");
    fs::create_dir(&outside).expect("create outside directory");
    fs::write(outside.join("foreign.flac"), b"foreign").expect("write foreign track");
    let spec = parse_spec(&format!(
        "[collections.configured]\ndirectory = {}\n\
         [collections.meta]\nalbum_name = \"Keep\"\n",
        toml_string(&outside)
    ));

    let local = canonical(&source).join("local.flac");
    let foreign = canonical(&outside).join("foreign.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    metadata_extraction_outcomes.insert(local.clone(), Ok(observation(&["Keep"], &[], &[])));
    // a matching outcome outside the default-root scope must not widen the domain
    metadata_extraction_outcomes.insert(foreign.clone(), Ok(observation(&["Keep"], &[], &[])));

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    assert_eq!(sorted_members(&membership, "meta"), vec![local]);
    assert!(collection(&membership, "meta").is_complete());
    assert_eq!(sorted_members(&membership, "configured"), vec![foreign]);
}

#[test]
fn directory_plus_metadata_is_an_intersection_gating_each_in_domain_file() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Album");
    let other = source.join("Other");
    fs::create_dir(&album).expect("create album");
    fs::create_dir(&other).expect("create other");
    fs::write(album.join("keep.flac"), b"keep").expect("write keep");
    fs::write(album.join("drop.flac"), b"drop").expect("write drop");
    fs::write(other.join("elsewhere.flac"), b"elsewhere").expect("write elsewhere");
    let spec = parse_spec(
        "[collections.combined]\ndirectory = \"Album\"\nalbum_name = \"Keep\"\n\
         [collections.other]\ndirectory = \"Other\"\n",
    );

    let keep = canonical(&album).join("keep.flac");
    let drop = canonical(&album).join("drop.flac");
    let elsewhere = canonical(&other).join("elsewhere.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    metadata_extraction_outcomes.insert(keep.clone(), Ok(observation(&["Keep"], &[], &[])));
    metadata_extraction_outcomes.insert(drop.clone(), Ok(observation(&["Other"], &[], &[])));
    // this match is reported only by the other collection's configured scope
    metadata_extraction_outcomes.insert(elsewhere.clone(), Ok(observation(&["Keep"], &[], &[])));

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    assert_eq!(sorted_members(&membership, "combined"), vec![keep]);
    // `drop` has a successful nonmatch, so it is a resolved non-member
    assert!(sorted_unresolved_paths(&membership, "combined").is_empty());
    assert!(collection(&membership, "combined").is_complete());
    assert!(!sorted_members(&membership, "combined").contains(&elsewhere));
    assert_eq!(sorted_members(&membership, "other"), vec![elsewhere]);
}

#[test]
fn directory_declarations_do_not_acquire_default_root_coverage() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Album");
    fs::create_dir(&album).expect("create album");
    fs::write(album.join("track.flac"), b"track").expect("write track");
    fs::write(source.join("root.flac"), b"root").expect("write root track");
    let spec = parse_spec(
        "[collections.dir]\ndirectory = \"Album\"\n\
         [collections.meta]\nalbum_name = \"Keep\"\n",
    );

    let track = canonical(&album).join("track.flac");
    let root = canonical(&source).join("root.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    metadata_extraction_outcomes.insert(root.clone(), Ok(observation(&["Keep"], &[], &[])));

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    // `track` is reported by both scopes. `dir` uses its configured scope,
    // while `meta` uses the default-root association, where `track` has no
    // outcome
    assert_eq!(sorted_members(&membership, "dir"), vec![track.clone()]);
    assert!(collection(&membership, "dir").is_complete());
    assert!(!sorted_members(&membership, "dir").contains(&root));

    assert_eq!(sorted_members(&membership, "meta"), vec![root]);
    assert_eq!(sorted_unresolved_paths(&membership, "meta"), vec![track]);
}

#[test]
fn metadata_evaluation_establishes_members_and_implicit_non_members() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("match.flac"), b"match").expect("write match");
    fs::write(source.join("miss.flac"), b"miss").expect("write miss");
    fs::write(source.join("absent.flac"), b"absent").expect("write absent");
    let spec = parse_spec("[collections.meta]\nalbum_name = \"Keep\"\n");

    let matched = canonical(&source).join("match.flac");
    let mismatched = canonical(&source).join("miss.flac");
    let absent = canonical(&source).join("absent.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    metadata_extraction_outcomes.insert(matched.clone(), Ok(observation(&["Keep"], &[], &[])));
    metadata_extraction_outcomes.insert(mismatched.clone(), Ok(observation(&["Other"], &[], &[])));

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    assert_eq!(sorted_members(&membership, "meta"), vec![matched]);
    assert_eq!(sorted_unresolved_paths(&membership, "meta"), vec![absent]);
    assert!(!sorted_members(&membership, "meta").contains(&mismatched));
}

#[test]
fn missing_outcomes_are_unavailable_and_failures_borrow_the_supplied_payload() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("failed.flac"), b"failed").expect("write failed");
    fs::write(source.join("absent.flac"), b"absent").expect("write absent");
    let spec = parse_spec("[collections.meta]\nalbum_name = \"Keep\"\n");

    let failed = canonical(&source).join("failed.flac");
    let absent = canonical(&source).join("absent.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<String> = HashMap::new();
    metadata_extraction_outcomes.insert(failed.clone(), Err("extraction failed".to_owned()));
    let stored = match metadata_extraction_outcomes
        .get(&failed)
        .expect("failure entry must exist")
    {
        Err(payload) => payload,
        Ok(_) => unreachable!("the test entry is a failure"),
    };

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    match reason_for(&membership, "meta", &failed) {
        UnresolvedMetadataReason::Failed(payload) => assert!(
            std::ptr::eq(*payload, stored),
            "membership must borrow the exact supplied failure payload"
        ),
        UnresolvedMetadataReason::Unavailable => {
            panic!("a supplied failure must stay a failure")
        }
    }
    assert!(matches!(
        reason_for(&membership, "meta", &absent),
        UnresolvedMetadataReason::Unavailable
    ));
    assert!(!membership.is_complete());
}

#[test]
fn every_declaration_is_represented_with_derived_completeness() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Album");
    let empty = source.join("Empty");
    fs::create_dir(&album).expect("create album");
    fs::create_dir(&empty).expect("create empty");
    fs::write(album.join("track.flac"), b"track").expect("write track");
    fs::write(empty.join("notes.txt"), b"notes").expect("write notes");
    let spec = parse_spec(
        "[collections.dir]\ndirectory = \"Album\"\n\
         [collections.meta]\nalbum_name = \"Keep\"\n\
         [collections.combined]\ndirectory = \"Album\"\nalbum_name = \"Keep\"\n\
         [collections.empty]\ndirectory = \"Empty\"\n",
    );

    let track = canonical(&album).join("track.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    metadata_extraction_outcomes.insert(track.clone(), Ok(observation(&["Keep"], &[], &[])));

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    let handles: Vec<&str> = membership
        .collections()
        .iter()
        .map(CollectionMembers::collection_handle)
        .collect();
    assert_eq!(handles, ["dir", "meta", "combined", "empty"]);
    assert!(membership.is_complete());
    assert_eq!(sorted_members(&membership, "dir"), vec![track.clone()]);
    assert_eq!(sorted_members(&membership, "meta"), vec![track.clone()]);
    assert_eq!(sorted_members(&membership, "combined"), vec![track.clone()]);
    assert_eq!(sorted_members(&membership, "empty"), Vec::<PathBuf>::new());
    assert!(collection(&membership, "empty").is_complete());

    // without the outcome, metadata-dependent collections are incomplete,
    // while directory-only collections remain complete
    let without_outcome: MetadataExtractionOutcomes<()> = HashMap::new();
    let membership = membership_for(&spec, &source, &without_outcome);
    assert!(!membership.is_complete());
    assert!(collection(&membership, "dir").is_complete());
    assert_eq!(sorted_members(&membership, "dir"), vec![track.clone()]);
    assert_eq!(
        sorted_unresolved_paths(&membership, "meta"),
        vec![track.clone()]
    );
    assert_eq!(
        sorted_unresolved_paths(&membership, "combined"),
        vec![track]
    );
    assert!(collection(&membership, "empty").is_complete());
}

#[test]
fn successful_empty_observations_are_distinct_from_unavailability() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("present.flac"), b"present").expect("write present");
    fs::write(source.join("empty.flac"), b"empty").expect("write empty");
    fs::write(source.join("absent.flac"), b"absent").expect("write absent");
    let spec = parse_spec("[collections.meta]\nalbum_name = \"\"\n");

    let present = canonical(&source).join("present.flac");
    let empty = canonical(&source).join("empty.flac");
    let absent = canonical(&source).join("absent.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    // a present empty album name matches the empty-string predicate
    metadata_extraction_outcomes.insert(present.clone(), Ok(observation(&[""], &[], &[])));
    // three empty families are a successful observation but contain no
    // value matching the empty-string predicate
    metadata_extraction_outcomes.insert(empty.clone(), Ok(observation(&[], &[], &[])));

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    assert_eq!(sorted_members(&membership, "meta"), vec![present]);
    assert!(!sorted_members(&membership, "meta").contains(&empty));
    assert!(matches!(
        reason_for(&membership, "meta", &absent),
        UnresolvedMetadataReason::Unavailable
    ));
    assert_eq!(sorted_unresolved_paths(&membership, "meta"), vec![absent]);
}

#[test]
fn extra_metadata_entries_widen_neither_domain_nor_files() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("local.flac"), b"local").expect("write local");
    let outside = sandbox.path().join("outside");
    fs::create_dir(&outside).expect("create outside directory");
    fs::write(outside.join("foreign.flac"), b"foreign").expect("write foreign");
    let spec = parse_spec(&format!(
        "[collections.configured]\ndirectory = {}\n\
         [collections.meta]\nalbum_name = \"Keep\"\n",
        toml_string(&outside)
    ));

    let local = canonical(&source).join("local.flac");
    let foreign = canonical(&outside).join("foreign.flac");
    let ghost = canonical(&source).join("ghost.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<String> = HashMap::new();
    metadata_extraction_outcomes.insert(local.clone(), Ok(observation(&["Keep"], &[], &[])));
    // `ghost` is absent from the inventory, and `foreign` is outside
    // `meta`'s domain. Neither outcome affects `meta`
    metadata_extraction_outcomes.insert(ghost, Err("ghost".to_owned()));
    metadata_extraction_outcomes.insert(foreign, Err("irrelevant".to_owned()));

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    assert_eq!(sorted_members(&membership, "meta"), vec![local]);
    assert!(sorted_unresolved_paths(&membership, "meta").is_empty());
    assert!(collection(&membership, "meta").is_complete());
}

#[test]
fn native_pathname_equality_preserves_inventory_spelling() {
    fn with_trailing_separator(path: &Path) -> PathBuf {
        let mut spelling = path.as_os_str().to_os_string();
        spelling.push(std::path::MAIN_SEPARATOR_STR);
        PathBuf::from(spelling)
    }

    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("track.flac"), b"track").expect("write track");
    fs::write(source.join("failed.flac"), b"failed").expect("write failed");
    let spec = parse_spec("[collections.meta]\nalbum_name = \"Keep\"\n");

    let track = canonical(&source).join("track.flac");
    let failed = canonical(&source).join("failed.flac");
    // `Path` equality ignores trailing native separators, including on Windows
    // verbatim paths. Appending to `OsString` preserves the distinct spelling
    let track_key = with_trailing_separator(&track);
    let failed_key = with_trailing_separator(&failed);
    assert_ne!(track.as_os_str(), track_key.as_os_str());
    assert_ne!(failed.as_os_str(), failed_key.as_os_str());
    assert_eq!(track, track_key);
    assert_eq!(failed, failed_key);

    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<String> = HashMap::new();
    metadata_extraction_outcomes.insert(track_key, Ok(observation(&["Keep"], &[], &[])));
    metadata_extraction_outcomes.insert(failed_key, Err("nope".to_owned()));

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    let members = sorted_members(&membership, "meta");
    assert_eq!(members.len(), 1);
    assert_eq!(members[0].as_os_str(), track.as_os_str());
    let unresolved = sorted_unresolved_paths(&membership, "meta");
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].as_os_str(), failed.as_os_str());
    assert!(matches!(
        reason_for(&membership, "meta", &failed),
        UnresolvedMetadataReason::Failed(error) if error.as_str() == "nope"
    ));
}

#[test]
fn explicit_empty_unit_outcomes_are_valid() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Album");
    fs::create_dir(&album).expect("create album");
    fs::write(album.join("track.flac"), b"track").expect("write track");
    let spec = parse_spec(
        "[collections.dir]\ndirectory = \"Album\"\n\
         [collections.meta]\nalbum_name = \"Keep\"\n",
    );

    let track = canonical(&album).join("track.flac");
    let metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    assert_eq!(sorted_members(&membership, "dir"), vec![track.clone()]);
    assert!(collection(&membership, "dir").is_complete());
    assert_eq!(sorted_unresolved_paths(&membership, "meta"), vec![track]);
    assert!(!membership.is_complete());
}

#[test]
fn evaluation_imposes_no_trait_bounds_on_the_opaque_payload() {
    struct Opaque {
        marker: u32,
    }

    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("track.flac"), b"track").expect("write track");
    let spec = parse_spec("[collections.meta]\nalbum_name = \"Keep\"\n");

    let track = canonical(&source).join("track.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<Opaque> = HashMap::new();
    metadata_extraction_outcomes.insert(track.clone(), Err(Opaque { marker: 7 }));
    let stored = match metadata_extraction_outcomes
        .get(&track)
        .expect("failure entry must exist")
    {
        Err(payload) => payload,
        Ok(_) => unreachable!("the test entry is a failure"),
    };

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    match reason_for(&membership, "meta", &track) {
        UnresolvedMetadataReason::Failed(payload) => {
            assert!(std::ptr::eq(*payload, stored));
            assert_eq!(payload.marker, 7);
        }
        UnresolvedMetadataReason::Unavailable => {
            panic!("expected the supplied failure")
        }
    }
}

#[test]
fn metadata_only_collections_for_each_selector_family_are_evaluated() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("named.flac"), b"named").expect("write named");
    fs::write(source.join("artist.flac"), b"artist").expect("write artist");
    fs::write(source.join("other.flac"), b"other").expect("write other");
    let spec = parse_spec(
        "[collections.by-name]\nalbum_name = \"Album\"\n\
         [collections.by-artist]\nalbum_artists = [\"Artist\"]\n\
         [collections.by-track]\ntrack_artists = [\"Performer\"]\n",
    );

    let named = canonical(&source).join("named.flac");
    let artist = canonical(&source).join("artist.flac");
    let other = canonical(&source).join("other.flac");
    let mut metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    metadata_extraction_outcomes.insert(named.clone(), Ok(observation(&["Album"], &[], &[])));
    metadata_extraction_outcomes.insert(artist.clone(), Ok(observation(&[], &["Artist"], &[])));
    metadata_extraction_outcomes.insert(other.clone(), Ok(observation(&[], &[], &[])));

    let membership = membership_for(&spec, &source, &metadata_extraction_outcomes);

    assert_eq!(sorted_members(&membership, "by-name"), vec![named]);
    assert_eq!(sorted_members(&membership, "by-artist"), vec![artist]);
    assert_eq!(
        sorted_members(&membership, "by-track"),
        Vec::<PathBuf>::new()
    );
    assert!(membership.is_complete());
}

#[test]
fn membership_evaluation_does_not_mutate_the_source_tree() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Album");
    fs::create_dir(&album).expect("create album");
    fs::write(album.join("track.flac"), b"track").expect("write track");
    fs::write(album.join("notes.txt"), b"notes").expect("write notes");
    let spec = parse_spec(
        "[collections.audio]\ndirectory = \"Album\"\n\
         [collections.with-album-name]\nalbum_name = \"Named\"\ndirectory = \"Album\"\n",
    );

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_observed_source_file_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("inventory must succeed");

    let metadata_extraction_outcomes: MetadataExtractionOutcomes<()> = HashMap::new();
    let membership =
        evaluate_collection_membership(&spec, success.inventory(), &metadata_extraction_outcomes);
    snapshot.assert_unchanged();

    assert_eq!(
        sorted_members(&membership, "audio"),
        vec![canonical(&album).join("track.flac")]
    );
}
