/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Parsing and validation of Scarab's TOML configuration.

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

use indexmap::IndexMap;
use serde::Deserialize;

/// Parses and validates a TOML library configuration document.
///
/// TOML syntax and shape errors are returned as [`LibraryBuildSpecError::Toml`].
/// Well-formed documents that violate a cross-field rule are returned as
/// [`LibraryBuildSpecError::Invalid`]. When several conflicts exist, the first in
/// validation order is reported.
pub fn parse(text: &str) -> Result<LibraryBuildSpec, LibraryBuildSpecError> {
    toml::from_str::<RawLibraryBuildSpec>(text)
        .map_err(LibraryBuildSpecError::Toml)?
        .validate()
}

/// The only codec supported by the prototype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    Opus,
}

/// Opus encoding profile, defaulting to `music` when omitted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncodingProfile {
    #[default]
    Music,
    None,
}

/// Global output size mode with exactly one of the two forms configured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SizeMode {
    /// Constant bitrate in kbps as configured by `bitrate`.
    Bitrate(u32),
    /// Target size exactly as configured by `target_size`. Sizing strings
    /// are not interpreted yet.
    TargetSize(String),
}

/// Optional selection for the output library.
///
/// `include` and `exclude` may both be configured. Selection semantics are
/// not applied by this parser.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Files {
    /// Copy a standalone front cover when one is found.
    pub album_art: Option<bool>,
    /// File extensions to include.
    pub include: Option<Vec<String>>,
    /// File extensions to exclude.
    pub exclude: Option<Vec<String>>,
}

/// Optional selectors for one declared album.
///
/// `name` and `artist` are optional, independent metadata predicates.
/// Supplied values are preserved exactly.
///
/// `directories` is the collapsed filesystem candidate scope from the
/// `directory` or `directories` configuration fields. `None` means no
/// filesystem scope was configured, in which case future resolution uses
/// the caller-supplied default scope. Configured directories describe
/// recursive candidate scopes and are preserved exactly, including
/// order, duplicates, and relative/absolute spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Album {
    pub name: Option<String>,
    pub artist: Option<String>,
    pub directories: Option<Vec<PathBuf>>,
}

/// Raw, pre-validation deserialization target for one `[albums.<handle>]`
/// table.
///
/// Keeps the separate singular `directory` and plural `directories` fields
/// separate until validation collapses them into [`Album::directories`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAlbum {
    name: Option<String>,
    artist: Option<String>,
    directory: Option<String>,
    directories: Option<Vec<String>>,
}

/// A bitrate override applied to every track of one or more declared albums.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlbumRule {
    #[serde(rename = "albums")]
    pub album_handles: Vec<String>,
    pub bitrate: u32,
}

/// A group of track overrides for one declared album.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRuleGroup {
    pub album_handle: String,
    pub rules: Vec<TrackRule>,
}

/// A TrackRule is either one or multiple tracks,
/// along with an action ([`TrackAction`]) to perform
/// on said tracks ([`TrackTarget`]).
///
/// The track names a rule applies to are found in its [`TrackTarget`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRule {
    pub target: TrackTarget,
    pub action: TrackAction,
}

/// The track names a rule applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackTarget {
    /// A single track name.
    Track(String),
    /// A list of track names. [`parse`] guarantees the list is non-empty.
    /// Independently constructed `TrackTarget`s do not make this guarantee.
    Tracks(Vec<String>),
}

/// The effect to be applied to a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackAction {
    Exclude,
    Bitrate(u32),
}

/// A complete library build configuration produced by [`parse`].
///
/// The only public construction path from untrusted config input is
/// [`parse`], which rejects documents violating config-level
/// rules.
///
/// This does not certify filesystem resources, album membership,
/// metadata matching, interpreted target-size semantics, or deferred
/// file-selection policy.
///
/// Nested values remain freely constructible representations,
/// and are not necessarily by type certified configuration
/// fragments. Operations relying on config validation must
/// retain parsed origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryBuildSpec {
    codec: Codec,
    encoding_profile: EncodingProfile,
    size_mode: SizeMode,
    files: Files,
    /// Albums keyed by their configuration handles.
    albums: IndexMap<String, Album>,
    album_rules: Vec<AlbumRule>,
    track_rules: Vec<TrackRuleGroup>,
}

impl LibraryBuildSpec {
    /// The configured codec.
    pub fn codec(&self) -> Codec {
        self.codec
    }

    /// The configured encoding profile.
    pub fn encoding_profile(&self) -> EncodingProfile {
        self.encoding_profile
    }

    /// The configured global output size mode.
    pub fn size_mode(&self) -> &SizeMode {
        &self.size_mode
    }

    /// The configured file selection.
    pub fn files(&self) -> &Files {
        &self.files
    }

