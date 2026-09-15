/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Integration tests for explicit configured album-directory resolution.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use scarab::resolve_album_directory;

mod common;

use common::{SourceTreeSnapshot, TempSandbox};

const EMPTY_SELECTOR_MESSAGE: &str = "album directory selector is empty";
const NOT_A_DIRECTORY_MESSAGE: &str = "resolved target is not a directory";

/// Resolves one album-directory selector while asserting the enclosing
/// sandbox is untouched.
///
/// The resolver result is not inspected until the immutability check has
/// completed.
fn resolve_album_directory_and_assert_unchanged(
    sandbox: &TempSandbox,
    source_root: &Path,
    configured_directory: &Path,
) -> io::Result<PathBuf> {
    let snapshot = SourceTreeSnapshot::capture(sandbox.path());
    let result = resolve_album_directory(source_root, configured_directory);
    snapshot.assert_unchanged();
    result
}

fn assert_synthetic_error(error: &io::Error, kind: io::ErrorKind, message: &str) {
    assert_eq!(error.kind(), kind, "unexpected ErrorKind");
    assert_eq!(error.to_string(), message, "unexpected message");
    assert_eq!(
        error.raw_os_error(),
        None,
        "synthetic error carries no OS code"
    );
}

/// Canonicalizes `path`.
fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|error| {
        panic!(
            "expected target {} must canonicalize: {error}",
            path.display()
        )
    })
}

/// Asserts that `error` is the native canonicalization failure independently
/// reported for `effective`, comparing (for `error`) kind and raw OS code only.
fn assert_native_canonicalize_error(error: &io::Error, effective: &Path) {
    let expected = fs::canonicalize(effective)
        .expect_err("independent canonicalization of the known-failing path must fail");
    assert_eq!(error.kind(), expected.kind(), "native canonicalize kind");
    assert_eq!(
        error.raw_os_error(),
        expected.raw_os_error(),
        "native canonicalize raw OS code"
    );
}

/// Creates an temporary Sandbox with `sentinel.txt` and return the PathBuf to the sandbox.
fn create_source(sandbox: &TempSandbox) -> PathBuf {
    let source = sandbox.path().join("source");
    fs::create_dir(&source).expect("create source root");
    fs::write(source.join("sentinel.txt"), b"sentinel").expect("write sentinel");
    source
}

#[test]
fn relative_selector_naming_existing_directory_resolves() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Lateralus");
    fs::create_dir(&album).expect("create album");

    let resolved =
        resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new("Lateralus"))
            .expect("relative existing directory must resolve");
    assert_eq!(resolved, canonical(&album));
}

#[test]
fn explicit_current_directory_selector_resolves_to_source_root() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);

    let resolved = resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new("."))
        .expect("explicit `.` must resolve");
    assert_eq!(resolved, canonical(&source));
}

#[test]
fn parent_selector_resolves_a_sibling_outside_the_source_root() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let sibling = sandbox.path().join("sibling");
    fs::create_dir(&sibling).expect("create sibling");
    fs::write(sibling.join("sibling-sentinel.txt"), b"sibling").expect("write sibling sentinel");

    let resolved =
        resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new("../sibling"))
            .expect("`../sibling` must resolve");
    assert_eq!(resolved, canonical(&sibling));
}

#[test]
fn absolute_selector_resolves_existing_directory() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let target = sandbox.path().join("absolute-target");
    fs::create_dir(&target).expect("create target");

    let resolved = resolve_album_directory_and_assert_unchanged(&sandbox, &source, &target)
        .expect("absolute existing directory must resolve");
    assert_eq!(resolved, canonical(&target));
}

#[test]
fn absolute_selector_ignores_nonexistent_source_root() {
    let sandbox = TempSandbox::new();
    let missing_source = sandbox.path().join("missing-source");
    let target = sandbox.path().join("absolute-target");
    fs::create_dir(&target).expect("create target");
    fs::write(target.join("sentinel.txt"), b"target sentinel").expect("write sentinel");

    let resolved = resolve_album_directory_and_assert_unchanged(&sandbox, &missing_source, &target)
        .expect("absolute selector must not depend on the source root");
    assert_eq!(resolved, canonical(&target));
}

