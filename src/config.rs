/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Parsing and validation of Scarab's TOML configuration.

use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;

use crate::metadata_selection::MetadataSelectorPredicate;
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
#[non_exhaustive]
pub enum Codec {
    Opus,
}

/// Opus encoding profile, defaulting to `music` when omitted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
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

/// Optional file-handling configuration for the output library.
///
/// `include` and `exclude` may both be configured. Selection semantics are
/// not applied by this parser.
///
/// Non-exhaustive to allow future fields. Downstream crates can access its
/// public fields but cannot use struct literals or exhaustive patterns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct FilesConfig {
    /// Copy a standalone front cover when one is found.
    pub album_art: Option<bool>,
    /// File extensions to include.
    pub include: Option<Vec<String>>,
    /// File extensions to exclude.
    pub exclude: Option<Vec<String>>,
}

/// Membership criteria for one declared collection.
///
/// Metadata selectors cover album names, album artists, and track artists.
/// Each family accepts either its singular TOML key (`album_name`,
/// `album_artist`, or `track_artist`) xor its plural key (`album_names`,
/// `album_artists`, or `track_artists`). A configured plural list must be
/// nonempty. If neither key in a family is set, that family is omitted and
/// imposes no constraint.
///
/// Configuration processing preserves the original selector strings. It does
/// not trim, normalize, split, filter, deduplicate, or case-fold them. Plural
/// alternative order is preserved. Alternatives combine by OR within a family.
/// Supplied families combine by AND. Matching removes all trailing U+0000s,
/// applies NFC normalization, then compares whole values with case-sensitive
/// equality. It does not change stored strings.
///
/// Validation checks selector shape, not whether metadata matches. Every
/// collection declaration must supply at least one metadata or directory selector.
///
/// `directories` stores the directory selector or selectors from the
/// `directory` or `directories` configuration fields. `None` means no directory
/// selector was configured, so scope preparation uses the caller-supplied
/// shared default source root. Configured selector values are preserved exactly,
/// including order, duplicates, and relative/absolute spelling.
///
/// This declaration contains criteria, not evaluated members. Only
/// [`LibraryBuildSpec`] carries aggregate validation guarantees. Detached or
/// modified declarations do not. A detached declaration may contain an empty
/// metadata list, which remains supplied and unsatisfiable when translated,
/// although parsing rejects empty plural arrays.
///
/// Non-exhaustive to allow future fields. Downstream crates can access its
/// public fields but cannot use struct literals or exhaustive patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CollectionDeclaration {
    pub album_names: Option<Vec<String>>,
    pub album_artists: Option<Vec<String>>,
    pub track_artists: Option<Vec<String>>,
    pub directories: Option<Vec<PathBuf>>,
}

impl CollectionDeclaration {
    /// Builds a compound predicate from this declaration's metadata selectors.
    ///
    /// The translation preserves `None`, empty lists, string contents, list
    /// order, and repetitions. It does not validate or normalize values,
    /// construct observations, evaluate matches, or access the filesystem.
    /// Directory selectors do not affect the result.
    // Staged until production metadata-selector integration.
    #[allow(dead_code)]
    pub(crate) fn metadata_selector_predicate(&self) -> MetadataSelectorPredicate {
        MetadataSelectorPredicate::new(
            self.album_names.clone(),
            self.album_artists.clone(),
            self.track_artists.clone(),
        )
    }
}

/// Raw, pre-validation deserialization target for one `[collections.<handle>]`
/// table.
///
/// Retains singular and plural TOML forms separately until validation rejects
/// conflicts and converts them to canonical fields in
/// [`CollectionDeclaration`].
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCollectionDeclaration {
    album_name: Option<String>,
    album_names: Option<Vec<String>>,
    album_artist: Option<String>,
    album_artists: Option<Vec<String>>,
    track_artist: Option<String>,
    track_artists: Option<Vec<String>>,
    directory: Option<String>,
    directories: Option<Vec<String>>,
}

