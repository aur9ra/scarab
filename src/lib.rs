/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Parsing and validation of Scarab's TOML configuration, filesystem discovery
//! and scope preparation, observed source file inventory, source-audio classification,
//! ffprobe metadata probing, and directory-selector resolution.
//!
//! [`parse`] deserializes TOML text into the validated [`LibraryBuildSpec`]
//! model or reports the first problem it finds. Ambiguity, such as a rule
//! applied twice, is rejected instead of being decided by declaration order.
//!
//! Collection declarations define membership criteria. The `album_names`,
//! `album_artists`, and `track_artists` fields hold selectors from their
//! singular or plural TOML keys. The `directory` and `directories` keys select
//! source paths.
//!
//! [`discover_source_files`] recursively collects ordinary files under one
//! source root. [`resolve_directory_selector`] resolves one configured
//! directory selector against a source root and verifies its target is a
//! directory. The private `collection_scope` module prepares filesystem scopes on behalf of
//! collection declarations.
//! Declarations without directory selectors share one default source-root scope,
//! if any. `required_discovery` scans prepared scopes, and
//! [`build_observed_source_file_inventory`]
//! combines scope preparation with required discovery into a completed observed
//! source file inventory. The private `collection_membership` module derives
//! directory-only membership from that inventory for declarations without
//! metadata selectors.
//!
//! [`classify_source_audio`] recognizes Scarab's source-audio formats from a
//! supplied path's final extension. [`probe_source_file`] requests ffprobe
//! format metadata for its supplied path and requires no prior classification.
//! Discovery, classification, and probing are distinct capabilities. None
//! establishes collection membership.

mod collection_membership;
mod collection_scope;
mod config;
mod directory_selector;
mod discovery;
mod metadata_selection;
mod observed_source_file_inventory;
mod probe;
mod required_discovery;
mod source_audio;
#[cfg(test)]
mod test_support;
mod vorbis_comments;

pub use config::{
    Codec, CollectionDeclaration, CollectionRule, EncodingProfile, FilesConfig,
    InvalidLibraryBuildSpec, LibraryBuildSpec, LibraryBuildSpecError, SizeMode, TrackAction,
    TrackRule, TrackRuleGroup, TrackTarget, parse,
};
pub use directory_selector::resolve_directory_selector;
pub use discovery::{DiscoveryError, discover_source_files};
pub use observed_source_file_inventory::{
    ConfiguredSelectorFailure, DefaultSourceRootFailure, DefaultSourceRootFailureKind,
    ObservedSourceFile, ObservedSourceFileInventory, ObservedSourceFileInventoryFailure,
    ObservedSourceFileInventorySuccess, RedundantConfiguredSelectors, RequiredScope,
    RequiredScopeDiscoveryFailure, build_observed_source_file_inventory,
};
pub use probe::{ProbeError, ProbedSourceFile, probe_source_file};
pub use source_audio::{SourceAudioFormat, classify_source_audio};
