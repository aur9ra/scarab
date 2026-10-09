/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::*;
use std::ffi::OsStr;

fn configured_scope(
    collection_handle: &str,
    resolved_directory: &str,
    contributing_selectors: &[&str],
) -> RequiredScope {
    RequiredScope::ConfiguredDirectory {
        collection_handle: collection_handle.to_owned(),
        resolved_directory: PathBuf::from(resolved_directory),
        contributing_selectors: contributing_selectors.iter().map(PathBuf::from).collect(),
    }
}

fn find_entry<'inventory>(
    inventory: &'inventory ObservedSourceFileInventory,
    path: &Path,
) -> Option<ObservedSourceFile<'inventory>> {
    inventory
        .files()
        .find(|observed_file| observed_file.path() == path)
}

#[test]
fn equal_path_observations_with_different_spellings_share_one_observed_source_file() {
    let scopes = vec![
        configured_scope("one", "/music/album", &["Album"]),
        configured_scope("two", "/music/album", &["Album"]),
    ];
    let first = PathBuf::from("/music/album/./track.dat");
    let second = PathBuf::from("/music/album/track.dat");
    assert_ne!(
        first.as_os_str(),
        second.as_os_str(),
        "test precondition needs different raw spellings"
    );
    assert_eq!(first, second, "test precondition needs equal native paths");

    let inventory = aggregate_observations(scopes, vec![vec![first.clone()], vec![second.clone()]]);

    assert_eq!(inventory.covered_scopes().len(), 2);
    let mut observed_files: Vec<ObservedSourceFile<'_>> = inventory.files().collect();
    assert_eq!(
        observed_files.len(),
        1,
        "equal observations must deduplicate"
    );
    let observed_file = observed_files.pop().expect("one observed source file");
    let representative = observed_file.path();
    assert!(
        representative == first.as_path() || representative == second.as_path(),
        "representative must be one reported pathname"
    );
    assert!(
        representative.as_os_str() == first.as_os_str()
            || representative.as_os_str() == second.as_os_str(),
        "representative spelling must match one observation"
    );
    let mut handles: Vec<&str> = observed_file
        .reporting_scopes()
        .map(|scope| match scope {
            RequiredScope::ConfiguredDirectory {
                collection_handle, ..
            } => collection_handle.as_str(),
            RequiredScope::DefaultSourceRoot { .. } => {
                panic!("expected configured scope")
            }
        })
        .collect();
    handles.sort();
    assert_eq!(handles, ["one", "two"]);
    assert_eq!(observed_file.reporting_scopes().count(), 2);
}

#[test]
fn distinct_pathnames_stay_distinct() {
    let scopes = vec![configured_scope("one", "/music/album", &["Album"])];
    let inventory = aggregate_observations(
        scopes,
        vec![vec![
            PathBuf::from("/music/album/a.dat"),
            PathBuf::from("/music/album/b.dat"),
        ]],
    );

    assert_eq!(inventory.files().count(), 2);
    assert!(find_entry(&inventory, Path::new("/music/album/a.dat")).is_some());
    assert!(find_entry(&inventory, Path::new("/music/album/b.dat")).is_some());
}

#[test]
fn metadata_requirements_use_reporting_associations_not_equal_or_overlapping_roots() {
    use crate::metadata_extraction_requirements::required_metadata_paths;
    use crate::test_support::parse_spec;

    let spec = parse_spec(
        "[collections.meta]\ndirectory = \"Album\"\nalbum_name = \"Keep\"\n\
         [collections.dir]\ndirectory = \"Album\"\n",
    );
    let track = PathBuf::from("/music/album/track.flac");
    for metadata_root in ["/music/album", "/music"] {
        let scopes = vec![
            configured_scope("meta", metadata_root, &["Album"]),
            configured_scope("dir", "/music/album", &["Album"]),
        ];
        // coverage can observe different contents at different scan times
        let empty_files_per_scope = vec![vec![], vec![track.clone()]];
        let inventory = aggregate_observations(scopes, empty_files_per_scope);
        assert!(required_metadata_paths(&spec, &inventory).is_empty());
    }

    // a reported pathname qualifies even outside the scope's lexical root
    let inventory = aggregate_observations(
        vec![configured_scope("meta", "/elsewhere", &["Album"])],
        vec![vec![track.clone()]],
    );
    assert_eq!(
        required_metadata_paths(&spec, &inventory),
        vec![track.as_path()]
    );
}