/// A collection-level bitrate output-policy override targeting one or more
/// collection handles. It does not define membership.
///
/// Non-exhaustive to allow future fields. Downstream crates can access its
/// public fields but cannot use struct literals or exhaustive patterns.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct CollectionRule {
    #[serde(rename = "collections")]
    pub collection_handles: Vec<String>,
    pub bitrate: u32,
}

/// A group of track output-policy overrides targeting one collection handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackRuleGroup {
    pub collection_handle: String,
    pub rules: Vec<TrackRule>,
}

/// A [`TrackTarget`] and the [`TrackAction`] to apply to it.
///
/// Non-exhaustive to allow future fields. Downstream crates can access its
/// public fields but cannot use struct literals or exhaustive patterns.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum TrackAction {
    Exclude,
    Bitrate(u32),
}

/// A validated library configuration returned by [`parse`].
///
/// Validation checks configuration rules only. It does not verify filesystem
/// resources, collection membership, metadata matching, target-size interpretation,
/// or deferred file-selection policy.
///
/// Nested structs expose public fields. [`FilesConfig`], [`CollectionDeclaration`],
/// [`CollectionRule`], and [`TrackRule`] are non-exhaustive. Downstream crates
/// can construct [`TrackRuleGroup`] with a struct literal.
///
/// Keep this aggregate for operations that rely on validation. Detached or
/// modified nested values do not carry its guarantee of validity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryBuildSpec {
    codec: Codec,
    encoding_profile: EncodingProfile,
    size_mode: SizeMode,
    files: FilesConfig,
    /// Collection declarations keyed by their configuration handles.
    collection_declarations: IndexMap<String, CollectionDeclaration>,
    collection_rules: Vec<CollectionRule>,
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

    /// The configured file-handling options.
    pub fn files(&self) -> &FilesConfig {
        &self.files
    }

    /// Returns collection declarations as `(handle, declaration)` pairs in declaration order.
    ///
    /// [`parse`] orders handles by first appearance. Adding fields to an existing declaration does
    /// not change its position.
    pub fn collection_declarations(
        &self,
    ) -> impl Iterator<Item = (&str, &CollectionDeclaration)> + '_ {
        self.collection_declarations
            .iter()
            .map(|(collection_handle, declaration)| (collection_handle.as_str(), declaration))
    }

    /// Returns the declaration under `collection_handle`, if any.
    ///
    /// Handles are matched exactly, with no trimming, case folding, or alias
    /// lookup.
    pub fn collection_declaration(
        &self,
        collection_handle: &str,
    ) -> Option<&CollectionDeclaration> {
        self.collection_declarations.get(collection_handle)
    }

    /// The configured collection rules, in document order.
    pub fn collection_rules(&self) -> &[CollectionRule] {
        &self.collection_rules
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
#[non_exhaustive]
pub enum InvalidLibraryBuildSpec {
    /// Neither `bitrate` nor `target_size` was configured.
    MissingSizeMode,
    /// Both `bitrate` and `target_size` were configured.
    ConflictingSizeMode,
    /// A collection table has no metadata or directory selector.
    MissingCollectionSelector { collection_handle: String },
    /// Both singular `album_name` and plural `album_names` keys were configured.
    ConflictingCollectionAlbumNameForms { collection_handle: String },
    /// The configured `album_names` list is empty.
    EmptyCollectionAlbumNames { collection_handle: String },
    /// Both singular `album_artist` and plural `album_artists` keys were configured.
    ConflictingCollectionAlbumArtistForms { collection_handle: String },
    /// The configured `album_artists` list is empty.
    EmptyCollectionAlbumArtists { collection_handle: String },
    /// Both singular `track_artist` and plural `track_artists` keys were configured.
    ConflictingCollectionTrackArtistForms { collection_handle: String },
    /// The configured `track_artists` list is empty.
    EmptyCollectionTrackArtists { collection_handle: String },
    /// A collection declaration configured both TOML keys `directory` and `directories`.
    ConflictingCollectionDirectoryForms { collection_handle: String },
    /// A collection configured an empty `directory` path.
    EmptyCollectionDirectory { collection_handle: String },
    /// A collection configured an empty `directories` list.
    EmptyCollectionDirectories { collection_handle: String },
    /// A collection configured an empty entry in its `directories` list.
    EmptyCollectionDirectoriesEntry {
        collection_handle: String,
        index: usize,
    },
    /// A rule references a handle with no `[collections.<handle>]` entry.
    UnknownCollectionHandle { collection_handle: String },
    /// A collection rule has no target handles in its TOML `collections` key.
    EmptyCollectionRuleTargets,
    /// A collection handle is targeted more than once, including repetition
    /// within one rule.
    DuplicateCollectionRule { collection_handle: String },
    /// More than one track rule targeted the same track of the same collection.
    DuplicateTrackRule {
        collection_handle: String,
        track_name: String,
    },
    /// An explicit `[[track_rules]]` group configured no nested rules.
    EmptyTrackRules { collection_handle: String },
    /// A track rule set both `track` and `tracks`, or neither.
    TrackTargetNotExclusive { collection_handle: String },
    /// A track rule configured an empty `tracks` list.
    EmptyTracks { collection_handle: String },
    /// A track rule set both `exclude = true` and `bitrate`, or neither.
    TrackActionNotExclusive { collection_handle: String },
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
            InvalidLibraryBuildSpec::MissingCollectionSelector { collection_handle } => {
                write!(
                    f,
                    "collection `{collection_handle}` must set at least one of the TOML keys `album_name`, `album_names`, `album_artist`, `album_artists`, `track_artist`, `track_artists`, `directory`, or `directories`"
                )
            }
            InvalidLibraryBuildSpec::ConflictingCollectionAlbumNameForms { collection_handle } => {
                write!(
                    f,
                    "collection `{collection_handle}` must set at most one of the TOML keys `album_name` or `album_names`"
                )
            }
            InvalidLibraryBuildSpec::EmptyCollectionAlbumNames { collection_handle } => {
                write!(
                    f,
                    "collection `{collection_handle}` has an empty `album_names` list"
                )
            }
            InvalidLibraryBuildSpec::ConflictingCollectionAlbumArtistForms {
                collection_handle,
            } => {
                write!(
                    f,
                    "collection `{collection_handle}` must set at most one of the TOML keys `album_artist` or `album_artists`"
                )
            }
            InvalidLibraryBuildSpec::EmptyCollectionAlbumArtists { collection_handle } => {
                write!(
                    f,
                    "collection `{collection_handle}` has an empty `album_artists` list"
                )
            }
            InvalidLibraryBuildSpec::ConflictingCollectionTrackArtistForms {
                collection_handle,
            } => {
                write!(
                    f,
                    "collection `{collection_handle}` must set at most one of the TOML keys `track_artist` or `track_artists`"
                )
            }
            InvalidLibraryBuildSpec::EmptyCollectionTrackArtists { collection_handle } => {
                write!(
                    f,
                    "collection `{collection_handle}` has an empty `track_artists` list"
                )
            }
            InvalidLibraryBuildSpec::ConflictingCollectionDirectoryForms { collection_handle } => {
                write!(
                    f,
                    "collection `{collection_handle}` must set at most one of the TOML keys `directory` or `directories`"
                )
            }
            InvalidLibraryBuildSpec::EmptyCollectionDirectory { collection_handle } => {
                write!(
                    f,
                    "collection `{collection_handle}` has an empty `directory` path"
                )
            }
            InvalidLibraryBuildSpec::EmptyCollectionDirectories { collection_handle } => {
                write!(
                    f,
                    "collection `{collection_handle}` has an empty `directories` list"
                )
            }
            InvalidLibraryBuildSpec::EmptyCollectionDirectoriesEntry {
                collection_handle,
                index,
            } => {
                write!(
                    f,
                    "collection `{collection_handle}` has an empty `directories` entry at index {index}"
                )
            }
            InvalidLibraryBuildSpec::UnknownCollectionHandle { collection_handle } => {
                write!(
                    f,
                    "rule references undeclared collection handle `{collection_handle}`"
                )
            }
            InvalidLibraryBuildSpec::EmptyCollectionRuleTargets => {
                write!(
                    f,
                    "collection rule must list at least one collection handle in `collections`"
                )
            }
            InvalidLibraryBuildSpec::DuplicateCollectionRule { collection_handle } => {
                write!(
                    f,
                    "collection handle `{collection_handle}` is targeted more than once by `collection_rules`"
                )
            }
            InvalidLibraryBuildSpec::DuplicateTrackRule {
                collection_handle,
                track_name,
            } => {
                write!(
                    f,
                    "track `{track_name}` in collection `{collection_handle}` has more than one rule"
                )
            }
            InvalidLibraryBuildSpec::EmptyTrackRules { collection_handle } => {
                write!(
                    f,
                    "track rule group for collection `{collection_handle}` has no nested `rules`"
                )
            }
            InvalidLibraryBuildSpec::TrackTargetNotExclusive { collection_handle } => {
                write!(
                    f,
                    "track rule for collection `{collection_handle}` must set exactly one of the TOML keys `track` or `tracks`"
                )
            }
            InvalidLibraryBuildSpec::EmptyTracks { collection_handle } => {
                write!(
                    f,
                    "track rule for collection `{collection_handle}` has an empty `tracks` list"
                )
            }
            InvalidLibraryBuildSpec::TrackActionNotExclusive { collection_handle } => {
                write!(
                    f,
                    "track rule for collection `{collection_handle}` must set exactly one of the TOML keys `exclude` or `bitrate`"
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
    files: FilesConfig,
    /// Ordered by first introduction of each handle so that
    /// the validated map preserves declaration order.
    #[serde(default)]
    collections: IndexMap<String, RawCollectionDeclaration>,
    #[serde(default)]
    collection_rules: Vec<CollectionRule>,
    #[serde(default)]
    track_rules: Vec<RawTrackRuleGroup>,
}

/// A track-rule group references a collection handle and contains nested
/// track rules for that collection.
///
/// Raw, pre-validation, mid-deserialization struct.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTrackRuleGroup {
    #[serde(rename = "collection")]
    collection_handle: String,
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
            collections,
            collection_rules,
            track_rules,
        } = self;

        let size_mode = match (bitrate, target_size) {
            (Some(bitrate), None) => SizeMode::Bitrate(bitrate),
            (None, Some(target_size)) => SizeMode::TargetSize(target_size),
            (Some(_), Some(_)) => return Err(InvalidLibraryBuildSpec::ConflictingSizeMode.into()),
            (None, None) => return Err(InvalidLibraryBuildSpec::MissingSizeMode.into()),
        };

        // Validate each collection declaration before checking rules that
        // reference handles, so a malformed declaration is reported first.
        let mut validated_declarations = IndexMap::new();
        for (collection_handle, raw_declaration) in collections {
            let declaration = raw_declaration.into_declaration(&collection_handle)?;
            validated_declarations.insert(collection_handle, declaration);
        }

        // Use a new BTreeSet to determine uniqueness
        let mut rule_checked_collection_handles = BTreeSet::new();
        for rule in &collection_rules {
            if rule.collection_handles.is_empty() {
                return Err(InvalidLibraryBuildSpec::EmptyCollectionRuleTargets.into());
            }
            for collection_handle in &rule.collection_handles {
                check_collection_handle_defined(&validated_declarations, collection_handle)?;
                // false means duplicate
                if !rule_checked_collection_handles.insert(collection_handle.as_str()) {
                    return Err(InvalidLibraryBuildSpec::DuplicateCollectionRule {
                        collection_handle: collection_handle.clone(),
                    }
                    .into());
                }
            }
        }

        // Once again, use a new BTreeSet to determine uniqueness
        let mut rule_checked_tracks: BTreeSet<(String, String)> = BTreeSet::new();
        let mut validated_groups = Vec::with_capacity(track_rules.len());
        for group in track_rules {
            check_collection_handle_defined(&validated_declarations, &group.collection_handle)?;
            if group.rules.is_empty() {
                return Err(InvalidLibraryBuildSpec::EmptyTrackRules {
                    collection_handle: group.collection_handle,
                }
                .into());
            }
            let RawTrackRuleGroup {
                collection_handle,
                rules,
            } = group;
            let mut validated_rules: Vec<TrackRule> = Vec::with_capacity(rules.len());
            for raw_rule in rules {
                let rule: TrackRule = raw_rule.into_rule(&collection_handle)?;
                for track_name in rule.target.track_names() {
                    let key = (collection_handle.clone(), track_name.to_owned());
                    // false means duplicate
                    if !rule_checked_tracks.insert(key) {
                        return Err(InvalidLibraryBuildSpec::DuplicateTrackRule {
                            collection_handle: collection_handle.clone(),
                            track_name: track_name.to_owned(),
                        }
                        .into());
                    }
                }
                validated_rules.push(rule);
            }
            validated_groups.push(TrackRuleGroup {
                collection_handle,
                rules: validated_rules,
            });
        }

        Ok(LibraryBuildSpec {
            codec,
            encoding_profile,
            size_mode,
            files,
            collection_declarations: validated_declarations,
            collection_rules,
            track_rules: validated_groups,
        })
    }
}

