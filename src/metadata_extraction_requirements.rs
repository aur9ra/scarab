/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Discovery of selector-metadata requirements from observed reporting scopes.
//!
//! This capability interprets configuration and inventory observations, not
//! extraction history or outcomes. It neither reads files nor evaluates
//! metadata predicates or collection membership.

use std::collections::HashMap;
use std::path::Path;

use crate::config::LibraryBuildSpec;
use crate::observed_source_file_inventory::{ObservedSourceFileInventory, RequiredScope};
use crate::source_audio::classify_source_audio;

/// Returns source-audio pathnames in any domain requiring selector metadata.
///
/// Here, metadata means the complete selector-metadata observation. A file
/// qualifies if a reporting configured scope's declaration supplies metadata
/// selectors, or if it is reported by the shared default source-root scope.
/// Validated declarations without directories necessarily have metadata
/// selectors, so the default scope always requires selector metadata.
///
/// Paths are borrowed from the inventory, with its retained spelling and native
/// pathname identity preserved. Each appears at most once, in unspecified
/// order. This eagerly evaluated result is not pending extraction work and
/// does not depend on whether extraction has occurred or would succeed.
/// No filesystem reinspection or metadata matching takes place.
///
/// # Panics
///
/// The inventory must be paired with the specification that produced it. A
/// reporting configured-scope handle absent from `spec` violates that contract.
// Staged until production extraction integration.
#[allow(dead_code)]
pub(crate) fn required_metadata_paths<'inventory>(
    spec: &LibraryBuildSpec,
    inventory: &'inventory ObservedSourceFileInventory,
) -> Vec<&'inventory Path> {
    let collection_requires_metadata: HashMap<&str, bool> = spec
        .collection_declarations()
        .map(|(collection_handle, declaration)| {
            (collection_handle, declaration.has_metadata_selectors())
        })
        .collect();

    inventory
        .files()
        .filter(|file| classify_source_audio(file.path()).is_some())
        // for all source audio files, see if it falls under *any* scope which references
        // a collection declared with metadata selectors
        //
        // currently we assume logically that any collections declared with a default source
        // root must be declared with metadata predicates
        .filter(|file| {
            file.reporting_scopes().any(|scope| match scope {
                RequiredScope::ConfiguredDirectory {
                    collection_handle, ..
                } => collection_requires_metadata
                    .get(collection_handle.as_str())
                    .copied()
                    .expect("reporting configured-scope handle must exist in the paired build specification"),
                RequiredScope::DefaultSourceRoot { .. } => true,
            })
        })
        .map(|file| file.path())
        .collect()
}

#[cfg(test)]
#[path = "tests/metadata_extraction_requirements.rs"]
mod tests;