#[test]
fn absolute_selector_ignores_an_ordinary_file_source_root() {
    let sandbox = TempSandbox::new();
    let file_source = sandbox.path().join("source-file");
    fs::write(&file_source, b"not a directory").expect("write source file");
    let target = sandbox.path().join("absolute-target");
    fs::create_dir(&target).expect("create target");
    fs::write(target.join("sentinel.txt"), b"target sentinel").expect("write sentinel");

    let resolved = resolve_album_directory_and_assert_unchanged(&sandbox, &file_source, &target)
        .expect("absolute selector must not depend on the source root");
    assert_eq!(resolved, canonical(&target));
}

#[test]
fn directory_name_with_ordinary_spaces_resolves() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let album = source.join("Fear of a Blank Planet");
    fs::create_dir(&album).expect("create spaced album");

    let resolved = resolve_album_directory_and_assert_unchanged(
        &sandbox,
        &source,
        Path::new("Fear of a Blank Planet"),
    )
    .expect("ordinary spaces must be admitted");
    assert_eq!(resolved, canonical(&album));
}

#[test]
fn ordinary_file_terminal_target_is_synthetic_not_a_directory() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("Anesthetize.flac"), b"audio").expect("write track");

    let error = resolve_album_directory_and_assert_unchanged(
        &sandbox,
        &source,
        Path::new("Anesthetize.flac"),
    )
    .expect_err("ordinary file must not resolve as a directory");
    assert_synthetic_error(
        &error,
        io::ErrorKind::NotADirectory,
        NOT_A_DIRECTORY_MESSAGE,
    );
}

#[test]
fn missing_target_preserves_native_canonicalize_error() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    let effective = source.join("missing");

    let error =
        resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new("missing"))
            .expect_err("missing target must fail");
    assert_native_canonicalize_error(&error, &effective);
}

#[test]
fn non_directory_intermediate_component_preserves_native_canonicalize_error() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);
    fs::write(source.join("Anesthetize.flac"), b"audio").expect("write track");
    let effective = source.join("Anesthetize.flac/inner");

    let error = resolve_album_directory_and_assert_unchanged(
        &sandbox,
        &source,
        Path::new("Anesthetize.flac/inner"),
    )
    .expect_err("non-directory intermediate component must fail natively");
    assert_native_canonicalize_error(&error, &effective);
}

#[test]
fn empty_selector_through_public_api_is_synthetic_invalid_input() {
    let sandbox = TempSandbox::new();
    let source = create_source(&sandbox);

    let error = resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new(""))
        .expect_err("empty selector must be rejected");
    assert_synthetic_error(&error, io::ErrorKind::InvalidInput, EMPTY_SELECTOR_MESSAGE);
}

#[test]
fn empty_source_root_with_current_directory_selector_uses_process_cwd() {
    // No fixture is created in the test process cwd and the cwd is unchanged.
    let resolved = resolve_album_directory(Path::new(""), Path::new("."))
        .expect("`.` in the process cwd must resolve");
    assert_eq!(resolved, canonical(Path::new(".")));
}

