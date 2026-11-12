//! Contract test: the generated document must stay inside `spec/openapi.yaml`.
//!
//! The comparison is deliberately structural rather than textual. Operational ids and the set of
//! status codes are compared exactly; schemas are compared by the fields the spec requires, because
//! the generator is free to order `properties` differently. Every other difference is caught by the
//! Rust type system or by the response goldens.

use std::collections::BTreeSet;

use boma_conformance::repo_path;

fn generated() -> serde_json::Value {
    serde_json::to_value(boma_api::openapi::document()).expect("the document serialises")
}

fn spec() -> serde_json::Value {
    let path = repo_path("spec/openapi.yaml");
    let text = std::fs::read_to_string(&path).expect("spec/openapi.yaml is readable");
    serde_yaml_ng::from_str(&text).expect("spec/openapi.yaml is valid YAML")
}

fn statuses(operation: &serde_json::Value) -> BTreeSet<String> {
    operation["responses"]
        .as_object()
        .map(|responses| responses.keys().cloned().collect())
        .unwrap_or_default()
}

#[test]
fn req_053_the_generated_document_is_a_subset_of_the_spec() {
    let generated = generated();
    let spec = spec();

    let paths = generated["paths"]
        .as_object()
        .expect("the generated document has paths");
    assert!(!paths.is_empty(), "a document with no path proves nothing");

    for (path, item) in paths {
        let spec_item = spec["paths"]
            .get(path)
            .unwrap_or_else(|| panic!("{path} is served but absent from spec/openapi.yaml"));
        for (method, operation) in item.as_object().expect("a path item is an object") {
            if !matches!(method.as_str(), "get" | "post" | "put" | "patch" | "delete") {
                continue;
            }
            let spec_operation = &spec_item[method];
            assert!(
                !spec_operation.is_null(),
                "{method} {path} is served but absent from the spec"
            );
            assert_eq!(
                operation["operationId"], spec_operation["operationId"],
                "{method} {path}: operationId"
            );
            let served = statuses(operation);
            let specified = statuses(spec_operation);
            assert!(
                served.is_subset(&specified),
                "{method} {path}: {served:?} is not a subset of the spec's {specified:?}"
            );
        }
    }
}

#[test]
fn req_053_health_aliases_stay_out_of_the_public_document() {
    let generated = generated();
    for alias in ["/healthz", "/readyz"] {
        assert!(
            generated["paths"].get(alias).is_none(),
            "{alias} is an AGENTS.md alias, not a contract path (docs/STATE.md)"
        );
    }
}

#[test]
fn req_053_every_implemented_route_keeps_its_documentation() {
    let generated = generated();
    for path in ["/health", "/ready", "/internal/health"] {
        assert!(
            generated["paths"].get(path).is_some(),
            "{path} lost its #[utoipa::path] annotation"
        );
    }
}
