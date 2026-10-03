/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Parsing and validation of Scarab's TOML configuration, filesystem discovery
//! and scope preparation, candidate inventory, source-audio classification,
//! ffprobe metadata probing, and album-directory resolution.
//!
//! [`parse`] deserializes TOML text into the validated [`LibraryBuildSpec`]
//! model or reports the first problem it finds. Ambiguity, such as a rule
//! applied twice, is rejected instead of being decided by declaration order.
//!
//! Albums may declare metadata selectors and optional directory selectors.
//!
//! [`discover_source_files`] recursively collects ordinary files under one
//! source root. [`resolve_album_directory`] resolves one configured directory
//! selector against a source root and verifies its target is a directory. The
//! private `album_scope` module prepares configured directory-selector scopes.
//! Declarations without directory selectors share one default source-root scope,
//! if any. `required_discovery` scans prepared scopes, and [`build_candidate_inventory`]
//! combines scope preparation with required discovery into an inventory of
//! observed filesystem pathnames. The private `collection_membership` module
//! derives directory-only membership from a completed inventory for
//! declarations without metadata selectors.
//!
//! [`classify_source_audio`] recognizes Scarab's source-audio formats from a
//! supplied path's final extension. [`probe_source_file`] requests ffprobe
//! format metadata for its supplied path and requires no prior classification.
//! Discovery, classification, and probing are distinct capabilities. None
//! establishes album membership.

mod album_directory;
mod album_scope;
mod candidate_inventory;
mod collection_membership;
mod config;
mod discovery;
mod probe;
mod required_discovery;
mod source_audio;
#[cfg(test)]
mod test_support;

pub use album_directory::resolve_album_directory;
pub use candidate_inventory::{
    Candidate, CandidateInventory, CandidateInventoryFailure, CandidateInventorySuccess,
    ConfiguredSelectorFailure, DefaultSourceRootFailure, DefaultSourceRootFailureKind,
    RedundantConfiguredSelectors, RequiredScope, RequiredScopeDiscoveryFailure,
    build_candidate_inventory,
};
pub use config::{
    Album, AlbumRule, Codec, EncodingProfile, Files, InvalidLibraryBuildSpec, LibraryBuildSpec,
    LibraryBuildSpecError, SizeMode, TrackAction, TrackRule, TrackRuleGroup, TrackTarget, parse,
};
pub use discovery::{DiscoveryError, discover_source_files};
pub use probe::{ProbeError, ProbedSourceFile, probe_source_file};
pub use source_audio::{SourceAudioFormat, classify_source_audio};