#[test]
fn dot_source_root_with_current_directory_selector_uses_process_cwd() {
    let resolved = resolve_album_directory(Path::new("."), Path::new("."))
        .expect("`.` in the process cwd must resolve");
    assert_eq!(resolved, canonical(Path::new(".")));
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsString;
    use std::fs;
    use std::io;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;
    use std::path::Path;

    use super::{
        NOT_A_DIRECTORY_MESSAGE, TempSandbox, assert_native_canonicalize_error,
        assert_synthetic_error, canonical, create_source,
        resolve_album_directory_and_assert_unchanged,
    };

    #[test]
    fn intermediate_symlink_leading_to_directory_resolves() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let real = source.join("real-album");
        fs::create_dir_all(real.join("disc")).expect("create real album");
        fs::write(real.join("disc/sentinel.txt"), b"sentinel").expect("write sentinel");
        symlink(&real, source.join("album-link")).expect("create album symlink");

        let resolved = resolve_album_directory_and_assert_unchanged(
            &sandbox,
            &source,
            Path::new("album-link/disc"),
        )
        .expect("intermediate symlink must resolve");
        assert_eq!(resolved, canonical(&real.join("disc")));
    }

    #[test]
    fn terminal_symlink_to_directory_resolves() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let real = source.join("real-album");
        fs::create_dir(&real).expect("create real album");
        fs::write(real.join("sentinel.txt"), b"sentinel").expect("write sentinel");
        symlink(&real, source.join("album-link")).expect("create album symlink");

        let resolved = resolve_album_directory_and_assert_unchanged(
            &sandbox,
            &source,
            Path::new("album-link"),
        )
        .expect("terminal symlink must resolve");
        assert_eq!(resolved, canonical(&real));
    }

    #[test]
    fn terminal_symlink_spellings_with_separator_or_dot_resolve() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let real = source.join("real-album");
        fs::create_dir(&real).expect("create real album");
        fs::write(real.join("sentinel.txt"), b"sentinel").expect("write sentinel");
        symlink(&real, source.join("album-link")).expect("create album symlink");

        for spelling in ["album-link/", "album-link/."] {
            let resolved = resolve_album_directory_and_assert_unchanged(
                &sandbox,
                &source,
                Path::new(spelling),
            )
            .unwrap_or_else(|error| panic!("spelling {spelling:?} must resolve: {error}"));
            assert_eq!(resolved, canonical(&real), "spelling {spelling:?}");
        }
    }

    #[test]
    fn symlink_may_escape_the_selected_source_root_within_the_sandbox() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let sibling = sandbox.path().join("sibling");
        fs::create_dir(&sibling).expect("create sibling");
        fs::write(sibling.join("sentinel.txt"), b"sibling").expect("write sentinel");
        symlink(&sibling, source.join("escape-link")).expect("create escape symlink");

        let resolved = resolve_album_directory_and_assert_unchanged(
            &sandbox,
            &source,
            Path::new("escape-link"),
        )
        .expect("escaping symlink must resolve");
        assert_eq!(resolved, canonical(&sibling));
    }

    #[test]
    fn symlinked_source_root_anchors_an_ordinary_relative_selector() {
        let sandbox = TempSandbox::new();
        let real_root = sandbox.path().join("real-root");
        let album = real_root.join("Lateralus");
        fs::create_dir_all(&album).expect("create real root album");
        fs::write(album.join("sentinel.txt"), b"sentinel").expect("write sentinel");
        let link_root = sandbox.path().join("link-root");
        symlink(&real_root, &link_root).expect("create root symlink");

        let resolved = resolve_album_directory_and_assert_unchanged(
            &sandbox,
            &link_root,
            Path::new("Lateralus"),
        )
        .expect("symlinked source root must anchor");
        assert_eq!(resolved, canonical(&album));
    }

    #[test]
    fn relative_selector_through_symlink_and_parent_is_resolved_natively() {
        // `link/..` must resolve to `target-dir`, the parent of the symlink
        // target `target-dir/child`. A premature lexical `..` collapse would
        // instead select `source`.
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let target = source.join("target-dir");
        let child = target.join("child");
        fs::create_dir_all(&child).expect("create target and child");
        fs::write(target.join("sentinel.txt"), b"target").expect("write target sentinel");
        symlink(&child, source.join("link")).expect("create child symlink");

        let resolved =
            resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new("link/.."))
                .expect("symlink parent traversal must resolve");
        assert_eq!(resolved, canonical(&target));
        assert_ne!(
            resolved,
            canonical(&source),
            "resolution must not collapse the selector lexically"
        );
    }

    #[test]
    fn terminal_symlink_to_ordinary_file_is_synthetic_not_a_directory() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let file = source.join("Anesthetize.flac");
        fs::write(&file, b"audio").expect("write track");
        symlink(&file, source.join("track-link")).expect("create track symlink");

        let error = resolve_album_directory_and_assert_unchanged(
            &sandbox,
            &source,
            Path::new("track-link"),
        )
        .expect_err("symlink to a file must not resolve as a directory");
        assert_synthetic_error(
            &error,
            io::ErrorKind::NotADirectory,
            NOT_A_DIRECTORY_MESSAGE,
        );
    }

    #[test]
    fn broken_symlink_preserves_native_canonicalize_error() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        symlink(source.join("missing"), source.join("broken-link")).expect("create broken symlink");
        let effective = source.join("broken-link");

        let error = resolve_album_directory_and_assert_unchanged(
            &sandbox,
            &source,
            Path::new("broken-link"),
        )
        .expect_err("broken symlink must fail natively");
        assert_native_canonicalize_error(&error, &effective);
    }

    #[test]
    fn symlink_loop_preserves_native_canonicalize_error() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        symlink(source.join("loop-b"), source.join("loop-a")).expect("create loop-a");
        symlink(source.join("loop-a"), source.join("loop-b")).expect("create loop-b");
        let effective = source.join("loop-a");

        let error =
            resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new("loop-a"))
                .expect_err("symlink loop must fail natively");
        assert_native_canonicalize_error(&error, &effective);
    }

    #[test]
    fn non_utf8_directory_name_resolves() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let name = OsString::from_vec(b"album-\xff\xfe".to_vec());
        let album = source.join(&name);
        fs::create_dir(&album).expect("create non-UTF-8 album");
        fs::write(album.join("sentinel.txt"), b"sentinel").expect("write sentinel");

        let resolved =
            resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new(&name))
                .expect("non-UTF-8 directory name must resolve");
        assert_eq!(resolved, canonical(&album));
    }

    #[test]
    fn whitespace_only_directory_name_resolves() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let album = source.join("   ");
        fs::create_dir(&album).expect("create whitespace-named album");
        fs::write(album.join("sentinel.txt"), b"sentinel").expect("write sentinel");

        let resolved =
            resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new("   "))
                .expect("whitespace-only directory name must resolve");
        assert_eq!(resolved, canonical(&album));
    }

    #[test]
    fn windows_looking_names_are_ordinary_unix_names() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let names = [r"back\slash", r"\rooted", "C:drive-like"];

        for name in names {
            let album = source.join(name);
            fs::create_dir(&album).unwrap_or_else(|error| panic!("create {name:?}: {error}"));
            fs::write(album.join("sentinel.txt"), b"sentinel").expect("write sentinel");
        }

        for name in names {
            let resolved =
                resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new(name))
                    .unwrap_or_else(|error| {
                        panic!("{name:?} must resolve as a Unix name: {error}")
                    });
            assert_eq!(resolved, canonical(&source.join(name)), "{name:?}");
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::fs;
    use std::io;
    use std::path::{Component, Path};

    use super::{
        TempSandbox, assert_synthetic_error, canonical, create_source,
        resolve_album_directory_and_assert_unchanged,
    };

    const SELECTOR_MESSAGE: &str = "album directory selector is not source-root-relative";
    const ANCHOR_MESSAGE: &str =
        "source root is not a supported anchor for a relative album directory selector";

    #[test]
    fn supported_relative_selector_resolves_existing_directory() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let album = source.join("Lateralus");
        fs::create_dir(&album).expect("create album");
        fs::write(album.join("sentinel.txt"), b"sentinel").expect("write sentinel");

        let resolved =
            resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new("Lateralus"))
                .expect("ordinary relative anchoring must succeed");
        assert_eq!(resolved, canonical(&album));
    }

    #[test]
    fn unsupported_selector_forms_are_synthetic_invalid_input() {
        let sandbox = TempSandbox::new();
        let _source = create_source(&sandbox);

        for literal in [r"\foo", "/foo", "C:foo", "C:"] {
            let error = resolve_album_directory_and_assert_unchanged(
                &sandbox,
                Path::new("ordinary"),
                Path::new(literal),
            )
            .expect_err("unsupported selector must be rejected");
            assert_synthetic_error(&error, io::ErrorKind::InvalidInput, SELECTOR_MESSAGE);
        }
    }

    #[test]
    fn unsupported_source_root_anchors_are_synthetic_invalid_input() {
        let sandbox = TempSandbox::new();
        let _source = create_source(&sandbox);

        let rejected = [
            r"\foo",
            "/foo",
            "C:foo",
            "C:",
            r"\\?\C:\Music",
            r"\\?\UNC\server\share\Music",
            r"\\?\cat_pics\Music",
        ];

        for literal in rejected {
            let error = resolve_album_directory_and_assert_unchanged(
                &sandbox,
                Path::new(literal),
                Path::new("Lateralus"),
            )
            .expect_err("unsupported anchor must be rejected");
            assert_synthetic_error(&error, io::ErrorKind::InvalidInput, ANCHOR_MESSAGE);
        }
    }

    #[test]
    fn absolute_existing_directory_succeeds_despite_rejected_anchors() {
        let sandbox = TempSandbox::new();
        let target = sandbox.path().join("absolute-target");
        fs::create_dir(&target).expect("create target");
        fs::write(target.join("sentinel.txt"), b"sentinel").expect("write sentinel");

        let rejected_anchors = [
            r"\foo",
            "/foo",
            "C:foo",
            "C:",
            r"\\?\C:\Music",
            r"\\?\UNC\server\share\Music",
            r"\\?\cat_pics\Music",
        ];

        for anchor in rejected_anchors {
            let resolved =
                resolve_album_directory_and_assert_unchanged(&sandbox, Path::new(anchor), &target)
                    .unwrap_or_else(|error| {
                        panic!("absolute target with anchor {anchor:?}: {error}")
                    });
            assert_eq!(resolved, canonical(&target), "anchor {anchor:?}");
        }
    }

    #[test]
    fn canonicalized_output_is_an_absolute_selector_but_not_an_anchor() {
        let sandbox = TempSandbox::new();
        let source = create_source(&sandbox);
        let album = source.join("Lateralus");
        fs::create_dir(&album).expect("create album");
        fs::write(album.join("sentinel.txt"), b"sentinel").expect("write sentinel");

        let resolved =
            resolve_album_directory_and_assert_unchanged(&sandbox, &source, Path::new("Lateralus"))
                .expect("relative selector must resolve");
        assert_eq!(resolved, canonical(&album));
        assert!(resolved.is_absolute(), "canonical output must be absolute");

        // A real canonicalization output is admitted as an absolute selector.
        let again = resolve_album_directory_and_assert_unchanged(&sandbox, &source, &resolved)
            .expect("canonicalized output must be admitted as an absolute selector");
        assert_eq!(again, canonical(&album));

        // Windows canonicalization returns a verbatim `\\?\` pathname, which
        // is deliberately not a reusable source-root anchor for a relative
        // selector.
        let prefix = match resolved.components().next() {
            Some(Component::Prefix(prefix)) => prefix.kind(),
            _ => panic!("canonical output must have a prefix"),
        };
        assert!(prefix.is_verbatim(), "canonical output must be verbatim");

        let error = resolve_album_directory_and_assert_unchanged(
            &sandbox,
            &resolved,
            Path::new("Lateralus"),
        )
        .expect_err("verbatim resolved pathname must not anchor a relative selector");
        assert_synthetic_error(&error, io::ErrorKind::InvalidInput, ANCHOR_MESSAGE);
    }
}