#[test]
fn metadata_requirements_borrow_inventory_retained_spelling() {
    use crate::metadata_extraction_requirements::required_metadata_paths;
    use crate::test_support::parse_spec;

    let spec = parse_spec("[collections.meta]\ndirectory = \"Album\"\nalbum_name = \"Keep\"\n");
    let first_spelling = PathBuf::from("/music/album/./track.FlaC");
    let second_spelling = PathBuf::from("/music/album/track.FlaC");
    assert_eq!(first_spelling, second_spelling);
    assert_ne!(first_spelling.as_os_str(), second_spelling.as_os_str());
    let inventory = aggregate_observations(
        vec![configured_scope("meta", "/music/album", &["Album"])],
        vec![vec![first_spelling, second_spelling]],
    );
    let retained = inventory
        .files()
        .next()
        .expect("aggregation must retain one observed source file")
        .path()
        .to_path_buf();
    let paths = required_metadata_paths(&spec, &inventory);
    assert_eq!(paths.len(), 1);
    assert_eq!(paths[0].as_os_str(), retained.as_os_str());
    assert!(std::ptr::eq(
        paths[0],
        inventory.files().next().unwrap().path()
    ));
}

#[test]
#[should_panic(
    expected = "reporting configured-scope handle must exist in the paired build specification"
)]
fn metadata_requirements_reject_unknown_configured_handle() {
    use crate::metadata_extraction_requirements::required_metadata_paths;
    use crate::test_support::parse_spec;

    let spec = parse_spec("[collections.meta]\nalbum_name = \"Keep\"\n");
    let inventory = aggregate_observations(
        vec![configured_scope("unknown", "/music", &["Album"])],
        vec![vec![PathBuf::from("/music/track.flac")]],
    );
    required_metadata_paths(&spec, &inventory);
}

#[test]
fn empty_scopes_stay_covered_without_observed_files() {
    let scopes = vec![
        configured_scope("one", "/music/one", &["one"]),
        configured_scope("two", "/music/two", &["two"]),
    ];
    let inventory = aggregate_observations(
        scopes,
        vec![vec![PathBuf::from("/music/one/track.dat")], Vec::new()],
    );

    assert_eq!(inventory.covered_scopes().len(), 2);
    assert_eq!(inventory.files().count(), 1);
    let observed_file = find_entry(&inventory, Path::new("/music/one/track.dat"))
        .expect("populated scope file must exist");
    assert_eq!(observed_file.reporting_scopes().count(), 1);
}

#[test]
fn same_scope_duplicate_observation_keeps_one_association() {
    let scopes = vec![configured_scope("one", "/music/album", &["Album"])];
    let inventory = aggregate_observations(
        scopes,
        vec![vec![
            PathBuf::from("/music/album/track.dat"),
            PathBuf::from("/music/album/./track.dat"),
        ]],
    );

    assert_eq!(inventory.files().count(), 1);
    let observed_file = find_entry(&inventory, Path::new("/music/album/track.dat"))
        .expect("observed source file must exist");
    assert_eq!(
        observed_file.reporting_scopes().count(),
        1,
        "one scope yields one association even with duplicate equal observations"
    );
}

#[test]
fn required_scope_translation_preserves_owned_fields_and_spellings() {
    let configured = required_scope_from_configured_directory(
        crate::collection_scope::ConfiguredDirectoryScope {
            collection_handle: "deadwing".to_owned(),
            resolved_directory: PathBuf::from("/music/./Deadwing"),
            contributing_selectors: vec![
                PathBuf::from("Deadwing"),
                PathBuf::from("./Deadwing"),
                PathBuf::from("Deadwing"),
            ],
        },
    );
    match configured {
        RequiredScope::ConfiguredDirectory {
            collection_handle,
            resolved_directory,
            contributing_selectors,
        } => {
            assert_eq!(collection_handle, "deadwing");
            assert_eq!(
                resolved_directory.as_os_str(),
                OsStr::new("/music/./Deadwing"),
                "resolved spelling must move unchanged"
            );
            assert_eq!(
                contributing_selectors.len(),
                3,
                "length of contributing selectors must survive"
            );
            assert_eq!(
                contributing_selectors[0].as_os_str(),
                OsStr::new("Deadwing")
            );
            assert_eq!(
                contributing_selectors[1].as_os_str(),
                OsStr::new("./Deadwing")
            );
            assert_eq!(
                contributing_selectors[2].as_os_str(),
                OsStr::new("Deadwing")
            );
        }
        other => panic!("configured translation must stay configured, got {other:?}"),
    }

    let default =
        required_scope_from_default_source_root(crate::collection_scope::DefaultSourceRootScope {
            original_source_root: PathBuf::from("./source"),
            resolved_traversal_root: PathBuf::from("/music/source/./"),
            dependent_collection_handles: vec!["z".to_owned(), "a".to_owned()],
        });
    match default {
        RequiredScope::DefaultSourceRoot {
            original_source_root,
            resolved_traversal_root,
            dependent_collection_handles,
        } => {
            assert_eq!(
                original_source_root.as_os_str(),
                OsStr::new("./source"),
                "caller spelling must move unchanged"
            );
            assert_eq!(
                resolved_traversal_root.as_os_str(),
                OsStr::new("/music/source/./"),
                "resolved spelling must move unchanged"
            );
            assert_eq!(
                dependent_collection_handles.len(),
                2,
                "length of dependent collection handles must survive"
            );
            assert_eq!(dependent_collection_handles, ["z", "a"]);
        }
        other => panic!("default translation must stay default, got {other:?}"),
    }
}
