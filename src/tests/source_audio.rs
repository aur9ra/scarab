/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

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

/// Recognition depends on the final extension, not whether the path exists.
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
