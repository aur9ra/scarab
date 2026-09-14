/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

//! Focused contract tests for the shared fixture-tree copier.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{TempSandbox, copy_fixture_tree};

mod common;

#[test]
fn rejects_destination_inside_source_before_creating_anything() {
    let sandbox = TempSandbox::new();
    let source = sandbox.path().join("source");
    fs::create_dir(&source).expect("create source");
    fs::write(source.join("track.flac"), b"fixture bytes").expect("write source file");

    let destination = source.join("copy");

    let panic = catch_unwind(AssertUnwindSafe(|| {
        copy_fixture_tree(&source, &destination);
    }))
    .expect_err("a destination inside the source must be rejected");

    let message = panic
        .downcast_ref::<String>()
        .expect("containment rejection must panic with a formatted message");
    assert!(
        message.contains("inside source"),
        "unexpected panic message: {message}"
    );
    assert!(
        !destination.exists(),
        "rejected destination {} must never be created",
        destination.display()
    );
}
