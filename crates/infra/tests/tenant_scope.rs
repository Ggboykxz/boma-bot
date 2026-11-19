//! ADR-002: the two tenant settings are applied in one SQL statement.
//!
//! `boma.space_id` and `boma.system_reason` used to be set by two sequential queries. Between
//! them the transaction had one setting but not the other, and a future edit could set one with
//! `is_local = false` and leave it sticky on a pooled connection. One statement closes that
//! window: either both settings land, or neither does.
//!
//! This is a source-level guard, like `tests/policy.rs` for D-09. The behaviour it protects is
//! structural (one statement, transaction-scoped), so a scan is the honest way to pin it.

#![allow(clippy::expect_used, clippy::panic)]

use std::fs;

#[test]
fn req_043_adr_002_tenant_settings_are_applied_in_one_statement() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/tenant.rs");
    let source = fs::read_to_string(path).expect("tenant.rs is readable");

    // Only the queries that set the tenant state matter; other queries in this module are none.
    let set_config_queries: Vec<&str> = source
        .match_indices("sqlx::query(")
        .map(|(at, _)| &source[at..(at + 400).min(source.len())])
        .filter(|call| call.contains("set_config"))
        .collect();

    assert_eq!(
        set_config_queries.len(),
        1,
        "the tenant state is applied by exactly one SQL statement, so there is no moment where \
         one setting is present without the other; found {}",
        set_config_queries.len()
    );

    let statement = set_config_queries[0];
    assert!(
        statement.contains("boma.space_id") && statement.contains("boma.system_reason"),
        "the single statement sets both the space and the system reason"
    );
    assert!(
        statement.contains("true"),
        "both settings stay transaction-scoped (is_local = true)"
    );
}
