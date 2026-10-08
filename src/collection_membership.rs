/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Reader-independent collection membership evaluation from a completed
//! observed source file inventory.
//!
//! Every configured declaration produces a result, even when its domain is
//! empty. Its domain comes from inventory reporting associations. Declarations
//! with configured directory selectors use source-audio files reported by
//! their own configured scopes. Declarations without them use source-audio
//! files reported by the shared default source-root scope if it names their
//! handle. Path ancestry, containment, or equal resolved roots do not establish
//! the required association. Another collection's scope does not qualify on its
//! own.
//!
//! Each inventory pathname appears at most once in a collection's results,
//! though it may belong to several collections. Evaluation uses only the spec,
//! inventory, and supplied extraction outcomes. It does not read source files,
//! access the filesystem, probe media, or decide output policy.
//!
//! Directory-only declarations include every in-domain file. Metadata
//! selectors filter the domain, so declarations with both directory and
//! metadata selectors include only files matching both.
//!
//! For metadata-dependent declarations, a missing extraction outcome leaves an
//! in-domain file unresolved as unavailable. A failed extraction outcome leaves it
//! unresolved and borrows the failure payload. A successful non-match is a resolved
//! non-member. Extraction outcomes outside a collection's domain have no effect, and
//! metadata-map entries never add inventory files.
//!
//! Membership preserves inventory pathname spelling and uses native `Path`
//! equality for outcome lookup. It owns collection handles and result
//! pathnames, borrows failure payloads from the supplied outcomes, and cannot
//! outlive them. The payload type `MetadataFailure` is opaque caller context
//! and has no trait bounds.

// This module is used only by its tests. Keep it private until the build
// pipeline uses it.
#![allow(dead_code)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::config::LibraryBuildSpec;
use crate::metadata_selection::{
    MetadataSelectorObservation, MetadataSelectorPredicate, observation_matches_predicate,
};
use crate::observed_source_file_inventory::{
    ObservedSourceFile, ObservedSourceFileInventory, RequiredScope,
};
use crate::source_audio::classify_source_audio;

/// One supplied metadata extraction outcome per pathname, keyed by native `Path` equality.
pub(crate) type MetadataExtractionOutcomes<MetadataFailure> =
    HashMap<PathBuf, Result<MetadataSelectorObservation, MetadataFailure>>;

/// Membership results of every declaration in one validated build specification.
#[derive(Debug)]
pub(crate) struct CollectionMembership<'metadata, MetadataFailure> {
    collections: Vec<CollectionMembers<'metadata, MetadataFailure>>,
}

/// Established members and unresolved files for one collection.
#[derive(Debug)]
pub(crate) struct CollectionMembers<'metadata, MetadataFailure> {
    collection_handle: String,
    members: Vec<PathBuf>,
    unresolved: Vec<UnresolvedMembership<'metadata, MetadataFailure>>,
}

/// An in-domain file in a metadata-dependent collection with a missing or
/// failed metadata outcome.
#[derive(Debug)]
pub(crate) struct UnresolvedMembership<'metadata, MetadataFailure> {
    path: PathBuf,
    reason: UnresolvedMetadataReason<'metadata, MetadataFailure>,
}

/// Why membership is unresolved for an in-domain file with a missing or failed
/// metadata outcome.
#[derive(Debug)]
pub(crate) enum UnresolvedMetadataReason<'metadata, MetadataFailure> {
    /// No outcome was supplied for the file's pathname.
    Unavailable,
    /// The supplied failure payload, borrowed unchanged.
    Failed(&'metadata MetadataFailure),
}

impl<'metadata, MetadataFailure> CollectionMembership<'metadata, MetadataFailure> {
    /// Evaluated collections in declaration order.
    pub(crate) fn collections(&self) -> &[CollectionMembers<'metadata, MetadataFailure>] {
        &self.collections
    }

    /// Whether all collections are complete.
    pub(crate) fn is_complete(&self) -> bool {
        self.collections.iter().all(CollectionMembers::is_complete)
    }
}

impl<'metadata, MetadataFailure> CollectionMembers<'metadata, MetadataFailure> {
    /// The configuration handle of this collection.
    pub(crate) fn collection_handle(&self) -> &str {
        &self.collection_handle
    }

    /// Pathnames of the collection's members, in no specified order.
    pub(crate) fn members(&self) -> &[PathBuf] {
        &self.members
    }

    /// In-domain files with unresolved membership, in no specified order.
    pub(crate) fn unresolved(&self) -> &[UnresolvedMembership<'metadata, MetadataFailure>] {
        &self.unresolved
    }

    /// Whether this collection has no unresolved in-domain files.
    pub(crate) fn is_complete(&self) -> bool {
        self.unresolved.is_empty()
    }
}