    /// Albums keyed by their configuration handles.
    ///
    /// [`parse`] preserves album declaration order. A handle's position
    /// is established by its first introduction in the document, and
    /// later additions to an album do not change its position within
    /// this order.
    pub fn albums(&self) -> &IndexMap<String, Album> {
        &self.albums
    }

    /// The configured album rules, in document order.
    pub fn album_rules(&self) -> &[AlbumRule] {
        &self.album_rules
    }

    /// The configured track rule groups, in document order.
    pub fn track_rules(&self) -> &[TrackRuleGroup] {
        &self.track_rules
    }
}

/// A failure to parse or validate a configuration document.
#[derive(Debug)]
pub enum LibraryBuildSpecError {
    /// The text is not valid TOML or does not match the configuration schema shape.
    Toml(toml::de::Error),
    /// The document is well-formed but violates a configuration-internal rule.
    Invalid(InvalidLibraryBuildSpec),
}

impl fmt::Display for LibraryBuildSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryBuildSpecError::Toml(error) => write!(f, "invalid TOML configuration: {error}"),
            LibraryBuildSpecError::Invalid(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for LibraryBuildSpecError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LibraryBuildSpecError::Toml(error) => Some(error),
            LibraryBuildSpecError::Invalid(error) => Some(error),
        }
    }
}

impl From<InvalidLibraryBuildSpec> for LibraryBuildSpecError {
    fn from(error: InvalidLibraryBuildSpec) -> Self {
        LibraryBuildSpecError::Invalid(error)
    }
}

/// Configuration-internal rules enforced by [`parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidLibraryBuildSpec {
    /// Neither `bitrate` nor `target_size` was configured.
    MissingSizeMode,
    /// Both `bitrate` and `target_size` were configured.
    ConflictingSizeMode,
    /// An `[albums.<handle>]` table configured none of `name`, `artist`,
    /// `directory`, or `directories`.
    MissingAlbumSelector { album_handle: String },
    /// An album configured both `directory` and `directories`.
    ConflictingAlbumPaths { album_handle: String },
    /// An album configured an empty `directory` path.
    EmptyAlbumDirectory { album_handle: String },
    /// An album configured an empty `directories` list.
    EmptyAlbumDirectories { album_handle: String },
    /// An album configured an empty entry in its `directories` list.
    EmptyAlbumDirectoriesEntry { album_handle: String, index: usize },
    /// A rule referenced an album handle with no `[albums.<handle>]` entry.
    UnknownAlbum { album_handle: String },
    /// An album rule configured an empty `albums` list.
    EmptyAlbums,
    /// More than one album rule targeted the same album, including twice
    /// within one rule.
    DuplicateAlbumRule { album_handle: String },
    /// More than one track rule targeted the same track of the same album.
    DuplicateTrackRule {
        album_handle: String,
        track_name: String,
    },
    /// An explicit `[[track_rules]]` group configured no nested rules.
    EmptyTrackRules { album_handle: String },
    /// A track rule set both `track` and `tracks`, or neither.
    TrackTargetNotExclusive { album_handle: String },
    /// A track rule configured an empty `tracks` list.
    EmptyTracks { album_handle: String },
    /// A track rule set both `exclude = true` and `bitrate`, or neither.
    TrackActionNotExclusive { album_handle: String },
}

impl std::error::Error for InvalidLibraryBuildSpec {}

