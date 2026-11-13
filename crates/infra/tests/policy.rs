//! D-09: no call site can name a system reason outside the enum's vocabulary.
//!
//! The tenant context is a typed enum, so the compiler is the first line of defence. This
//! test is the second line: it scans the workspace for every construction of a `SystemReason`
//! and refuses a variant outside the five the deployment ships. A reviewer adding a sixth
//! reason by accident or by pressure hits this wall.

#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

// The five variants the deployment ships, plus the constant that enumerates them. A sixth
// reason means: add the variant and this allow-list together, or the change does not pass.
const KNOWN: [&str; 6] = [
    "Boot",
    "Authentication",
    "WebhookDelivery",
    "ScheduledJob",
    "DataMigration",
    "ALL",
];

#[test]
fn d_09_a_system_reason_must_be_a_known_callsite() {
    let root = workspace_root();
    let mut checked = 0usize;
    let mut occurrences = 0usize;
    for entry in walk(&root) {
        checked += 1;
        let relative = entry
            .strip_prefix(&root)
            .expect("the entry belongs to the workspace")
            .display()
            .to_string();
        let text = fs::read_to_string(&entry).expect("a source file is utf-8");
        for occurrence in text.match_indices("SystemReason::") {
            let after = &text[occurrence.0..occurrence.0 + 96.min(text.len() - occurrence.0)];
            let variant = after["SystemReason::".len()..]
                .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .next()
                .unwrap_or("")
                .to_owned();
            // Lowercase members of the type are methods, not a tenant reason: `parse`, `as_str`.
            // Only constructions (and the constant enumerating them) carry the allow-list.
            if variant.starts_with(|c: char| c.is_ascii_lowercase()) {
                continue;
            }
            assert!(
                KNOWN.contains(&variant.as_str()),
                "{relative}: SystemReason::{variant} is not a known system reason; extend the \
                 enum and the allow-list together, or do not pass the gate"
            );
            occurrences += 1;
        }
    }
    assert!(
        checked > 50,
        "the scanner saw {checked} source files across the workspace"
    );
    assert!(
        occurrences >= 10,
        "the scanner should have seen the production call sites; saw {occurrences}"
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the crate has a parent")
        .parent()
        .expect("the crates directory has a parent")
        .to_path_buf()
}

fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn walk(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).expect("a directory is readable");
        for entry in entries {
            let path = entry.expect("an entry is readable").path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !["target", ".git", "node_modules", "spec"].contains(&name) {
                    stack.push(path);
                }
                continue;
            }
            let is_rust = path.extension().is_some_and(|ext| ext == "rs");
            // The scanner does not scan itself: the needle text would look like a call site.
            let is_self = relative(&path, root) == "crates/infra/tests/policy.rs";
            if is_rust && !is_self {
                out.push(path);
            }
        }
    }
    out
}
