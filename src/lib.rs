//! Parsing and validation of Scarab's TOML configuration.
//!
//! [`parse`] is the entry point. It deserializes TOML text into the
//! validated [`LibraryBuildSpec`] model or reports the first problem it finds. Ambiguity,
//! such as a rule applied twice, is rejected instead of being decided by
//! declaration order.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

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

/// Identity metadata for one declared album.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Album {
    pub name: String,
    pub artist: String,
}

/// A bitrate override applied to every track of one or more declared albums.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlbumRule {
    pub albums: Vec<String>,
    pub bitrate: u32,
}

/// A group of track overrides for one declared album.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRuleGroup {
    pub album: String,
    pub rules: Vec<TrackRule>,
}

/// A TrackRule is either one or multiple tracks,
/// along with an action ([`TrackAction`]) to perform
/// on said tracks ([`TrackTarget`]).
///
/// Access TrackTarget names via [`TrackTarget::names`]
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
    /// A non-empty list of track names.
    Tracks(Vec<String>),
}

/// The effect to be applied to a track.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackAction {
    Exclude,
    Bitrate(u32),
}

/// A complete, internally consistent configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryBuildSpec {
    pub codec: Codec,
    pub encoding_profile: EncodingProfile,
    pub size_mode: SizeMode,
    pub files: Files,
    pub albums: BTreeMap<String, Album>,
    pub album_rules: Vec<AlbumRule>,
    pub track_rules: Vec<TrackRuleGroup>,
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
    /// A rule referenced an album handle with no `[albums.<handle>]` entry.
    UnknownAlbum { handle: String },
    /// An album rule configured an empty `albums` list.
    EmptyAlbums,
    /// More than one album rule targeted the same album, including twice
    /// within one rule.
    DuplicateAlbumRule { handle: String },
    /// More than one track rule targeted the same track of the same album.
    DuplicateTrackRule { album: String, track: String },
    /// An explicit `[[track_rules]]` group configured no nested rules.
    EmptyTrackRules { album: String },
    /// A track rule set both `track` and `tracks`, or neither.
    TrackTargetNotExclusive { album: String },
    /// A track rule configured an empty `tracks` list.
    EmptyTracks { album: String },
    /// A track rule set both `exclude = true` and `bitrate`, or neither.
    TrackActionNotExclusive { album: String },
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
            InvalidLibraryBuildSpec::UnknownAlbum { handle } => {
                write!(f, "rule references undeclared album handle `{handle}`")
            }
            InvalidLibraryBuildSpec::EmptyAlbums => {
                write!(f, "album rule must list at least one album handle")
            }
            InvalidLibraryBuildSpec::DuplicateAlbumRule { handle } => {
                write!(f, "album `{handle}` receives more than one album rule")
            }
            InvalidLibraryBuildSpec::DuplicateTrackRule { album, track } => {
                write!(
                    f,
                    "track `{track}` in album `{album}` has more than one rule"
                )
            }
            InvalidLibraryBuildSpec::EmptyTrackRules { album } => {
                write!(f, "track rule group for `{album}` has no nested rules")
            }
            InvalidLibraryBuildSpec::TrackTargetNotExclusive { album } => {
                write!(
                    f,
                    "track rule for `{album}` must set exactly one of track or tracks"
                )
            }
            InvalidLibraryBuildSpec::EmptyTracks { album } => {
                write!(f, "track rule for `{album}` has an empty tracks list")
            }
            InvalidLibraryBuildSpec::TrackActionNotExclusive { album } => {
                write!(
                    f,
                    "track rule for `{album}` must set exactly one of exclude or bitrate"
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
    #[serde(default)]
    albums: BTreeMap<String, Album>,
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
    album: String,
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

        // Use a new BTreeSet to determine uniqueness
        let mut rule_checked_albums = BTreeSet::new();
        for rule in &album_rules {
            if rule.albums.is_empty() {
                return Err(InvalidLibraryBuildSpec::EmptyAlbums.into());
            }
            for album in &rule.albums {
                check_declared(&albums, album)?;
                // false means duplicate
                if !rule_checked_albums.insert(album.as_str()) {
                    return Err(InvalidLibraryBuildSpec::DuplicateAlbumRule {
                        handle: album.clone(),
                    }
                    .into());
                }
            }
        }

        // Once again, use a new BTreeSet to determine uniqueness
        let mut rule_checked_tracks: BTreeSet<(String, String)> = BTreeSet::new();
        let mut validated_groups = Vec::with_capacity(track_rules.len());
        for group in track_rules {
            check_declared(&albums, &group.album)?;
            if group.rules.is_empty() {
                return Err(InvalidLibraryBuildSpec::EmptyTrackRules { album: group.album }.into());
            }
            let RawTrackRuleGroup { album, rules } = group;
            let mut validated_rules: Vec<TrackRule> = Vec::with_capacity(rules.len());
            for raw in rules {
                let rule: TrackRule = raw.into_rule(&album)?;
                for track in rule.target.names() {
                    let track = track.as_str();
                    let key = (album.clone(), track.to_owned());
                    // false means duplicate
                    if !rule_checked_tracks.insert(key) {
                        return Err(InvalidLibraryBuildSpec::DuplicateTrackRule {
                            album: album.clone(),
                            track: track.to_owned(),
                        }
                        .into());
                    }
                }
                validated_rules.push(rule);
            }
            validated_groups.push(TrackRuleGroup {
                album,
                rules: validated_rules,
            });
        }

        Ok(LibraryBuildSpec {
            codec,
            encoding_profile,
            size_mode,
            files,
            albums,
            album_rules,
            track_rules: validated_groups,
        })
    }
}

impl TrackTarget {
    /// Iterates the names this target applies to.
    fn names(&self) -> std::slice::Iter<'_, String> {
        match self {
            TrackTarget::Track(track) => std::slice::from_ref(track).iter(),
            TrackTarget::Tracks(tracks) => tracks.iter(),
        }
    }
}

impl RawTrackRule {
    fn into_rule(self, album: &str) -> Result<TrackRule, LibraryBuildSpecError> {
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
                    album: album.to_owned(),
                }
                .into());
            }
            _ => {
                return Err(InvalidLibraryBuildSpec::TrackTargetNotExclusive {
                    album: album.to_owned(),
                }
                .into());
            }
        };

        let action = match (exclude, bitrate) {
            (Some(true), None) => TrackAction::Exclude,
            (None, Some(bitrate)) => TrackAction::Bitrate(bitrate),
            _ => {
                return Err(InvalidLibraryBuildSpec::TrackActionNotExclusive {
                    album: album.to_owned(),
                }
                .into());
            }
        };

        Ok(TrackRule { target, action })
    }
}

fn check_declared(
    albums: &BTreeMap<String, Album>,
    handle: &str,
) -> Result<(), LibraryBuildSpecError> {
    if albums.contains_key(handle) {
        Ok(())
    } else {
        Err(InvalidLibraryBuildSpec::UnknownAlbum {
            handle: handle.to_owned(),
        }
        .into())
    }
}
