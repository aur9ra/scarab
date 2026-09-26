/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Parsing and validation of Scarab's TOML configuration, discovery of
//! source files under a source root, recognition of source-audio candidates
//! among source files, probing of source audio files with ffprobe, and
//! resolution of one explicit configured album-directory selector.
//!
//! [`parse`] deserializes TOML text into the validated [`LibraryBuildSpec`]
//! model or reports the first problem it finds. Ambiguity, such as a rule
//! applied twice, is rejected instead of being decided by declaration order.
//!
//! Albums declare optional metadata selectors and an optional
//! filesystem candidate scope.
//!
//! [`discover_source_files`] recursively collects every ordinary file under
//! one source root. [`classify_source_audio`] recognizes whether
//! one supplied pathname names a supported source-audio format by its final
//! extension. [`probe_track`] describes one supplied source file by
//! invoking ffprobe. [`resolve_album_directory`] resolves one configured
//! album directory against one source root and returns its resolved path after
//! confirming it is a directory. The private `album_scope` module applies
//! this resolver to configured selectors and prepares the shared default root
//! for albums without selectors. `required_discovery` scans each prepared
//! scope, and [`build_candidate_inventory`] combines both steps into a
//! filesystem candidate inventory.
//!
//! Discovery, candidate recognition, and probing are not yet connected into a
//! single pipeline.

mod album_directory;
mod album_scope;
mod candidate_inventory;
mod config;
mod discovery;
mod probe;
mod required_discovery;
mod source_audio;

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
pub use probe::{ProbeError, ProbedTrack, probe_track};
pub use source_audio::{SourceAudioFormat, classify_source_audio};
