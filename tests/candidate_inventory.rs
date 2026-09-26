/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Integration tests for filesystem candidate inventory.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use scarab::{
    CandidateInventoryFailure, LibraryBuildSpec, RequiredScope, build_candidate_inventory,
};

mod common;

use common::{SourceTreeSnapshot, TempSandbox};

fn parse_spec(album_declarations: &str) -> LibraryBuildSpec {
    let text = format!("codec = \"opus\"\nbitrate = 128\n{album_declarations}");
    scarab::parse(&text).expect("test configuration must parse and validate")
}

fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path)
        .unwrap_or_else(|error| panic!("expected {} to canonicalize: {error}", path.display()))
}

fn create_source(sandbox: &TempSandbox) -> PathBuf {
    let source = sandbox.path().join("source");
    fs::create_dir(&source).expect("create source root");
    source
}

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

fn find_configured_scope<'a>(scopes: &'a [RequiredScope], album_handle: &str) -> &'a RequiredScope {
    scopes
        .iter()
        .find(|scope| match scope {
            RequiredScope::ConfiguredDirectory {
                album_handle: handle,
                ..
            } => handle == album_handle,
            RequiredScope::DefaultSourceRoot { .. } => false,
        })
        .unwrap_or_else(|| panic!("configured scope for album {album_handle} must exist"))
}

#[test]
fn zero_albums_missing_unrelated_root_succeeds_empty() {
    let sandbox = TempSandbox::new();
    let missing_root = sandbox.path().join("missing-root");
    let spec = parse_spec("");

    let result = build_candidate_inventory(&spec, &missing_root).expect("zero albums must succeed");

    assert!(result.warnings().is_empty());
    assert!(result.inventory().covered_scopes().is_empty());
    assert_eq!(result.inventory().candidates().count(), 0);
}

#[test]
fn absolute_configured_only_scope_ignores_unusable_source_root() {
    let sandbox = TempSandbox::new();
    let target = sandbox.path().join("absolute-target");
    fs::create_dir(&target).expect("create target");
    fs::write(target.join("track.dat"), b"track").expect("write track");
    let missing_root = sandbox.path().join("missing-root");
    let spec = parse_spec(&format!(
        "[albums.tool]\ndirectory = {}\n",
        toml_string(&target)
    ));

    let snapshot = SourceTreeSnapshot::capture(sandbox.path());
    let result = build_candidate_inventory(&spec, &missing_root);
    snapshot.assert_unchanged();
    let success = result.expect("absolute selector must ignore the missing root");

    assert!(success.warnings().is_empty());
    let scopes = success.inventory().covered_scopes();
    assert_eq!(scopes.len(), 1);
    match &scopes[0] {
        RequiredScope::ConfiguredDirectory {
            album_handle,
            resolved_directory,
            contributing_selectors,
        } => {
            assert_eq!(album_handle, "tool");
            assert_eq!(resolved_directory, &canonical(&target));
            assert_eq!(contributing_selectors.len(), 1);
            assert_eq!(
                contributing_selectors[0].as_os_str(),
                target.as_os_str(),
                "absolute spelling must be preserved"
            );
        }
        other => panic!("expected configured scope, got {other:?}"),
    }
    let candidates: Vec<_> = success.inventory().candidates().collect();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].path(),
        canonical(&target).join("track.dat").as_path()
    );
    assert_eq!(candidates[0].scopes().count(), 1);
}

#[test]
fn shared_default_single_scope_with_declaration_ordered_dependents() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("sentinel.txt"), b"sentinel").expect("write sentinel");
    let spec = parse_spec(
        "[albums.z]\nname = \"Z\"\n\
         [albums.a]\nname = \"A\"\n\
         [albums.q]\nartist = \"Q\"\n",
    );

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_candidate_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("shared default must succeed");

    assert!(success.warnings().is_empty());
    let scopes = success.inventory().covered_scopes();
    assert_eq!(scopes.len(), 1);
    match &scopes[0] {
        RequiredScope::DefaultSourceRoot {
            original_source_root,
            resolved_traversal_root,
            dependent_album_handles,
        } => {
            assert_eq!(original_source_root.as_os_str(), source.as_os_str());
            assert_eq!(resolved_traversal_root, &canonical(&source));
            assert_eq!(dependent_album_handles, &["z", "a", "q"]);
        }
        other => panic!("expected default scope, got {other:?}"),
    }
    let candidates: Vec<_> = success.inventory().candidates().collect();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].path(),
        canonical(&source).join("sentinel.txt").as_path()
    );
    assert_eq!(candidates[0].scopes().count(), 1);
}

