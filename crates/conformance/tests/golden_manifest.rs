//! The manifest gate: every fixture is either replayed or explicitly pending.
//!
//! A spec export must never enlarge the fixture set silently, and a fixture must never be dropped
//! from the manifest to make the suite green. Both directions fail here.

use std::collections::BTreeSet;

use boma_conformance::{
    Manifest, Status, fixture_stems, load_fixture, load_ui_fixture, repo_path,
};

fn stems(relative: &str) -> Vec<String> {
    fixture_stems(&repo_path(relative)).expect("the spec directory is readable")
}

#[test]
fn req_053_every_spec_fixture_is_replayed_or_explicitly_pending() {
    let manifest = Manifest::load().expect("golden-status.json is valid");

    let api = stems("spec/golden");
    let ui = stems("spec/golden/ui");

    // The counts come from spec/EXPORT.md. A change here means the spec was re-exported: update the
    // manifest and this number in the same commit, deliberately.
    assert_eq!(api.len(), 30, "spec/EXPORT.md declares 30 API goldens");
    assert_eq!(ui.len(), 7, "spec/EXPORT.md declares 7 UI goldens");

    let manifest_api: BTreeSet<&String> = manifest.api.keys().collect();
    let file_api: BTreeSet<&String> = api.iter().collect();
    assert_eq!(
        manifest_api, file_api,
        "golden-status.json and spec/golden must name the same API fixtures"
    );

    let manifest_ui: BTreeSet<&String> = manifest.ui.keys().collect();
    let file_ui: BTreeSet<&String> = ui.iter().collect();
    assert_eq!(
        manifest_ui, file_ui,
        "golden-status.json and spec/golden/ui must name the same UI fixtures"
    );
}

#[test]
fn req_053_a_pending_fixture_says_why_and_names_its_milestone() {
    let manifest = Manifest::load().expect("golden-status.json is valid");
    for (name, entry) in manifest.api.iter().chain(manifest.ui.iter()) {
        assert!(
            entry.req.len() == 7 && entry.req.starts_with("REQ-"),
            "{name} must name a REQ id, got `{}`",
            entry.req
        );
        match entry.status {
            Status::Pending => {
                let reason = entry.reason.as_deref().unwrap_or_default();
                assert!(!reason.trim().is_empty(), "{name} must say why it is pending");
                assert!(
                    reason.starts_with('M') && reason.contains(" - "),
                    "{name} must name its milestone and a reason, got `{reason}`"
                );
            }
            Status::Implemented => {
                assert!(
                    entry.reason.is_none(),
                    "{name} is implemented; a stale reason would hide a regression"
                );
            }
        }
    }
}

#[test]
fn req_053_every_fixture_parses_with_the_strict_model() {
    let manifest = Manifest::load().expect("golden-status.json is valid");
    for name in manifest.api.keys() {
        load_fixture(name).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    for (name, entry) in &manifest.ui {
        assert!(
            name.starts_with(&entry.req),
            "{name} must be named after the requirement it covers ({})",
            entry.req
        );
        load_ui_fixture(name).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
}
