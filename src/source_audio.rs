/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Filepath-based recognition of source-audio candidates among ordinary files.
//!
//! [`classify_source_audio`] inspects a path's final extension and
//! reports whether the pathname names a supported source-audio candidate.
//! A `Some<SourceAudioFormat>` result means the pathname is a supported candidate
//! for later `ffprobe` probing.
//!
//! The classifier performs no filesystem, process, or configuration
//! access of any kind, never canonicalizes or rewrites paths,
//! and does not enforce that its input came from discovery, therefore
//! it is unable to certify that the file exists, contains audio,
//! or is readable. These are steps for future stages of the pipeline.

use std::ffi::OsStr;
use std::path::Path;

/// A source-audio format recognized by lexical pathname inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceAudioFormat {
    Flac,
}

/// Recognizes a pathname as a supported source-audio candidate. Compares
/// ASCII-case-insensitively.
///
/// The path is never searched as a substring. Only the final extension is
/// examined, using [`Path::extension`], so a bare dotfile such
/// as `.flac` has no extension and returns [`None`], while `.hidden.flac`
/// returns [`SourceAudioFormat::Flac`]. A non-UTF-8 basename with an ordinary
/// ASCII extension remains eligible. Existence, file type, contents, and
/// metadata are never inspected, so a nonexistent `missing.flac` is still a
/// candidate.
pub fn classify_source_audio(path: &Path) -> Option<SourceAudioFormat> {
    let extension = path.extension()?;
    is_flac_extension(extension).then_some(SourceAudioFormat::Flac)
}

/// Compares an extension against `flac` ASCII-case-insensitively without
/// requiring the extension to be valid UTF-8.
fn is_flac_extension(extension: &OsStr) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        extension.as_bytes().eq_ignore_ascii_case(b"flac")
    }
    // non-unix branch is unverified at runtime
    // let me know if this doesn't work, please :)
    #[cfg(not(unix))]
    {
        // A non-UTF-8 extension cannot equal the ASCII spelling `flac`
        // on this platform, so treating it as unsupported matches the classifier's needs.
        extension
            .to_str()
            .is_some_and(|text| text.eq_ignore_ascii_case("flac"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_case_insensitive_flac_extensions() {
        for name in ["track.flac", "track.FLAC", "track.FlaC", "track.fLaC"] {
            assert_eq!(
                classify_source_audio(Path::new(name)),
                Some(SourceAudioFormat::Flac),
                "expected {name} to be recognized"
            );
        }
    }

    #[test]
    fn rejects_unsupported_audio_extension() {
        assert_eq!(classify_source_audio(Path::new("track.wav")), None);
        assert_eq!(classify_source_audio(Path::new("track.mp3")), None);
        assert_eq!(classify_source_audio(Path::new("track.aiff")), None);
    }

    #[test]
    fn rejects_non_audio_extensions() {
        assert_eq!(classify_source_audio(Path::new("cover.jpg")), None);
        assert_eq!(classify_source_audio(Path::new("notes.txt")), None);
        assert_eq!(classify_source_audio(Path::new("booklet.pdf")), None);
    }

    #[test]
    fn rejects_appended_extension_suffix() {
        assert_eq!(classify_source_audio(Path::new("track.flac.bak")), None);
        assert_eq!(classify_source_audio(Path::new("take1.flac.orig")), None);
    }

    #[test]
    fn rejects_extensionless_filename() {
        assert_eq!(classify_source_audio(Path::new("extensionless")), None);
    }

    #[test]
    fn rejects_unsupported_child_of_flac_named_directory() {
        assert_eq!(
            classify_source_audio(Path::new("tracks.flac/notes.txt")),
            None
        );
        assert_eq!(classify_source_audio(Path::new("tracks.flac/cover")), None);
    }

    #[test]
    fn rejects_bare_dotfile_flac() {
        assert_eq!(classify_source_audio(Path::new(".flac")), None);
    }

    #[test]
    fn accepts_hidden_dotfile_with_flac_extension() {
        assert_eq!(
            classify_source_audio(Path::new(".hidden.flac")),
            Some(SourceAudioFormat::Flac)
        );
    }

    #[test]
    fn rejects_empty_path() {
        assert_eq!(classify_source_audio(Path::new("")), None);
    }

    #[test]
    fn rejects_trailing_dot_filename() {
        assert_eq!(classify_source_audio(Path::new("track.")), None);
    }

    #[test]
    fn accepts_multidot_filename_with_final_flac_extension() {
        assert_eq!(
            classify_source_audio(Path::new("artist.album.disc1.master.flac")),
            Some(SourceAudioFormat::Flac)
        );
    }

    /// This test is to pass because probing should flag this file as
    /// non-existent later in the pipeline.
    #[test]
    fn accepts_nonexistent_flac_named_path() {
        assert_eq!(
            classify_source_audio(Path::new("does/not/exist/missing.flac")),
            Some(SourceAudioFormat::Flac)
        );
    }

    // Non-UTF-8 path construction is a Unix-specific capability of OsString,
    // so these classifier cases compile only on Unix platforms. All portable
    // classifier tests above remain available everywhere.
    #[cfg(unix)]
    mod unix {
        use super::*;
        use std::borrow::Cow;
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        use std::path::PathBuf;

        fn non_utf8_os_string(bytes: &[u8]) -> OsString {
            OsString::from_vec(bytes.to_vec())
        }

        fn non_utf8_path(bytes: &[u8]) -> Cow<'_, Path> {
            Cow::Owned(PathBuf::from(non_utf8_os_string(bytes)))
        }

        #[test]
        fn accepts_non_utf8_basename_with_flac_extension() {
            let path = non_utf8_path(b"/music/\xFF\xFEbad-name.flac");
            assert_eq!(classify_source_audio(&path), Some(SourceAudioFormat::Flac));
        }

        #[test]
        fn rejects_non_utf8_extension() {
            let path = non_utf8_path(b"/music/track.\xFFlaC");
            assert_eq!(classify_source_audio(&path), None);
        }
    }
}
