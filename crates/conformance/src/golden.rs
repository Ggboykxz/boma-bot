//! The fixture manifest and the fixtures themselves.

use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, de::DeserializeOwned};

/// Whether the suite replays a fixture today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    /// The use case exists and the golden must pass.
    Implemented,
    /// The use case does not exist yet; the fixture must carry a reason and a milestone.
    Pending,
}

/// One fixture's entry in `golden-status.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Current status.
    pub status: Status,
    /// Requirement the fixture belongs to.
    pub req: String,
    /// Why the fixture is pending. Required unless implemented.
    #[serde(default)]
    pub reason: Option<String>,
}

/// `golden-status.json`, at the repository root.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// API fixtures, keyed by file stem.
    pub api: BTreeMap<String, Entry>,
    /// UI fixtures, keyed by file stem.
    pub ui: BTreeMap<String, Entry>,
}

/// A manifest or fixture that cannot be read.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// The file is missing or unreadable.
    #[error("cannot read {path}: {source}")]
    Read {
        /// File that failed.
        path: PathBuf,
        /// Underlying IO error.
        #[source]
        source: io::Error,
    },
    /// The file is not the JSON the harness expects.
    #[error("invalid JSON in {path}: {source}")]
    Json {
        /// File that failed.
        path: PathBuf,
        /// Underlying parse error.
        #[source]
        source: serde_json::Error,
    },
}

impl Manifest {
    /// Loads the manifest from the repository root.
    ///
    /// # Errors
    ///
    /// Returns [`ManifestError`] when the file is missing, unreadable or not the expected shape.
    pub fn load() -> Result<Self, ManifestError> {
        load_json(&repo_path("golden-status.json"))
    }
}

/// An API golden fixture: one request, the response the reference produced, and why.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    /// The request the reference received.
    pub request: GoldenRequest,
    /// The response the reference returned.
    pub response: GoldenResponse,
    /// What the fixture pins down, in prose.
    pub notes: String,
}

/// The request half of an API fixture.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenRequest {
    /// HTTP method.
    pub method: String,
    /// Path, query string included.
    pub path: String,
    /// Request headers.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// JSON body, when the request has one.
    #[serde(default)]
    pub body: Option<serde_json::Value>,
}

/// The response half of an API fixture.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenResponse {
    /// Status code the reference returned.
    pub status: u16,
    /// Response headers the fixture pins, when any.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Declared content type, for fixtures that are not JSON.
    #[serde(default, rename = "contentType")]
    pub content_type: Option<String>,
    /// JSON body, when the reference returned one.
    #[serde(default)]
    pub body: Option<serde_json::Value>,
    /// Stream frames, for the SSE fixtures.
    #[serde(default)]
    pub events: Option<serde_json::Value>,
}

/// A UI golden fixture: a seeded state and what must be visible in it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiFixture {
    /// The state the runner must seed before asserting.
    pub state: UiState,
    /// The assertions, in document order.
    pub expect: Vec<UiExpectation>,
    /// What the fixture pins down, in prose.
    pub notes: String,
}

/// The seeded state of a UI fixture.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiState {
    /// Route to open.
    pub route: String,
    /// Name of the seed the runner must apply.
    pub seed: String,
    /// Role the signed-in person has.
    pub role: String,
    /// Viewport, as `WIDTHxHEIGHT`.
    pub viewport: String,
    /// Media features to emulate.
    pub media: UiMedia,
}

/// Media features to emulate before asserting.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiMedia {
    /// `light` or `dark`.
    #[serde(rename = "colorScheme")]
    pub color_scheme: String,
    /// `reduce` or `no-preference`.
    #[serde(rename = "reducedMotion")]
    pub reduced_motion: String,
}

/// One assertion of a UI fixture.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiExpectation {
    /// ARIA role or selector the assertion targets.
    pub role: String,
    /// State it must be in, when the fixture pins one.
    #[serde(default)]
    pub state: Option<String>,
    /// Exact number of matches, when the fixture pins one.
    #[serde(default)]
    pub count: Option<u32>,
}

/// A path inside the repository, independent of the working directory.
#[must_use]
pub fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

/// File stems of `<dir>/*.json`, sorted.
///
/// # Errors
///
/// Returns the underlying IO error when the directory cannot be read.
pub fn fixture_stems(dir: &Path) -> io::Result<Vec<String>> {
    let mut stems = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(std::ffi::OsStr::to_str) == Some("json") {
            if let Some(stem) = path.file_stem().and_then(std::ffi::OsStr::to_str) {
                stems.push(stem.to_owned());
            }
        }
    }
    stems.sort();
    Ok(stems)
}

/// Loads one API fixture by file stem.
///
/// # Errors
///
/// Returns [`ManifestError`] when the fixture is missing, unreadable or not the expected shape.
pub fn load_fixture(stem: &str) -> Result<Fixture, ManifestError> {
    load_json(&repo_path(&format!("spec/golden/{stem}.json")))
}

/// Loads one UI fixture by file stem.
///
/// # Errors
///
/// Returns [`ManifestError`] when the fixture is missing, unreadable or not the expected shape.
pub fn load_ui_fixture(stem: &str) -> Result<UiFixture, ManifestError> {
    load_json(&repo_path(&format!("spec/golden/ui/{stem}.json")))
}

fn load_json<T: DeserializeOwned>(path: &Path) -> Result<T, ManifestError> {
    let bytes = fs::read(path).map_err(|source| ManifestError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| ManifestError::Json {
        path: path.to_path_buf(),
        source,
    })
}
