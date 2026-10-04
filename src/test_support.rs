/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Helpers shared by unit tests; compiled only in test builds.

mod sandbox;
mod snapshots;

pub(crate) use sandbox::TempSandbox;
pub(crate) use snapshots::SourceTreeSnapshot;

use std::fs;
use std::path::{Path, PathBuf};

use crate::collection_scope::{
    CollectionScopePreparation, PreparedCollectionScopes, RedundancyWarning,
};

/// Builds a valid test spec with the supplied collection declarations.
pub(crate) fn parse_spec(collection_declarations: &str) -> crate::config::LibraryBuildSpec {
    let text = format!("codec = \"opus\"\nbitrate = 128\n{collection_declarations}");
    crate::parse(&text).expect("test configuration must parse and validate")
}

/// Creates a `source` directory inside `sandbox`.
pub(crate) fn create_source(sandbox: &TempSandbox) -> PathBuf {
    let source = sandbox.path().join("source");
    fs::create_dir(&source).expect("create source root");
    source
}

/// Canonicalizes `path`, including it in the panic if canonicalization fails.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    fs::canonicalize(path)
        .unwrap_or_else(|error| panic!("expected {} to canonicalize: {error}", path.display()))
}

/// Returns the scopes and warnings, or panics if preparation failed.
pub(crate) fn expect_prepared(
    preparation: CollectionScopePreparation,
) -> (PreparedCollectionScopes, Vec<RedundancyWarning>) {
    match preparation {
        CollectionScopePreparation::Prepared { scopes, warnings } => (scopes, warnings),
        CollectionScopePreparation::Failed {
            configured_failures,
            default_source_root_failure,
            warnings,
        } => panic!(
            "expected complete preparation, got failures {configured_failures:?} \
             {default_source_root_failure:?} and warnings {warnings:?}"
        ),
    }
}