#[test]
fn mixed_empty_and_populated_scopes_both_stay_visible() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let populated = source.join("populated");
    fs::create_dir(&populated).expect("create populated");
    fs::write(populated.join("track.dat"), b"track").expect("write track");
    fs::create_dir(source.join("empty-album")).expect("create empty album");
    let spec = parse_spec(
        "[albums.one]\ndirectory = \"populated\"\n\
         [albums.two]\ndirectory = \"empty-album\"\n",
    );

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_candidate_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("mixed scopes must succeed");

    assert!(success.warnings().is_empty());
    let scopes = success.inventory().covered_scopes();
    assert_eq!(scopes.len(), 2);
    let one = find_configured_scope(scopes, "one");
    let two = find_configured_scope(scopes, "two");
    match one {
        RequiredScope::ConfiguredDirectory {
            resolved_directory, ..
        } => {
            assert_eq!(resolved_directory, &canonical(&populated));
        }
        _ => unreachable!(),
    }
    match two {
        RequiredScope::ConfiguredDirectory {
            resolved_directory, ..
        } => {
            assert_eq!(resolved_directory, &canonical(&source.join("empty-album")));
        }
        _ => unreachable!(),
    }
    let candidates: Vec<_> = success.inventory().candidates().collect();
    assert_eq!(candidates.len(), 1);
    let candidate = success
        .inventory()
        .candidates()
        .find(|candidate| candidate.path() == canonical(&populated).join("track.dat").as_path())
        .expect("populated file must be a candidate");
    assert_eq!(candidate.scopes().count(), 1);
    let _ = two;
}

#[test]
fn within_album_overlap_parent_and_nested_scopes() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let parent = source.join("parent");
    let child = parent.join("child");
    fs::create_dir_all(&child).expect("create parent and child");
    fs::write(parent.join("root.txt"), b"root").expect("write root file");
    fs::write(child.join("track.dat"), b"nested").expect("write nested file");
    let spec = parse_spec("[albums.tool]\ndirectories = [\"parent\", \"parent/child\"]\n");

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_candidate_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("overlapping scopes must succeed");

    assert!(success.warnings().is_empty());
    assert_eq!(success.inventory().covered_scopes().len(), 2);
    let candidates: Vec<_> = success.inventory().candidates().collect();
    assert_eq!(candidates.len(), 2, "two distinct pathnames total");
    let root = success
        .inventory()
        .candidates()
        .find(|candidate| candidate.path() == canonical(&parent).join("root.txt").as_path())
        .expect("root file must be a candidate");
    assert_eq!(root.scopes().count(), 1, "root file belongs to one scope");
    let nested = success
        .inventory()
        .candidates()
        .find(|candidate| candidate.path() == canonical(&child).join("track.dat").as_path())
        .expect("nested file must be a candidate");
    assert_eq!(
        nested.scopes().count(),
        2,
        "nested file belongs to both scopes"
    );
}

#[test]
fn across_album_overlap_equal_path_retains_both_scopes() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let shared = source.join("shared");
    fs::create_dir(&shared).expect("create shared");
    fs::write(shared.join("track.dat"), b"track").expect("write track");
    let spec = parse_spec(
        "[albums.one]\ndirectory = \"shared\"\n\
         [albums.two]\ndirectory = \"shared\"\n",
    );

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_candidate_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("shared roots must succeed");

    assert!(success.warnings().is_empty());
    let scopes = success.inventory().covered_scopes();
    assert_eq!(scopes.len(), 2);
    let expected = canonical(&shared).join("track.dat");
    let candidates: Vec<_> = success.inventory().candidates().collect();
    assert_eq!(candidates.len(), 1, "one equal-path candidate");
    let candidate = &candidates[0];
    assert_eq!(candidate.path(), expected.as_path());
    assert_eq!(candidate.scopes().count(), 2, "both scopes stay associated");
    let mut handles: Vec<&str> = candidate
        .scopes()
        .map(|scope| match scope {
            RequiredScope::ConfiguredDirectory { album_handle, .. } => album_handle.as_str(),
            RequiredScope::DefaultSourceRoot { .. } => panic!("expected configured scope"),
        })
        .collect();
    handles.sort();
    assert_eq!(handles, ["one", "two"]);
}

