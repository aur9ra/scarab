/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Deterministic parse and validation rules for Scarab's TOML configuration.
//!
//! Tests are split by configuration-level concern into modules under
//! `tests/config/`: whole-document/global configuration (`global.rs`),
//! `[files]`, `[albums.<handle>]`, `[[album_rules]]`, and `[[track_rules]]`.
//!
//! The modules are wired with explicit `#[path]` attributes because a plain
//! child declaration such as `mod albums;` from this crate root would resolve
//! beside the root under `tests/`, not under `tests/config/`.

#[path = "config/album_rules.rs"]
mod album_rules;

#[path = "config/albums.rs"]
mod albums;

#[path = "config/files.rs"]
mod files;

#[path = "config/global.rs"]
mod global;

#[path = "config/track_rules.rs"]
mod track_rules;

use scarab::{InvalidLibraryBuildSpec, LibraryBuildSpec, LibraryBuildSpecError, parse};

/// Valid top-level keys followed by one declared album. Top-level keys must
/// precede tables in TOML, so tests append only tables to this prefix.
const PREFIX: &str =
    "codec = \"opus\"\nbitrate = 128\n[albums.aenima]\nname = \"Ænima\"\nartist = \"Tool\"\n";

fn prefixed(body: &str) -> String {
    format!("{PREFIX}{body}")
}

fn valid(text: &str) -> LibraryBuildSpec {
    match parse(text) {
        Ok(config) => config,
        Err(error) => panic!("expected valid configuration, got: {error}"),
    }
}

fn invalid(text: &str) -> InvalidLibraryBuildSpec {
    match parse(text) {
        Err(LibraryBuildSpecError::Invalid(error)) => error,
        Err(LibraryBuildSpecError::Toml(error)) => {
            panic!("expected validation error, got TOML error: {error}")
        }
        Ok(config) => panic!("expected validation error, got: {config:?}"),
    }
}

fn rejects_toml(text: &str) {
    match parse(text) {
        Err(LibraryBuildSpecError::Toml(_)) => {}
        Err(other) => panic!("expected TOML error, got: {other}"),
        Ok(config) => panic!("expected TOML error, got: {config:?}"),
    }
}

/// One standalone album declaration on top of the minimal global config.
fn album_config(handle: &str, body: &str) -> String {
    format!("codec = \"opus\"\nbitrate = 128\n[albums.{handle}]\n{body}")
}