impl fmt::Display for InvalidLibraryBuildSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidLibraryBuildSpec::MissingSizeMode => {
                write!(f, "exactly one of bitrate or target_size must be set")
            }
            InvalidLibraryBuildSpec::ConflictingSizeMode => {
                write!(f, "bitrate and target_size are mutually exclusive")
            }
            InvalidLibraryBuildSpec::MissingAlbumSelector { album_handle } => {
                write!(
                    f,
                    "album `{album_handle}` must set at least one of name, artist, directory, or directories"
                )
            }
            InvalidLibraryBuildSpec::ConflictingAlbumPaths { album_handle } => {
                write!(
                    f,
                    "album `{album_handle}` must set at most one of directory or directories"
                )
            }
            InvalidLibraryBuildSpec::EmptyAlbumDirectory { album_handle } => {
                write!(f, "album `{album_handle}` has an empty directory path")
            }
            InvalidLibraryBuildSpec::EmptyAlbumDirectories { album_handle } => {
                write!(f, "album `{album_handle}` has an empty directories list")
            }
            InvalidLibraryBuildSpec::EmptyAlbumDirectoriesEntry {
                album_handle,
                index,
            } => {
                write!(
                    f,
                    "album `{album_handle}` has an empty directories entry at index {index}"
                )
            }
            InvalidLibraryBuildSpec::UnknownAlbum { album_handle } => {
                write!(
                    f,
                    "rule references undeclared album handle `{album_handle}`"
                )
            }
            InvalidLibraryBuildSpec::EmptyAlbums => {
                write!(f, "album rule must list at least one album handle")
            }
            InvalidLibraryBuildSpec::DuplicateAlbumRule { album_handle } => {
                write!(
                    f,
                    "album `{album_handle}` receives more than one album rule"
                )
            }
            InvalidLibraryBuildSpec::DuplicateTrackRule {
                album_handle,
                track_name,
            } => {
                write!(
                    f,
                    "track `{track_name}` in album `{album_handle}` has more than one rule"
                )
            }
            InvalidLibraryBuildSpec::EmptyTrackRules { album_handle } => {
                write!(
                    f,
                    "track rule group for `{album_handle}` has no nested rules"
                )
            }
            InvalidLibraryBuildSpec::TrackTargetNotExclusive { album_handle } => {
                write!(
                    f,
                    "track rule for `{album_handle}` must set exactly one of track or tracks"
                )
            }
            InvalidLibraryBuildSpec::EmptyTracks { album_handle } => {
                write!(
                    f,
                    "track rule for `{album_handle}` has an empty tracks list"
                )
            }
            InvalidLibraryBuildSpec::TrackActionNotExclusive { album_handle } => {
                write!(
                    f,
                    "track rule for `{album_handle}` must set exactly one of exclude or bitrate"
                )
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLibraryBuildSpec {
    codec: Codec,
    #[serde(default)]
    encoding_profile: EncodingProfile,
    bitrate: Option<u32>,
    target_size: Option<String>,
    #[serde(default)]
    files: Files,
    /// Ordered by first introduction of each handle so that
    /// the validated map preserves declaration order.
    #[serde(default)]
    albums: IndexMap<String, RawAlbum>,
    #[serde(default)]
    album_rules: Vec<AlbumRule>,
    #[serde(default)]
    track_rules: Vec<RawTrackRuleGroup>,
}

/// A TrackRuleGroup is an album, as well as a TrackRule:
/// either one or multiple tracks, along with an action
/// to perform on said track(s).
///
/// Raw, pre-validation, mid-deserialization struct.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTrackRuleGroup {
    #[serde(rename = "album")]
    album_handle: String,
    #[serde(default)]
    rules: Vec<RawTrackRule>,
}

/// A TrackRule is either one or multiple tracks, along with
/// an action to perform on said tracks.
///
/// Raw, pre-validation, mid-deserialization struct.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTrackRule {
    track: Option<String>,
    tracks: Option<Vec<String>>,
    exclude: Option<bool>,
    bitrate: Option<u32>,
}

impl RawLibraryBuildSpec {
    fn validate(self) -> Result<LibraryBuildSpec, LibraryBuildSpecError> {
        let RawLibraryBuildSpec {
            codec,
            encoding_profile,
            bitrate,
            target_size,
            files,
            albums,
            album_rules,
            track_rules,
        } = self;

        let size_mode = match (bitrate, target_size) {
            (Some(bitrate), None) => SizeMode::Bitrate(bitrate),
            (None, Some(target_size)) => SizeMode::TargetSize(target_size),
            (Some(_), Some(_)) => return Err(InvalidLibraryBuildSpec::ConflictingSizeMode.into()),
            (None, None) => return Err(InvalidLibraryBuildSpec::MissingSizeMode.into()),
        };

        // Validate each album declaration before checking rules that
        // reference handles, so a malformed declaration is reported first.
        let mut validated_albums = IndexMap::new();
        for (album_handle, raw_album) in albums {
            let album = raw_album.into_album(&album_handle)?;
            validated_albums.insert(album_handle, album);
        }

        // Use a new BTreeSet to determine uniqueness
        let mut rule_checked_album_handles = BTreeSet::new();
        for rule in &album_rules {
            if rule.album_handles.is_empty() {
                return Err(InvalidLibraryBuildSpec::EmptyAlbums.into());
            }
            for album_handle in &rule.album_handles {
                check_album_handle_defined(&validated_albums, album_handle)?;
                // false means duplicate
                if !rule_checked_album_handles.insert(album_handle.as_str()) {
                    return Err(InvalidLibraryBuildSpec::DuplicateAlbumRule {
                        album_handle: album_handle.clone(),
                    }
                    .into());
                }
            }
        }

        // Once again, use a new BTreeSet to determine uniqueness
        let mut rule_checked_tracks: BTreeSet<(String, String)> = BTreeSet::new();
        let mut validated_groups = Vec::with_capacity(track_rules.len());
        for group in track_rules {
            check_album_handle_defined(&validated_albums, &group.album_handle)?;
            if group.rules.is_empty() {
                return Err(InvalidLibraryBuildSpec::EmptyTrackRules {
                    album_handle: group.album_handle,
                }
                .into());
            }
            let RawTrackRuleGroup {
                album_handle,
                rules,
            } = group;
            let mut validated_rules: Vec<TrackRule> = Vec::with_capacity(rules.len());
            for raw_rule in rules {
                let rule: TrackRule = raw_rule.into_rule(&album_handle)?;
                for track_name in rule.target.track_names() {
                    let key = (album_handle.clone(), track_name.to_owned());
                    // false means duplicate
                    if !rule_checked_tracks.insert(key) {
                        return Err(InvalidLibraryBuildSpec::DuplicateTrackRule {
                            album_handle: album_handle.clone(),
                            track_name: track_name.to_owned(),
                        }
                        .into());
                    }
                }
                validated_rules.push(rule);
            }
            validated_groups.push(TrackRuleGroup {
                album_handle,
                rules: validated_rules,
            });
        }

        Ok(LibraryBuildSpec {
            codec,
            encoding_profile,
            size_mode,
            files,
            albums: validated_albums,
            album_rules,
            track_rules: validated_groups,
        })
    }
}

