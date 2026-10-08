/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Lexical, filepath-based recognition of Scarab's source-audio formats.
//!
//! [`classify_source_audio`] recognizes a supplied path by its final extension.
//! The path is not required to have come from discovery or inventory.
//!
//! Classification does not establish existence, ordinary-file status, readability,
//! valid media, probe success, collection membership, logical-track identity, or output selection.
//!
//! The classifier performs no filesystem, process, or configuration access and
//! never canonicalizes or rewrites paths.

use std::ffi::OsStr;
use std::path::Path;

/// A source-audio format recognized by lexical pathname inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SourceAudioFormat {
    Flac,
}

/// Recognizes a Scarab-supported source-audio format from a pathname's
/// final extension. Compares ASCII-case-insensitively.
///
/// The path is never searched as a substring. Only the final extension is
/// examined, using [`Path::extension`], so a bare dotfile such
/// as `.flac` has no extension and returns [`None`], while `.hidden.flac`
/// returns [`SourceAudioFormat::Flac`]. A non-UTF-8 basename with an ordinary
/// ASCII extension remains eligible. Existence, file type, contents, and
/// metadata are never inspected, so even a nonexistent `missing.flac` is
/// recognized purely from its extension.
pub fn classify_source_audio(path: &Path) -> Option<SourceAudioFormat> {
    let extension = path.extension()?;
    is_flac_extension(extension).then_some(SourceAudioFormat::Flac)
}

/// Compares an extension against `flac` ASCII-case-insensitively without
/// requiring the extension to be valid UTF-8.
fn is_flac_extension(extension: &OsStr) -> bool {
    extension.eq_ignore_ascii_case("flac")
}

#[cfg(test)]
#[path = "tests/source_audio.rs"]
mod tests;