#[test]
fn duplicate_selectors_with_dot_spelling_keep_one_association() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Album");
    fs::create_dir(&album).expect("create album");
    fs::write(album.join("track.dat"), b"track").expect("write track");
    let spec = parse_spec("[albums.tool]\ndirectories = [\"Album\", \"Album/.\", \"Album\"]\n");

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_candidate_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("duplicate selectors must succeed");

    let scopes = success.inventory().covered_scopes();
    assert_eq!(scopes.len(), 1);
    match &scopes[0] {
        RequiredScope::ConfiguredDirectory {
            album_handle,
            resolved_directory,
            contributing_selectors,
        } => {
            assert_eq!(album_handle, "tool");
            assert_eq!(resolved_directory, &canonical(&album));
            assert_eq!(contributing_selectors.len(), 3);
            assert_eq!(contributing_selectors[0].as_os_str(), OsStr::new("Album"));
            assert_eq!(contributing_selectors[1].as_os_str(), OsStr::new("Album/."));
            assert_eq!(contributing_selectors[2].as_os_str(), OsStr::new("Album"));
        }
        other => panic!("expected configured scope, got {other:?}"),
    }
    assert_eq!(success.warnings().len(), 1, "redundant group warns once");
    let warning = &success.warnings()[0];
    assert_eq!(warning.album_handle, "tool");
    assert_eq!(warning.resolved_directory, canonical(&album));
    assert_eq!(
        warning.contributing_selectors,
        match &scopes[0] {
            RequiredScope::ConfiguredDirectory {
                contributing_selectors,
                ..
            } => {
                contributing_selectors.clone()
            }
            _ => unreachable!(),
        }
    );
    let candidates: Vec<_> = success.inventory().candidates().collect();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].scopes().count(),
        1,
        "grouped occurrences yield one association"
    );
}

#[test]
fn configured_and_default_equal_roots_share_candidate_with_two_scopes() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("track.dat"), b"track").expect("write track");
    let spec = parse_spec(
        "[albums.one]\ndirectory = \".\"\n\
         [albums.two]\nname = \"Two\"\n",
    );

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_candidate_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("equal configured and default roots must succeed");

    assert!(success.warnings().is_empty());
    let scopes = success.inventory().covered_scopes();
    assert_eq!(scopes.len(), 2);
    let expected = canonical(&source).join("track.dat");
    let candidates: Vec<_> = success.inventory().candidates().collect();
    assert_eq!(candidates.len(), 1, "one shared candidate");
    assert_eq!(candidates[0].path(), expected.as_path());
    assert_eq!(
        candidates[0].scopes().count(),
        2,
        "configured and default scopes stay distinct"
    );
    assert!(
        scopes
            .iter()
            .any(|scope| matches!(scope, RequiredScope::ConfiguredDirectory { .. }))
    );
    assert!(
        scopes
            .iter()
            .any(|scope| matches!(scope, RequiredScope::DefaultSourceRoot { .. }))
    );
}

#[test]
fn file_content_and_configuration_blindness() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Album");
    fs::create_dir(&album).expect("create album");
    fs::write(album.join("invalid.flac"), b"not audio at all").expect("write invalid audio");
    fs::write(album.join("cover.jpg"), b"fake image").expect("write image");
    fs::write(album.join("notes.txt"), b"text notes").expect("write text");
    fs::write(album.join("alternate.ogg"), b"alternate extension").expect("write ogg");
    fs::write(album.join("no-extension"), b"raw bytes").expect("write extensionless");
    // Inventory ignores file policy, metadata predicates, and track rules.
    let spec = parse_spec(
        "[files]\ninclude = [\"flac\"]\nexclude = [\"txt\", \"jpg\"]\n\
         [albums.tool]\ndirectory = \"Album\"\nname = \"Tool\"\nartist = \"Tool\"\n\
         [[album_rules]]\nalbums = [\"tool\"]\nbitrate = 64\n\
         [[track_rules]]\nalbum = \"tool\"\nrules = [{ track = \"invalid.flac\", exclude = true }]\n",
    );

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_candidate_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("blind discovery must succeed");

    let mut names: Vec<String> = success
        .inventory()
        .candidates()
        .map(|candidate| {
            candidate
                .path()
                .file_name()
                .expect("candidate must have a file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "alternate.ogg",
            "cover.jpg",
            "invalid.flac",
            "no-extension",
            "notes.txt",
        ]
    );
}

