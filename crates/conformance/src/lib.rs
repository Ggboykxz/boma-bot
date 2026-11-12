//! # boma-conformance
//!
//! The harness that keeps the rebuild honest, with no database and no browser:
//!
//! * [`golden`] models `golden-status.json`, the manifest that says, for every fixture in
//!   `spec/golden/`, whether the API already replays it or why it cannot yet.
//! * `tests/golden_manifest.rs` fails when a fixture and the manifest disagree in either direction,
//!   so a new spec export cannot slip in unreplayed and an implemented fixture cannot be forgotten.
//! * `tests/golden_api.rs` replays every implemented fixture against the real router.
//! * `tests/contract.rs` compares the generated OpenAPI document with `spec/openapi.yaml`.
//!
//! A pending fixture is never skipped silently: the runner prints `SKIP <name> (<req>): <reason>`
//! and the manifest test refuses an entry without a reason.

#![deny(missing_docs)]

pub mod golden;

pub use golden::{
    Entry, Fixture, GoldenRequest, GoldenResponse, Manifest, ManifestError, Status, UiFixture,
    fixture_stems, load_fixture, load_ui_fixture, repo_path,
};
