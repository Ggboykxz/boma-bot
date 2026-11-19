---
swarm: 2026-11-18-001
worker: rust
weight: 1.0
choice: B
confidence: 0.7
---

## Vote: B — board/triage upgrade (REQ-036/038)

### Backend / contract rationale

The five votes for A all say "no new transport". That is false from the Rust side. `Block::Card` (`crates/domain/src/message.rs:87`) carries only `card_id` and `kind`; `create_waiting_card` (`usecases.rs:6834`) writes exactly those two. An effect · reason · reversal summary and a durable answered state are not on the messages wire — the detail lives, at best, in `NewPendingToolCall.arguments`. So A needs new wire fields (`#[serde(default)]` under the workspace's strict `deny_unknown_fields` policy), a spec/03 + openapi edit, a golden refresh, an ADR and a DEVIATION entry in docs/PARITY.md with owner approval before shipping. The answered-state fix that STATE.md already records as an open spec question likely also needs a denormalised status column — an additive migration plus backfill. A is not a quick next slice. C has zero Rust surface (`User.locale` is a free string; there is no catalogue list to count), but the number is contested — REQ-057 says 14, `04-nonfunctional.md:113`/TRACE say 15 — and DEC-11 ("Locales to ship") is still open and explicitly blocks the translation pipeline. A count test would enshrine a disputed value before the owner decides.

B is the only honest additive choice. `boards.get`, `tickets.create|move|comment` and `TicketView.state` (`api/src/wire.rs:1059`) already exist; saved filters add one RLS table, a Store port with Postgres + in-memory doubles, two procedures and a spec/03 entry — no wire break, no existing golden change, no deviation. Status counts derive from the existing ticket list. REQ-036/038 already have `req_038_the_state_vocabulary_round_trips` and `req_038_tickets_round_trip_through_postgres`, so `trace --max-missing 7` stays green. Keep the flag-off 404 test green: the COULD/feature-gated contract guarantee is the easy thing to break here. CONFIRMED by reading the crates; space-vs-user filter scope is a GUESS for the ADR.

### Wire / migration by option

- **A:** new card wire fields + spec/golden/ADR/deviation; migration only if answered status is stored.
- **B:** additive `board_filters` migration (RLS on `space_id`), additive RPC; all existing readers keep working.
- **C:** none in Rust; client-only catalogue assertion.

### First REQ-named Rust test

`req_038_a_saved_board_filter_round_trips_through_postgres_and_is_space_scoped` in `crates/infra/tests/m1.rs`, preceded by the domain test `req_038_a_saved_filter_name_is_printable_and_bounded` in `crates/domain/src/ticket.rs`.