#[test]
fn hard_link_pathnames_stay_separate_candidates() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Album");
    fs::create_dir(&album).expect("create album");
    let original = album.join("original.dat");
    fs::write(&original, b"shared bytes").expect("write original");
    let link = album.join("linked.dat");
    // Fail rather than silently skip hard-link coverage.
    fs::hard_link(&original, &link).expect("create hard link");

    let spec = parse_spec("[albums.tool]\ndirectory = \"Album\"\n");

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_candidate_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("hard link tree must succeed");

    let expected_original = canonical(&original);
    let expected_link = canonical(&album).join("linked.dat");
    assert_ne!(
        expected_original.as_os_str(),
        expected_link.as_os_str(),
        "test precondition needs distinct spellings"
    );
    let mut found_original = false;
    let mut found_link = false;
    for candidate in success.inventory().candidates() {
        if candidate.path() == expected_original.as_path() {
            found_original = true;
        } else if candidate.path() == expected_link.as_path() {
            found_link = true;
        }
    }
    assert!(found_original, "original pathname must stay a candidate");
    assert!(found_link, "linked pathname must stay a separate candidate");
    assert_eq!(
        success.inventory().candidates().count(),
        2,
        "hard links are distinct pathnames"
    );
}

#[test]
fn warning_on_success_retains_full_redundancy() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Stupid Dream");
    fs::create_dir(&album).expect("create album");
    fs::write(album.join("track.dat"), b"track").expect("write track");
    let spec =
        parse_spec("[albums.stupid-dream]\ndirectories = [\"Stupid Dream\", \"Stupid Dream\"]\n");

    let snapshot = SourceTreeSnapshot::capture(&source);
    let result = build_candidate_inventory(&spec, &source);
    snapshot.assert_unchanged();
    let success = result.expect("redundant selectors must succeed");

    assert_eq!(success.warnings().len(), 1);
    let warning = &success.warnings()[0];
    assert_eq!(warning.album_handle, "stupid-dream");
    assert_eq!(warning.resolved_directory, canonical(&album));
    assert_eq!(warning.contributing_selectors.len(), 2);
    assert_eq!(
        warning.contributing_selectors[0].as_os_str(),
        OsStr::new("Stupid Dream")
    );
    assert_eq!(
        warning.contributing_selectors[1].as_os_str(),
        OsStr::new("Stupid Dream")
    );
    let scopes = success.inventory().covered_scopes();
    assert_eq!(scopes.len(), 1);
    match &scopes[0] {
        RequiredScope::ConfiguredDirectory {
            contributing_selectors,
            ..
        } => {
            assert_eq!(contributing_selectors, &warning.contributing_selectors);
        }
        other => panic!("expected configured scope, got {other:?}"),
    }
}