impl RawAlbum {
    /// Collapses one raw album declaration into its validated form.
    ///
    /// Metadata predicates are preserved exactly, including empty and
    /// whitespace-only values. The singular `directory` and plural
    /// `directories` forms collapse into one ordered collection, and no
    /// successful parse ever produces an empty collection.
    fn into_album(self, album_handle: &str) -> Result<Album, LibraryBuildSpecError> {
        let RawAlbum {
            name,
            artist,
            directory,
            directories,
        } = self;

        let has_selector =
            name.is_some() || artist.is_some() || directory.is_some() || directories.is_some();

        if !has_selector {
            return Err(InvalidLibraryBuildSpec::MissingAlbumSelector {
                album_handle: album_handle.to_owned(),
            }
            .into());
        }

        let directories = match (directory, directories) {
            (None, None) => None,
            (Some(directory), None) => {
                // An empty decoded string is exactly an empty path
                if directory.is_empty() {
                    return Err(InvalidLibraryBuildSpec::EmptyAlbumDirectory {
                        album_handle: album_handle.to_owned(),
                    }
                    .into());
                }
                Some(vec![PathBuf::from(directory)])
            }
            (None, Some(directories)) => {
                if directories.is_empty() {
                    return Err(InvalidLibraryBuildSpec::EmptyAlbumDirectories {
                        album_handle: album_handle.to_owned(),
                    }
                    .into());
                }
                if let Some(index) = directories.iter().position(String::is_empty) {
                    return Err(InvalidLibraryBuildSpec::EmptyAlbumDirectoriesEntry {
                        album_handle: album_handle.to_owned(),
                        index,
                    }
                    .into());
                }
                Some(directories.into_iter().map(PathBuf::from).collect())
            }
            (Some(_), Some(_)) => {
                return Err(InvalidLibraryBuildSpec::ConflictingAlbumPaths {
                    album_handle: album_handle.to_owned(),
                }
                .into());
            }
        };

        Ok(Album {
            name,
            artist,
            directories,
        })
    }
}

impl TrackTarget {
    /// Iterates the track names this target applies to.
    fn track_names(&self) -> std::slice::Iter<'_, String> {
        match self {
            TrackTarget::Track(track_name) => std::slice::from_ref(track_name).iter(),
            TrackTarget::Tracks(track_names) => track_names.iter(),
        }
    }
}

impl RawTrackRule {
    fn into_rule(self, album_handle: &str) -> Result<TrackRule, LibraryBuildSpecError> {
        let RawTrackRule {
            track,
            tracks,
            exclude,
            bitrate,
        } = self;

        let target = match (track, tracks) {
            (Some(track), None) => TrackTarget::Track(track),
            (None, Some(tracks)) if !tracks.is_empty() => TrackTarget::Tracks(tracks),
            (None, Some(_)) => {
                return Err(InvalidLibraryBuildSpec::EmptyTracks {
                    album_handle: album_handle.to_owned(),
                }
                .into());
            }
            _ => {
                return Err(InvalidLibraryBuildSpec::TrackTargetNotExclusive {
                    album_handle: album_handle.to_owned(),
                }
                .into());
            }
        };

        let action = match (exclude, bitrate) {
            (Some(true), None) => TrackAction::Exclude,
            (None, Some(bitrate)) => TrackAction::Bitrate(bitrate),
            _ => {
                return Err(InvalidLibraryBuildSpec::TrackActionNotExclusive {
                    album_handle: album_handle.to_owned(),
                }
                .into());
            }
        };

        Ok(TrackRule { target, action })
    }
}

fn check_album_handle_defined(
    albums: &IndexMap<String, Album>,
    album_handle: &str,
) -> Result<(), LibraryBuildSpecError> {
    if albums.contains_key(album_handle) {
        Ok(())
    } else {
        Err(InvalidLibraryBuildSpec::UnknownAlbum {
            album_handle: album_handle.to_owned(),
        }
        .into())
    }
}