impl RawCollectionDeclaration {
    /// Collapses one raw collection declaration into its validated form.
    ///
    /// Singular metadata keys become one-element lists. Metadata strings are
    /// preserved exactly, including empty and whitespace-only values. Plural
    /// lists retain their order and repetitions. Supplying both forms in one
    /// family is an error, even when values agree or the plural list is empty.
    /// Empty plural metadata lists are also invalid. The singular and plural
    /// directory keys conflict when both are supplied. Otherwise they produce
    /// one ordered list. Empty directory paths, `directories = []`, and empty
    /// entries in `directories` are rejected.
    fn into_declaration(
        self,
        collection_handle: &str,
    ) -> Result<CollectionDeclaration, LibraryBuildSpecError> {
        let RawCollectionDeclaration {
            album_name,
            album_names,
            album_artist,
            album_artists,
            track_artist,
            track_artists,
            directory,
            directories,
        } = self;

        let has_selector = album_name.is_some()
            || album_names.is_some()
            || album_artist.is_some()
            || album_artists.is_some()
            || track_artist.is_some()
            || track_artists.is_some()
            || directory.is_some()
            || directories.is_some();

        if !has_selector {
            return Err(InvalidLibraryBuildSpec::MissingCollectionSelector {
                collection_handle: collection_handle.to_owned(),
            }
            .into());
        }

        let album_names = match (album_name, album_names) {
            (Some(_), Some(_)) => {
                return Err(
                    InvalidLibraryBuildSpec::ConflictingCollectionAlbumNameForms {
                        collection_handle: collection_handle.to_owned(),
                    }
                    .into(),
                );
            }
            (Some(album_name), None) => Some(vec![album_name]),
            (None, Some(album_names)) => {
                if album_names.is_empty() {
                    return Err(InvalidLibraryBuildSpec::EmptyCollectionAlbumNames {
                        collection_handle: collection_handle.to_owned(),
                    }
                    .into());
                }
                Some(album_names)
            }
            (None, None) => None,
        };

        let album_artists = match (album_artist, album_artists) {
            (Some(_), Some(_)) => {
                return Err(
                    InvalidLibraryBuildSpec::ConflictingCollectionAlbumArtistForms {
                        collection_handle: collection_handle.to_owned(),
                    }
                    .into(),
                );
            }
            (Some(album_artist), None) => Some(vec![album_artist]),
            (None, Some(album_artists)) => {
                if album_artists.is_empty() {
                    return Err(InvalidLibraryBuildSpec::EmptyCollectionAlbumArtists {
                        collection_handle: collection_handle.to_owned(),
                    }
                    .into());
                }
                Some(album_artists)
            }
            (None, None) => None,
        };

        let track_artists = match (track_artist, track_artists) {
            (Some(_), Some(_)) => {
                return Err(
                    InvalidLibraryBuildSpec::ConflictingCollectionTrackArtistForms {
                        collection_handle: collection_handle.to_owned(),
                    }
                    .into(),
                );
            }
            (Some(track_artist), None) => Some(vec![track_artist]),
            (None, Some(track_artists)) => {
                if track_artists.is_empty() {
                    return Err(InvalidLibraryBuildSpec::EmptyCollectionTrackArtists {
                        collection_handle: collection_handle.to_owned(),
                    }
                    .into());
                }
                Some(track_artists)
            }
            (None, None) => None,
        };

        let directories = match (directory, directories) {
            (None, None) => None,
            (Some(directory), None) => {
                // An empty decoded string is exactly an empty path
                if directory.is_empty() {
                    return Err(InvalidLibraryBuildSpec::EmptyCollectionDirectory {
                        collection_handle: collection_handle.to_owned(),
                    }
                    .into());
                }
                Some(vec![PathBuf::from(directory)])
            }
            (None, Some(directories)) => {
                if directories.is_empty() {
                    return Err(InvalidLibraryBuildSpec::EmptyCollectionDirectories {
                        collection_handle: collection_handle.to_owned(),
                    }
                    .into());
                }
                if let Some(index) = directories.iter().position(String::is_empty) {
                    return Err(InvalidLibraryBuildSpec::EmptyCollectionDirectoriesEntry {
                        collection_handle: collection_handle.to_owned(),
                        index,
                    }
                    .into());
                }
                Some(directories.into_iter().map(PathBuf::from).collect())
            }
            (Some(_), Some(_)) => {
                return Err(
                    InvalidLibraryBuildSpec::ConflictingCollectionDirectoryForms {
                        collection_handle: collection_handle.to_owned(),
                    }
                    .into(),
                );
            }
        };

        Ok(CollectionDeclaration {
            album_names,
            album_artists,
            track_artists,
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
    fn into_rule(self, collection_handle: &str) -> Result<TrackRule, LibraryBuildSpecError> {
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
                    collection_handle: collection_handle.to_owned(),
                }
                .into());
            }
            _ => {
                return Err(InvalidLibraryBuildSpec::TrackTargetNotExclusive {
                    collection_handle: collection_handle.to_owned(),
                }
                .into());
            }
        };

        let action = match (exclude, bitrate) {
            (Some(true), None) => TrackAction::Exclude,
            (None, Some(bitrate)) => TrackAction::Bitrate(bitrate),
            _ => {
                return Err(InvalidLibraryBuildSpec::TrackActionNotExclusive {
                    collection_handle: collection_handle.to_owned(),
                }
                .into());
            }
        };

        Ok(TrackRule { target, action })
    }
}

fn check_collection_handle_defined(
    declarations: &IndexMap<String, CollectionDeclaration>,
    collection_handle: &str,
) -> Result<(), LibraryBuildSpecError> {
    if declarations.contains_key(collection_handle) {
        Ok(())
    } else {
        Err(InvalidLibraryBuildSpec::UnknownCollectionHandle {
            collection_handle: collection_handle.to_owned(),
        }
        .into())
    }
}
