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
| REQ-003 | M2 | `crates/testkit/src/build.rs::req_003_a_message_during_an_active_run_joins_it`, `crates/testkit/src/build.rs::req_003_the_worker_folds_a_steering_message_in_before_it_answers`, `crates/domain/src/run.rs::req_003_non_conversational_triggers_never_take_steering` | `crates/domain/src/run.rs`, `crates/app/src/usecases.rs::send_message`, `crates/app/src/worker.rs::provider_pass`, `migrations/0003_steering.sql` |
| REQ-004 | M1 | `crates/api/tests/rpc_surface.rs::req_004_creating_a_bot_creates_its_thread` | `crates/app/src/usecases.rs::create_bot` |
| REQ-005 | M1 | `crates/api/tests/rpc_surface.rs::req_005_the_message_page_is_newest_first_and_carries_a_cursor` | `crates/app/src/usecases.rs::page_messages`, `crates/domain/src/message.rs` |
| REQ-006 | M2 | `crates/testkit/src/build.rs::req_006_a_stopped_run_absorbs_nothing_else`, `crates/api/tests/rpc_surface.rs::req_006_a_cancel_stops_the_run_and_is_idempotent` | `crates/app/src/usecases.rs::cancel_run`, `crates/api/src/rpc.rs::runs_cancel`, `crates/app/src/worker.rs` |
| REQ-007 | M2 | `apps/web/tests/golden.spec.ts` (UI golden REQ-007-pending-card) | `apps/web/components/thread/message-blocks.tsx`, `apps/web/tests/fixtures.ts::SEED_CARD_MESSAGE` |
| REQ-008 | M2 | `crates/testkit/src/build.rs::req_008_a_reply_carries_the_message_it_answers_and_the_quote`, `apps/web/tests/reply.spec.ts`, `crates/api/tests/rpc_surface.rs::req_008_reacting_to_a_message_reads_back_the_truth`, `crates/infra/tests/m1.rs::req_008_reactions_round_trip_in_postgres`, `apps/web/tests/reactions.spec.ts` | `crates/domain/src/message.rs`, `crates/app/src/ports.rs` (NewMessage, ReactionStore), `crates/app/src/usecases.rs::send_message`, `crates/app/src/usecases.rs::react_to_message`, `crates/infra/src/stores/reactions.rs`, `migrations/0007_replies.sql`, `migrations/0009_reactions.sql`, `apps/web/components/thread/thread-screen.tsx`, `apps/web/components/thread/thread-composer.tsx`, `apps/web/lib/api/rpc.ts` |
| REQ-009 | M2 | `crates/api/tests/rpc_surface.rs::req_009_head_and_marks_move_the_read_position`, `crates/api/tests/rpc_surface.rs::req_009_the_roster_snooze_and_unsnooze_round_trip`, `crates/infra/tests/m1.rs::req_009_the_read_position_round_trips_in_postgres`, `crates/infra/tests/m1.rs::req_009_the_roster_reads_unread_and_snooze_in_postgres`, `crates/infra/tests/tenancy.rs::req_009_a_read_position_cannot_cross_tenants`, `crates/infra/tests/tenancy.rs::req_009_a_snooze_cannot_cross_tenants`, `apps/web/tests/golden.spec.ts` (UI golden REQ-009-unread-divider), `apps/web/tests/snooze.spec.ts` | `migrations/0010_read_states.sql`, `migrations/0011_snoozes.sql`, `crates/app/src/ports.rs` (ReadStateStore, RosterStore, BotSummary), `crates/infra/src/stores/read_states.rs`, `crates/infra/src/stores/roster.rs`, `crates/app/src/usecases.rs::thread_head`, `crates/app/src/usecases.rs::list_bots`, `crates/api/src/rpc.rs::threads_head`, `crates/api/src/rpc.rs::bots_list`, `apps/web/components/thread/thread-screen.tsx`, `apps/web/components/shell/bot-rail.tsx` |
| REQ-010 | M2 | — | — |
| REQ-011 | M1 | `crates/api/src/wire.rs::req_011_a_run_always_carries_its_empty_fields` | `crates/app/src/usecases.rs::list_runs` |
| REQ-012 | M1 | `crates/testkit/src/memory.rs::req_012_two_claims_never_return_the_same_run`, `crates/infra/tests/m1.rs::req_012_the_claim_moves_a_run_to_leased_and_the_fence_guards_the_write`, `crates/infra/tests/m1.rs::req_012_a_failed_run_is_retried_once_in_postgres`, `crates/api/tests/rpc_surface.rs::req_012_retry_only_from_failed`, golden `run-retry` | `crates/app/src/usecases.rs::claim_run`, `crates/app/src/usecases.rs::retry_run`, `crates/app/src/ports.rs` (RunStore::retry), `crates/infra/src/stores/runs.rs`, `crates/testkit/src/memory.rs::MemoryRunStore`, `crates/domain/src/run.rs`, `migrations/0012_attempt.sql` |
| REQ-013 | M1 | `crates/testkit/src/memory.rs::req_013_an_expired_lease_is_reclaimed_after_a_deploy` | `crates/infra/src/stores/runs.rs::claim_next_queued`, `crates/testkit/src/memory.rs::MemoryRunStore::claim_next_queued` |
| REQ-014 | M1 | `crates/infra/tests/m1.rs::req_014_an_expired_lease_is_reclaimed_by_another_worker` | `crates/infra/src/stores/runs.rs::claim_next_queued` |
| REQ-015 | M2 | `crates/domain/src/model.rs::req_015_a_pinned_model_the_deployment_cannot_reach_is_a_visible_failure`, `crates/testkit/src/build.rs::req_015_a_pinned_model_the_deployment_cannot_reach_fails_visibly` | `crates/domain/src/model.rs`, `crates/app/src/usecases.rs::send_message`, `migrations/0006_model.sql` |
| REQ-016 | M2 | `crates/testkit/src/build.rs::req_016_the_catalogue_carries_only_granted_tools`, `crates/testkit/src/build.rs::req_016_the_worker_asks_with_only_granted_tools` | `crates/app/src/usecases.rs::tool_catalogue`, `crates/infra/src/stores/tools.rs`, `migrations/0005_tools.sql` |
| REQ-017 | M2 | `crates/testkit/src/build.rs::req_017_answering_a_card_records_the_effect_exactly_once`, `crates/api/tests/rpc_surface.rs::req_017_answering_a_card_resumes_the_run_and_is_refused_twice`, `crates/infra/tests/m1.rs::req_017_answer_a_card_and_record_its_effect_once_in_postgres`, golden `thread-answer` | `crates/app/src/usecases.rs::answer_card`, `crates/api/src/rpc.rs::threads_answer`, `crates/infra/src/stores/cards.rs`, `migrations/0004_cards.sql` |
| REQ-018 | M8 | — | — |
| REQ-019 | M2 | `crates/testkit/src/build.rs::req_019_delegate_helper_run_creates_a_child_run` | `crates/app/src/usecases.rs::delegate_helper_run`, `crates/domain/src/run.rs::is_helper`, `migrations/0008_helper_runs.sql`, `crates/app/src/usecases.rs::write_reply` (helper run writes role `tool`) |
| REQ-020 | M1 | `crates/app/src/worker.rs::req_020_the_prompt_is_the_last_thing_the_person_wrote` | `crates/app/src/worker.rs`, `crates/api/src/realtime.rs` |
| REQ-021 | M1 | `crates/testkit/src/build.rs::req_021_a_run_that_fails_carries_a_legible_reason` | `crates/app/src/worker.rs::fail_run`, `crates/app/src/service.rs::Failure` |
| REQ-022 | M3 | `crates/api/tests/rpc_surface.rs::req_022_memory_write_grows_the_revision`, `crates/api/tests/rpc_surface.rs::req_022_memory_scope_rules_and_removal`, `crates/infra/tests/m1.rs::req_022_memory_revisions_are_append_only_in_postgres`, golden `memory-write` | `crates/domain/src/memory.rs`, `migrations/0013_memory.sql`, `crates/app/src/ports.rs` (MemoryStore, NewMemoryDocument), `crates/app/src/usecases.rs::memory_write`, `crates/infra/src/stores/memory.rs`, `crates/api/src/rpc.rs::memory_write`, `crates/infra/src/stores/rows.rs` (MemoryDocumentRow) |
| REQ-023 | M3 | `crates/api/tests/rpc_surface.rs::req_023_a_created_routine_wakes_on_its_next_instant_the_wire_agrees_on`, `crates/domain/src/routine.rs::req_023_*`, goldens `routine-create`, `routine-once` | `migrations/0014_routines.sql`, `crates/domain/src/routine.rs`, `crates/app/src/ports.rs` (RoutineStore), `crates/infra/src/stores/routines.rs`, `crates/api/src/rpc.rs::routines_*`, `crates/conformance/src/golden_world.rs` |
| REQ-024 | M3 | `crates/testkit/src/build.rs::req_024_a_due_routine_fires_into_a_routine_run`, `crates/testkit/src/build.rs::req_024_a_once_routine_fires_once_and_deactivates_itself`, `crates/testkit/src/build.rs::req_024_a_coalesced_routine_records_the_skip_instead_of_stampeding`, `crates/testkit/src/build.rs::routine_retry_tests::req_024_a_retryable_failure_backs_off_before_it_counts`, `crates/testkit/src/build.rs::routine_retry_tests::req_024_a_pausing_routine_tells_the_owner_in_the_thread`, `crates/infra/tests/m1.rs::req_024_a_due_routine_is_claimed_and_a_once_fire_deactivates_postgres`, `crates/infra/tests/m1.rs::req_024_a_backoff_queued_run_waits_for_its_window_in_postgres`, `crates/domain/src/run.rs::req_024_the_retry_delay_ladder_doubles_and_caps` | `crates/app/src/usecases.rs::dispatch_routines`, `crates/app/src/usecases.rs::auto_retry_routine_run`, `crates/app/src/usecases.rs::record_routine_completed`, `crates/app/src/usecases.rs::record_routine_failed`, `crates/app/src/worker.rs`, `crates/app/src/error.rs::is_retryable`, `crates/domain/src/run.rs` (`retry_delay_seconds`, `MAX_ATTEMPTS`), `crates/app/src/ports.rs` (RoutineStore::record_fired/record_failure/record_skip/record_completed, RunStore::find_open_routine), `crates/infra/src/stores/routines.rs`, `crates/infra/src/stores/runs.rs` (claim honours `not_before`; create persists `routine_id`), `migrations/0016_run_not_before.sql`, `crates/testkit/src/memory.rs` |
| REQ-025 | M3 | `crates/api/tests/rpc_surface.rs::req_025_an_on_demand_trial_runs_the_routine_as_a_normal_turn` | `crates/app/src/usecases.rs::routines_run_now`, `crates/api/src/rpc.rs::routines_run_now`, `crates/app/src/worker.rs` (routine prompt on routine-woken runs) |
| REQ-026 | M4 | `crates/testkit/src/build.rs::req_026_a_computer_boots_lazily_and_is_reused`, `crates/testkit/src/build.rs::req_026_the_idle_sweeper_releases_a_computer_past_its_window`, `crates/testkit/src/build.rs::req_026_a_scripted_runtime_that_cannot_deliver_answers_unavailable`, `crates/api/tests/rpc_surface.rs::req_026_a_bot_asks_for_a_computer_is_answered_and_the_answer_holds`, `crates/infra/tests/computers.rs::req_026_a_computer_boots_stops_and_its_home_survives_in_docker` (real daemon), golden `computer-status` | `crates/domain/src/computer.rs`, `migrations/0017_computers.sql`, `crates/app/src/ports.rs` (ComputerRuntime, ComputerStore), `crates/app/src/usecases.rs` (ensure_computer, stop_computer, sweep_idle_computers), `crates/app/src/service.rs` (Stores::computers), `crates/worker`+`crates/bin/src/commands.rs` (sweeper), `crates/infra/src/computer.rs` (Docker supervisor), `crates/infra/src/stores/computer.rs`, `crates/testkit/src/memory.rs`, `crates/conformance/src/golden_world.rs` (computer-status fixture) |
| REQ-027 | M4 | `crates/bin/src/computer.rs::req_027_*`, `crates/api/tests/rpc_surface.rs::req_027_a_deployment_without_a_computer_backend_answers_computers_unavailable`, `crates/api/tests/rpc_surface.rs::req_027_a_bot_with_no_computer_answers_stopped_never_an_error` | `crates/bin/src/computer.rs` (fail-closed selection), `crates/bin/src/config.rs` (SANDBOX_PROVIDER/NOVITA_API_KEY/DOCKER_HOST/limits), `crates/bin/src/commands.rs` (startup ping), `crates/infra/src/computer.rs` (bollard client), `crates/app/src/ports.rs::ComputerRuntime`, `crates/api/src/error.rs::ComputersUnavailable` |
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
