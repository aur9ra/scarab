/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Resolution of one explicit configured album-directory selector.
//!
//! [`resolve_album_directory`] interprets one configured selector against one
//! source root, resolves the effective pathname through the native filesystem,
//! and verifies that a subsequent metadata lookup reports it as a directory.
//!
//! The primitive is deliberately narrow. It does not iterate album
//! declarations, accumulate failures across declarations, default a scope for
//! an undeclared selector, associate candidate files, interpret metadata, or
//! decide album membership. It reports only that canonicalization produced a
//! pathname and that a subsequent metadata lookup through that pathname
//! reported a directory. Continuing object identity, readability, containment
//! within the source root, and later pathname stability are not established.
//!
//! The configured selector, its effective pathname, and the resolved pathname
//! are distinct. Only the configured input and the resolved output cross the
//! API boundary. An absolute configured selector is used as spelled and
//! ignores the source root. A relative selector is anchored to the source root
//! and resolved through native platform semantics; on Windows, only selectors
//! and anchors with supported structural shapes are admitted.

use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

#[cfg(windows)]
use std::path::{Component, Prefix};

/// Resolves one configured album-directory selector against one
/// `source_root`.
///
/// A nonempty absolute `configured_directory` is resolved as spelled and never
/// inspects `source_root`.
/// A nonempty relative selector is anchored to `source_root` and resolved through
/// native platform pathname semantics, including symlinks and `..` components,
/// without Scarab-side normalization.
///
/// On Windows, a relative selector must be source-root-relative
/// and `source_root` must be a supported anchor.
///
/// On success, returns the pathname produced by [`fs::canonicalize`] after
/// confirming that [`fs::metadata`] reports it as a directory.
///
/// Returns [`io::Error`] unchanged for canonicalization and metadata errors.
/// A successfully inspected target that is not a directory is reported as
/// [`io::ErrorKind::NotADirectory`]. An empty selector and unsupported Windows
/// structural forms are reported as [`io::ErrorKind::InvalidInput`].
///
/// Successful resolution does not establish containment within `source_root`,
/// continuing object identity, readability, or later pathname stability.
pub fn resolve_album_directory(
    source_root: &Path,
    configured_directory: &Path,
) -> io::Result<PathBuf> {
    let effective_path = effective_album_directory(source_root, configured_directory)?;
    let resolved_path = fs::canonicalize(&effective_path)?;
    let metadata = fs::metadata(&resolved_path)?;

    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotADirectory,
            "resolved target is not a directory",
        ));
    }

    Ok(resolved_path)
}

/// Builds the effective pathname for one configured selector without touching
/// the filesystem.
///
/// Absolute configured directories never inspect the source root.
/// Relative configured directories are joined to the source root.
fn effective_album_directory(
    source_root: &Path,
    configured_directory: &Path,
) -> io::Result<PathBuf> {
    if configured_directory.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "album directory selector is empty",
        ));
    }

    // An absolute selector (already checked for emptiness) is fully
    // self-describing, and bypasses anchor policy entirely.
    if configured_directory.is_absolute() {
        return Ok(configured_directory.to_path_buf());
    }

    // Windows distinguishes rooted, prefixed, and plain relative forms.
    // Elsewhere every non-absolute selector is source-root-relative and native
    // resolution applies to the joined result.
    #[cfg(windows)]
    {
        if configured_directory.has_root() || component_prefix(configured_directory).is_some() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "album directory selector is not source-root-relative",
            ));
        }

        if !is_supported_anchor(source_root) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "source root is not a supported anchor for a relative album directory selector",
            ));
        }
    }

    Ok(source_root.join(configured_directory))
}

#[cfg(windows)]
/// The parsed prefix of `path`'s first component, if it has one.
fn component_prefix(path: &Path) -> Option<Prefix<'_>> {
    match path.components().next() {
        Some(Component::Prefix(prefix)) => Some(prefix.kind()),
        _ => None,
    }
}