#[test]
fn preparation_failure_retains_configured_default_and_warning_without_inventory() {
    let sandbox = TempSandbox::new();
    let existing = sandbox.path().join("existing");
    fs::create_dir(&existing).expect("create existing");
    fs::write(existing.join("track.dat"), b"track").expect("write track");
    let missing_one = sandbox.path().join("missing-one");
    let missing_two = sandbox.path().join("missing-two");
    let missing_root = sandbox.path().join("missing-root");
    let spec = parse_spec(&format!(
        "[albums.good]\ndirectories = [{}, {}]\n\
         [albums.bad-one]\ndirectory = {}\n\
         [albums.bad-two]\ndirectory = {}\n\
         [albums.dependent]\nname = \"Dependent\"\n",
        toml_string(&existing),
        toml_string(&existing),
        toml_string(&missing_one),
        toml_string(&missing_two),
    ));

    let snapshot = SourceTreeSnapshot::capture(sandbox.path());
    let result = build_candidate_inventory(&spec, &missing_root);
    snapshot.assert_unchanged();
    let failure = result.expect_err("preparation must fail");

    match &failure {
        CandidateInventoryFailure::Preparation {
            configured_failures,
            default_source_root_failure,
            warnings,
        } => {
            assert_eq!(
                configured_failures.len(),
                2,
                "both missing selectors stay separate"
            );
            assert_eq!(configured_failures[0].album_handle, "bad-one");
            assert_eq!(
                configured_failures[0].configured_selector.as_os_str(),
                missing_one.as_os_str()
            );
            assert_eq!(configured_failures[1].album_handle, "bad-two");
            assert_eq!(
                configured_failures[1].configured_selector.as_os_str(),
                missing_two.as_os_str()
            );
            let default = default_source_root_failure
                .as_ref()
                .expect("dependent album must fail on the missing root");
            assert_eq!(
                default.original_source_root.as_os_str(),
                missing_root.as_os_str()
            );
            assert_eq!(default.dependent_album_handles, ["dependent"]);
            assert_eq!(warnings.len(), 1, "successful group still warns on failure");
            assert_eq!(warnings[0].album_handle, "good");
            assert_eq!(warnings[0].resolved_directory, canonical(&existing));
            assert_eq!(warnings[0].contributing_selectors.len(), 2);
            assert_eq!(
                warnings[0].contributing_selectors[0].as_os_str(),
                existing.as_os_str()
            );
            assert_eq!(failure.warnings().len(), 1);
        }
        other => panic!("expected preparation failure, got {other:?}"),
    }
    assert_eq!(failure.warnings().len(), 1);
    assert!(
        !format!("{failure}").is_empty(),
        "Display must describe the failure"
    );
    let as_error: &dyn std::error::Error = &failure;
    assert!(!format!("{as_error}").is_empty());
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;

    #[test]
    fn non_utf8_file_name_stays_native_candidate() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let album = source.join("Album");
        fs::create_dir(&album).expect("create album");
        let raw = vec![
            0x66, 0x69, 0x6c, 0x65, 0x2d, 0xff, 0xfe, 0x2e, 0x64, 0x61, 0x74,
        ];
        let name = OsString::from_vec(raw.clone());
        fs::write(album.join(&name), b"bytes").expect("write non-utf8 file");
        let spec = parse_spec("[albums.tool]\ndirectory = \"Album\"\n");

        let snapshot = SourceTreeSnapshot::capture(&source);
        let result = build_candidate_inventory(&spec, &source);
        snapshot.assert_unchanged();
        let success = result.expect("non-utf8 tree must succeed");

        let expected = canonical(&album).join(&name);
        let candidate = success
            .inventory()
            .candidates()
            .find(|candidate| candidate.path() == expected.as_path())
            .expect("native non-utf8 pathname must stay a candidate");
        assert_eq!(
            candidate.path().as_os_str(),
            expected.as_os_str(),
            "native spelling must be retained without String conversion"
        );
        assert!(
            candidate.path().to_str().is_none(),
            "test precondition needs a non-utf8 name"
        );
    }

    #[test]
    fn symlink_root_resolves_while_descendant_symlinks_stay_skipped() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let real = source.join("real");
        fs::create_dir(&real).expect("create real");
        fs::write(real.join("track.dat"), b"track").expect("write track");
        let outside = sandbox.path().join("outside");
        fs::create_dir(&outside).expect("create outside");
        fs::write(outside.join("outside.dat"), b"outside").expect("write outside");
        symlink(&real, source.join("link")).expect("create selector symlink");
        symlink(outside.join("outside.dat"), real.join("linked-file.dat"))
            .expect("create descendant file symlink");
        symlink(&outside, real.join("linked-dir")).expect("create descendant dir symlink");
        let spec = parse_spec("[albums.tool]\ndirectory = \"link\"\n");

        let snapshot = SourceTreeSnapshot::capture(&source);
        let result = build_candidate_inventory(&spec, &source);
        snapshot.assert_unchanged();
        let success = result.expect("resolved symlink root must succeed");

        assert!(success.warnings().is_empty());
        let scopes = success.inventory().covered_scopes();
        assert_eq!(scopes.len(), 1);
        match &scopes[0] {
            RequiredScope::ConfiguredDirectory {
                contributing_selectors,
                resolved_directory,
                ..
            } => {
                assert_eq!(resolved_directory, &canonical(&real));
                assert_eq!(contributing_selectors.len(), 1);
                assert_eq!(contributing_selectors[0].as_os_str(), OsStr::new("link"));
            }
            other => panic!("expected configured scope, got {other:?}"),
        }
        let candidates: Vec<_> = success.inventory().candidates().collect();
        assert_eq!(candidates.len(), 1, "descendant symlinks stay skipped");
        assert_eq!(
            candidates[0].path(),
            canonical(&real).join("track.dat").as_path()
        );
    }
}