impl<'metadata, MetadataFailure> UnresolvedMembership<'metadata, MetadataFailure> {
    /// The inventory pathname, with its spelling preserved.
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// Why this file's membership is unresolved.
    pub(crate) fn reason(&self) -> &UnresolvedMetadataReason<'metadata, MetadataFailure> {
        &self.reason
    }
}

/// Evaluates every collection declared in `spec` using `inventory` and
/// `metadata_extraction_outcomes`.
///
/// Only inventory files recognized by [`classify_source_audio`] participate. A
/// declaration with configured directory selectors uses files reported by its
/// own configured scopes. Without them, it uses files reported by the default
/// source-root scope that names its handle. Directory-only declarations include
/// every participating in-domain file. Metadata selectors include files only when
/// their successful outcome matches. Missing outcomes leave files unresolved as
/// unavailable, supplied failures remain unresolved with their payload borrowed
/// unchanged, and successful nonmatches are known non-members.
///
/// Returns one result per declaration in declaration order, including empty
/// domains. The inputs are not modified.
pub(crate) fn evaluate_collection_membership<'metadata, MetadataFailure>(
    spec: &LibraryBuildSpec,
    inventory: &ObservedSourceFileInventory,
    metadata_extraction_outcomes: &'metadata MetadataExtractionOutcomes<MetadataFailure>,
) -> CollectionMembership<'metadata, MetadataFailure> {
    let source_audio_files: Vec<ObservedSourceFile<'_>> = inventory
        .files()
        .filter(|file| classify_source_audio(file.path()).is_some())
        .collect();

    let mut collections = Vec::new();
    for (collection_handle, declaration) in spec.collection_declarations() {
        let has_configured_directories = declaration.directories.is_some();
        let metadata_predicate: Option<MetadataSelectorPredicate> =
            if declaration.has_metadata_selectors() {
                Some(declaration.metadata_selector_predicate())
            } else {
                None
            };

        let mut collection_members = Vec::new();
        let mut unresolved = Vec::new();
        for file in source_audio_files
            .iter()
            .filter(|file| file_in_domain(file, collection_handle, has_configured_directories))
        {
            let path = file.path();
            // for this collection, does this file it's found under contain metadata selector
            // predicates?
            match &metadata_predicate {
                // no
                // directory-only declarations include every in-domain file
                None => collection_members.push(path.to_path_buf()),
                // yes
                // are there observations relevant to the file?
                Some(predicate) => match metadata_extraction_outcomes.get(path) {
                    // yes, and the observation family is valid
                    Some(Ok(observation)) => {
                        // the observations match the predicates, therefore this file
                        // is a valid member of this collection
                        if observation_matches_predicate(observation, predicate) {
                            collection_members.push(path.to_path_buf());
                        }
                    }
                    // yes, but the observation was a failure. record the reason of failure
                    Some(Err(failure)) => unresolved.push(UnresolvedMembership {
                        path: path.to_path_buf(),
                        reason: UnresolvedMetadataReason::Failed(failure),
                    }),
                    // no. record membership unresolved due to unable to access metadata
                    None => unresolved.push(UnresolvedMembership {
                        path: path.to_path_buf(),
                        reason: UnresolvedMetadataReason::Unavailable,
                    }),
                },
            }
        }

        collections.push(CollectionMembers {
            collection_handle: collection_handle.to_owned(),
            members: collection_members,
            unresolved,
        });
        // proceed to next collection
    }

    CollectionMembership { collections }
}

/// Whether `file` is in `collection_handle`'s domain according to its reporting
/// scopes.
///
/// A collection with directory selectors uses only its configured scopes.
/// Without them, the default source-root scope must list its handle. Another
/// collection's scope does not qualify merely because its resolved root is
/// equal or its path overlaps.
#[allow(clippy::needless_return)] // explicit returns highlight closure exit
// paths within nested branches
fn file_in_domain(
    file: &ObservedSourceFile<'_>,
    collection_handle: &str,
    has_configured_directories: bool,
) -> bool {
    file.reporting_scopes().any(|required_scope| {
        if has_configured_directories {
            // only this collection's own configured scope qualifies
            if let RequiredScope::ConfiguredDirectory {
                collection_handle: scope_handle,
                ..
            } = required_scope
            {
                return scope_handle.as_str() == collection_handle;
            } else {
                return false;
            }
        } else if let RequiredScope::DefaultSourceRoot {
            dependent_collection_handles,
            ..
        } = required_scope
        {
            // only the shared default root that lists this handle qualifies
            return dependent_collection_handles
                .iter()
                .any(|dependent_handle| dependent_handle.as_str() == collection_handle);
        } else {
            return false;
        }
    })
}

#[cfg(test)]
#[path = "tests/collection_membership.rs"]
mod tests;