#[cfg(windows)]
/// Whether `source_root` can anchor a relative selector.
///
/// A verbatim prefix is never accepted. A non-verbatim prefix is accepted only
/// when the source root is fully absolute. A prefixless path is accepted only
/// when it has no root. An ordinary relative source root remains accepted and
/// therefore is dependent on the process CWD.
fn is_supported_anchor(source_root: &Path) -> bool {
    match component_prefix(source_root) {
        Some(prefix) => !prefix.is_verbatim() && source_root.is_absolute(),
        None => !source_root.has_root(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::ffi::OsString;

    const EMPTY_SELECTOR_MESSAGE: &str = "album directory selector is empty";

    /// The raw spelling of `selector` appended to a nonempty `source_root`
    /// using the host separator, independent of `Path::join`.
    fn joined_spelling(source_root: &str, selector: &str) -> OsString {
        if source_root.is_empty() {
            selector.into()
        } else {
            format!("{source_root}{}{selector}", std::path::MAIN_SEPARATOR).into()
        }
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

    #[test]
    fn empty_configured_selector_is_rejected_before_anchor_policy() {
        // The anchors include shapes that a relative selector would not admit
        // on Windows. The empty selector must be rejected first either way.
        let source_roots = [
            Path::new(""),
            Path::new("ordinary/root"),
            Path::new(r"C:"),
            Path::new(r"\rooted"),
        ];

        for source_root in source_roots {
            let error = effective_album_directory(source_root, Path::new(""))
                .expect_err("empty selector must be rejected");
            assert_synthetic_error(&error, io::ErrorKind::InvalidInput, EMPTY_SELECTOR_MESSAGE);
        }
    }

    #[test]
    fn ordinary_relative_selector_is_joined_to_supplied_anchor() {
        let effective = effective_album_directory(
            Path::new("relative/root"),
            Path::new("Porcupine Tree/Fear of a Blank Planet"),
        )
        .expect("ordinary relative selector must be admitted");

        assert_eq!(
            effective.as_os_str(),
            joined_spelling("relative/root", "Porcupine Tree/Fear of a Blank Planet").as_os_str()
        );
    }

    #[test]
    fn ordinary_relative_source_root_is_accepted() {
        let effective = effective_album_directory(Path::new("music/root"), Path::new("Lateralus"))
            .expect("ordinary relative source root must be admitted");

        assert_eq!(
            effective.as_os_str(),
            joined_spelling("music/root", "Lateralus").as_os_str()
        );
    }

    #[test]
    fn empty_source_root_leaves_relative_selector_relative() {
        let effective = effective_album_directory(Path::new(""), Path::new("Ænima"))
            .expect("empty source root must be admitted");

        assert_eq!(effective.as_os_str(), OsStr::new("Ænima"));
    }

    #[test]
    fn explicit_dot_components_survive_effective_path_construction() {
        let selectors = [
            ".",
            "..",
            "Porcupine Tree/./Fear of a Blank Planet",
            "Porcupine Tree/../Tool",
        ];

        for selector in selectors {
            let effective = effective_album_directory(Path::new("root"), Path::new(selector))
                .unwrap_or_else(|error| panic!("selector {selector:?} must be admitted: {error}"));
            assert_eq!(
                effective.as_os_str(),
                joined_spelling("root", selector).as_os_str(),
                "selector {selector:?} spelling changed"
            );
        }
    }

    #[test]
    fn whitespace_only_selector_is_admitted_and_preserved() {
        let effective = effective_album_directory(Path::new("root"), Path::new("   "))
            .expect("whitespace-only selector must not be treated as empty");

        assert_eq!(
            effective.as_os_str(),
            joined_spelling("root", "   ").as_os_str()
        );
    }

    #[test]
    fn host_absolute_selector_is_returned_unchanged_and_ignores_anchor() {
        #[cfg(unix)]
        let absolute = "/absolute/selector";
        #[cfg(windows)]
        let absolute = r"C:\absolute\selector";

        let selector = Path::new(absolute);
        assert!(
            selector.is_absolute(),
            "test precondition: {absolute:?} must be host-absolute"
        );

        // One anchor is empty and one is an ordinary relative path on Unix;
        // on Windows the backslash spelling is root-relative and would be
        // rejected for a relative selector. An absolute selector must ignore
        // the anchor completely.
        for source_root in [Path::new(""), Path::new(r"\root-relative")] {
            let effective = effective_album_directory(source_root, selector)
                .expect("absolute selector must be admitted");
            assert_eq!(effective.as_os_str(), selector.as_os_str());
        }
    }

    #[cfg(windows)]
    mod windows {
        use super::super::effective_album_directory;
        use super::assert_synthetic_error;
        use super::joined_spelling;
        use std::ffi::OsStr;
        use std::io;
        use std::path::Component;
        use std::path::Path;
        use std::path::Prefix;

        const SELECTOR_MESSAGE: &str = "album directory selector is not source-root-relative";
        const ANCHOR_MESSAGE: &str =
            "source root is not a supported anchor for a relative album directory selector";

        /// The parsed prefix of `path`'s first component, from native parsing.
        fn native_prefix(path: &Path) -> Option<Prefix<'_>> {
            match path.components().next() {
                Some(Component::Prefix(prefix)) => Some(prefix.kind()),
                _ => None,
            }
        }

        fn prefix_category(prefix: Prefix<'_>) -> &'static str {
            match prefix {
                Prefix::Verbatim(_) => "Verbatim",
                Prefix::VerbatimUNC(..) => "VerbatimUNC",
                Prefix::VerbatimDisk(_) => "VerbatimDisk",
                Prefix::DeviceNS(_) => "DeviceNS",
                Prefix::UNC(..) => "UNC",
                Prefix::Disk(_) => "Disk",
            }
        }

        fn assert_selector_rejected(source_root: &Path, selector: &Path) {
            let error = effective_album_directory(source_root, selector)
                .expect_err("unsupported configured selector must be rejected");
            assert_synthetic_error(&error, io::ErrorKind::InvalidInput, SELECTOR_MESSAGE);
        }

        fn assert_anchor_rejected(source_root: &Path) {
            let error = effective_album_directory(source_root, Path::new("Lateralus"))
                .expect_err("unsupported source-root anchor must be rejected");
            assert_synthetic_error(&error, io::ErrorKind::InvalidInput, ANCHOR_MESSAGE);
        }

        #[test]
        fn partially_qualified_configured_selectors_are_rejected() {
            let cases: [(&str, bool, bool); 4] = [
                (r"\foo", true, false),
                ("/foo", true, false),
                ("C:foo", false, true),
                ("C:", false, true),
            ];

            for (literal, expected_root, expected_prefix) in cases {
                let selector = Path::new(literal);
                assert!(!selector.is_absolute(), "{literal:?} must be nonabsolute");
                assert_eq!(selector.has_root(), expected_root, "root of {literal:?}");
                assert_eq!(
                    native_prefix(selector).is_some(),
                    expected_prefix,
                    "prefix of {literal:?}"
                );

                assert_selector_rejected(Path::new("ordinary"), selector);
            }
        }

        #[test]
        fn partially_qualified_source_roots_are_rejected_for_relative_selectors() {
            let cases: [(&str, bool, bool); 4] = [
                (r"\foo", true, false),
                ("/foo", true, false),
                ("C:foo", false, true),
                ("C:", false, true),
            ];

            for (literal, expected_root, expected_prefix) in cases {
                let source_root = Path::new(literal);
                assert!(
                    !source_root.is_absolute(),
                    "{literal:?} must be nonabsolute"
                );
                assert_eq!(source_root.has_root(), expected_root, "root of {literal:?}");
                assert_eq!(
                    native_prefix(source_root).is_some(),
                    expected_prefix,
                    "prefix of {literal:?}"
                );

                assert_anchor_rejected(source_root);
            }
        }

        #[test]
        fn selector_rejection_precedes_anchor_rejection() {
            // Both forms are unsupported in each pair. The selector error must
            // win so callers can learn about the configured selector first.
            let pairs = [(r"C:", r"C:foo"), (r"\rooted", r"\foo")];

            for (anchor, selector) in pairs {
                let error = effective_album_directory(Path::new(anchor), Path::new(selector))
                    .expect_err("both the selector and the anchor are unsupported");
                assert_synthetic_error(&error, io::ErrorKind::InvalidInput, SELECTOR_MESSAGE);
            }
        }

        #[test]
        fn supported_source_root_anchors_accept_ordinary_relative_selectors() {
            let cases: [(&str, Option<Prefix<'_>>); 6] = [
                ("", None),
                ("music", None),
                (r"C:\Music", Some(Prefix::Disk(b'C'))),
                ("C:/Music", Some(Prefix::Disk(b'C'))),
                (
                    r"\\server\share\Music",
                    Some(Prefix::UNC(OsStr::new("server"), OsStr::new("share"))),
                ),
                (r"\\.\C:\Music", Some(Prefix::DeviceNS(OsStr::new("C:")))),
            ];

            for (literal, expected_prefix) in cases {
                let source_root = Path::new(literal);
                assert_eq!(
                    native_prefix(source_root),
                    expected_prefix,
                    "prefix of {literal:?}"
                );
                match native_prefix(source_root) {
                    Some(prefix) => {
                        assert!(!prefix.is_verbatim(), "{literal:?} must not be verbatim");
                        assert!(source_root.is_absolute(), "{literal:?} must be absolute");
                    }
                    None => assert!(!source_root.has_root(), "{literal:?} must have no root"),
                }

                let effective = effective_album_directory(source_root, Path::new("Lateralus"))
                    .unwrap_or_else(|error| panic!("{literal:?} must anchor: {error}"));
                assert_eq!(
                    effective.as_os_str(),
                    joined_spelling(literal, "Lateralus").as_os_str(),
                    "anchor {literal:?} spelling changed"
                );
            }
        }

        #[test]
        fn verbatim_source_root_anchors_are_rejected() {
            let cases: [(&str, &str); 3] = [
                (r"\\?\C:\Music", "VerbatimDisk"),
                (r"\\?\UNC\server\share\Music", "VerbatimUNC"),
                (r"\\?\cat_pics\Music", "Verbatim"),
            ];

            for (literal, category) in cases {
                let source_root = Path::new(literal);
                let prefix = native_prefix(source_root)
                    .unwrap_or_else(|| panic!("{literal:?} must have a prefix"));
                assert_eq!(prefix_category(prefix), category, "category of {literal:?}");
                assert!(prefix.is_verbatim(), "{literal:?} must be verbatim");

                assert_anchor_rejected(source_root);
            }
        }

        #[test]
        fn absolute_configured_selectors_bypass_source_root_policy() {
            let cases: [(&str, &str); 7] = [
                (r"C:\Music", "Disk"),
                ("C:/Music", "Disk"),
                (r"\\server\share\Music", "UNC"),
                (r"\\?\C:\Music", "VerbatimDisk"),
                (r"\\?\UNC\server\share\Music", "VerbatimUNC"),
                (r"\\?\cat_pics\Music", "Verbatim"),
                (r"\\.\C:\Music", "DeviceNS"),
            ];

            // `C:` is rejected as an anchor for a relative selector. Every
            // absolute selector must bypass it and keep its raw spelling.
            let rejected_anchor = Path::new("C:");
            assert_anchor_rejected(rejected_anchor);

            for (literal, category) in cases {
                let selector = Path::new(literal);
                assert!(selector.is_absolute(), "{literal:?} must be absolute");
                let prefix = native_prefix(selector)
                    .unwrap_or_else(|| panic!("{literal:?} must have a prefix"));
                assert_eq!(prefix_category(prefix), category, "category of {literal:?}");

                let effective = effective_album_directory(rejected_anchor, selector)
                    .unwrap_or_else(|error| panic!("{literal:?} must be admitted: {error}"));
                assert_eq!(
                    effective.as_os_str(),
                    selector.as_os_str(),
                    "{literal:?} spelling changed"
                );
            }
        }

        #[test]
        fn verbatim_disk_without_root_follows_native_classification() {
            let selector = Path::new(r"\\?\C:");
            let prefix = native_prefix(selector)
                .unwrap_or_else(|| panic!("{selector:?} must have a prefix"));
            assert_eq!(prefix_category(prefix), "VerbatimDisk");
            assert!(
                selector.has_root(),
                "native classification treats any prefix other than `Prefix::Disk` as implicitly rooted"
            );

            // Native `is_absolute` is `has_root && prefix`, so `\\?\C:` is
            // absolute even though ordinary `C:` is not. The helper must admit
            // it unchanged rather than reject it.
            assert!(
                selector.is_absolute(),
                "test precondition from native classification"
            );
            let effective = effective_album_directory(Path::new("C:"), selector)
                .expect("absolute selector must be admitted");
            assert_eq!(effective.as_os_str(), selector.as_os_str());

            // Ordinary `C:` remains rejected as a configured selector.
            assert_selector_rejected(Path::new("ordinary"), Path::new("C:"));
        }
    }
}
