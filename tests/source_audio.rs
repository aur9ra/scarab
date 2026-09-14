/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Integration tests for source-audio candidate classification.
//!
//! Because the classifier operates on a lexical basis, these tests do not
//! need to touch the filesystem; therefore no real media, fixture copying,
//! or ffprobe is required.

use std::path::Path;

use scarab::{SourceAudioFormat, classify_source_audio};

#[test]
fn public_api_recognizes_supported_and_unsupported_candidates() {
    assert_eq!(
        classify_source_audio(Path::new("album/01 - Track.flac")),
        Some(SourceAudioFormat::Flac)
    );
    assert_eq!(
        classify_source_audio(Path::new("album/01 - Track.FLAC")),
        Some(SourceAudioFormat::Flac)
    );
    assert_eq!(
        classify_source_audio(Path::new("album/.hidden.flac")),
        Some(SourceAudioFormat::Flac)
    );
    assert_eq!(classify_source_audio(Path::new("album/cover.jpg")), None);
    assert_eq!(classify_source_audio(Path::new("album/notes.txt")), None);
    assert_eq!(
        classify_source_audio(Path::new("album/track.flac.bak")),
        None
    );
    assert_eq!(classify_source_audio(Path::new("album/.flac")), None);
    assert_eq!(classify_source_audio(Path::new("album/track.")), None);
    assert_eq!(classify_source_audio(Path::new("")), None);
}
