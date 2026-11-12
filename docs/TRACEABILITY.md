# Traceability

Every requirement of `spec/01-requirements.md`, the test that proves it, and where the behaviour
lives. A row is written only when a test with the REQ id in its **name** exists: the gate
(`tools/trace_check.py`) counts declarations, not comments, so a row cannot be claimed without
evidence.

Status values:

- **M1** — implemented in the walking skeleton.
- **M2** — the milestone that owns it per `docs/PLAN.md`.
- **M3 …** — same, later milestones.

`tools/trace_check.py` counts the covered ids; the budget only ever decreases (see `docs/STATE.md`).

## Requirements

| REQ | Milestone | Test | Code |
|---|---|---|---|
| REQ-001 | M1 | `crates/api/tests/rpc_surface.rs::req_001_a_send_writes_one_message_and_starts_one_queued_run` | `crates/app/src/usecases.rs::send_message` |
| REQ-002 | M1 | `crates/api/tests/rpc_surface.rs::req_002_the_same_nonce_returns_the_same_message_and_run` | `crates/app/src/usecases.rs::send_message`, `crates/infra/src/stores/messages.rs` |
| REQ-003 | M2 | — | `crates/domain/src/run.rs::RunTrigger::accepts_steering` |
| REQ-004 | M1 | `crates/api/tests/rpc_surface.rs::req_004_creating_a_bot_creates_its_thread` | `crates/app/src/usecases.rs::create_bot` |
| REQ-005 | M1 | `crates/api/tests/rpc_surface.rs::req_005_the_message_page_is_newest_first_and_carries_a_cursor` | `crates/app/src/usecases.rs::page_messages`, `crates/domain/src/message.rs` |
| REQ-006 | M2 | `crates/testkit/src/build.rs::req_006_a_stopped_run_absorbs_nothing_else`, `crates/api/tests/rpc_surface.rs::req_006_a_cancel_stops_the_run_and_is_idempotent` | `crates/app/src/usecases.rs::cancel_run`, `crates/api/src/rpc.rs::runs_cancel`, `crates/app/src/worker.rs` |
| REQ-007 | M2 | — | `crates/domain/src/message.rs::Block::Card` |
| REQ-008 | M2 | — | `crates/domain/src/message.rs` |
| REQ-009 | M2 | — | — |
| REQ-010 | M2 | — | — |
| REQ-011 | M1 | `crates/api/src/wire.rs::req_011_a_run_always_carries_its_empty_fields` | `crates/app/src/usecases.rs::list_runs` |
| REQ-012 | M1 | `crates/testkit/src/memory.rs::req_012_two_claims_never_return_the_same_run`, `crates/infra/tests/m1.rs::req_012_the_claim_moves_a_run_to_leased_and_the_fence_guards_the_write` | `crates/app/src/usecases.rs::claim_run`, `crates/domain/src/run.rs` |
| REQ-013 | M1 | `crates/testkit/src/memory.rs::req_013_an_expired_lease_is_reclaimed_after_a_deploy` | `crates/infra/src/stores/runs.rs::claim_next_queued`, `crates/testkit/src/memory.rs::MemoryRunStore::claim_next_queued` |
| REQ-014 | M1 | `crates/infra/tests/m1.rs::req_014_an_expired_lease_is_reclaimed_by_another_worker` | `crates/infra/src/stores/runs.rs::claim_next_queued` |
| REQ-015 | M2 | — | — |
| REQ-016 | M2 | — | — |
| REQ-017 | M2 | — | — |
| REQ-018 | M8 | — | — |
| REQ-019 | M2 | — | — |
| REQ-020 | M1 | `crates/app/src/worker.rs::req_020_the_prompt_is_the_last_thing_the_person_wrote` | `crates/app/src/worker.rs`, `crates/api/src/realtime.rs` |
| REQ-021 | M1 | `crates/testkit/src/build.rs::req_021_a_run_that_fails_carries_a_legible_reason` | `crates/app/src/worker.rs::fail_run`, `crates/app/src/service.rs::Failure` |
| REQ-022 | M3 | — | — |
| REQ-023 | M3 | — | — |
| REQ-024 | M3 | — | — |
| REQ-025 | M3 | — | — |
| REQ-026 | M4 | — | — |
| REQ-027 | M4 | — | — |
| REQ-028 | M4 | — | — |
| REQ-029 | M4 | — | — |
| REQ-030 | M4 | — | — |
| REQ-031 | M4 | — | — |
| REQ-032 | M5 | — | — |
| REQ-033 | M5 | — | — |
| REQ-034 | M5 | — | — |
| REQ-035 | M5 | — | — |
| REQ-036 | M6 | — | — |
| REQ-037 | M6 | — | — |
| REQ-038 | M6 | — | — |
| REQ-039 | M6 | — | — |
| REQ-040 | M1 | `crates/api/tests/rpc_surface.rs::req_040_the_capabilities_route_reports_the_sign_in_methods` | `crates/api/src/authcap.rs`, `crates/app/src/accounts.rs` |
| REQ-041 | M1 | `crates/api/tests/rpc_surface.rs::req_041_a_sign_in_sets_a_session_cookie_that_carries_no_token_in_the_body` | `crates/api/src/auth.rs`, `crates/api/src/authroutes.rs` |
| REQ-042 | M1 | `crates/api/tests/rpc_surface.rs::req_042_creating_a_space_makes_the_caller_its_owner` | `crates/app/src/usecases.rs::create_space` |
| REQ-043 | M1 | `crates/testkit/src/memory.rs::req_043_a_member_context_cannot_read_another_tenants_bot` | `crates/infra/src/tenant.rs`, `crates/app/src/usecases.rs::resolve_actor` |
| REQ-044 | M9 | — | — |
| REQ-045 | M9 | — | — |
| REQ-046 | M5 | — | — |
| REQ-047 | M7 | — | — |
| REQ-048 | M7 | — | — |
| REQ-049 | deferred | — | — (COULD; recorded in `docs/STATE.md`) |
| REQ-050 | M7 | — | — |
| REQ-051 | M8 | — | — |
| REQ-052 | M8 | — | — |
| REQ-053 | M0 | `crates/api/tests/http_surface.rs::req_053_health_is_live_even_when_every_dependency_is_down` | `crates/api/src/health.rs`, `crates/app/src/health.rs` |
| REQ-054 | M0 | `crates/api/tests/http_surface.rs::req_054_every_health_answer_carries_a_request_id_and_no_store` | `crates/api/src/error.rs` |
| REQ-055 | M10 | — | `apps/web` |
| REQ-056 | deferred | — | — (SHOULD; recorded in `docs/STATE.md`) |
| REQ-057 | M1 (deferred to the UI slice) | — | `apps/web/lib/i18n.ts` (not built yet) |
| REQ-058 | M9 | `crates/bin/src/cli.rs::req_058_every_job_has_a_command_line` | `crates/bin/src/cli.rs` |

## Notes

- A row with an em dash in the *Test* column means the requirement has no test yet; the
  requirement is covered by the next milestone named in the *Milestone* column.
- The RLS proof (REQ-043) has two halves: the unit test above proves the *repository* refuses a
  cross-tenant read, and `crates/infra/tests/` proves the *database* refuses it (the policy is in
  the migration, and the row is added there when the integration test lands).
- The golden fixtures (`spec/golden/`) are replayed by `crates/conformance/tests/golden_api.rs`;
  each replayed fixture is an additional piece of evidence for the REQ the manifest names.
