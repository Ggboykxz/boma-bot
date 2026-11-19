# v0.4.0 architecture proposal — `crates/infra/src/stores/mail.rs` (mail store / held intake "outbox")

Worker #351 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Claim status: every "current" statement is CONFIRMED by direct reads in this worktree of
`mail.rs`, `crates/app/src/ports.rs:3308-3424`, `crates/app/src/usecases.rs:2160-2580`,
`crates/infra/src/tenant.rs`, `migrations/0040_bot_mail.sql`, `0041`, `0051`,
`crates/testkit/src/memory.rs:7137-7360`, `crates/infra/tests/tenancy.rs:284`.
Forward-looking items are proposals (GUESS until implemented and verified). English per disk
convention; no gate weakened; no wire change is proposed.

## Current structure (CONFIRMED)

- `PostgresMailStore` is a stateless adapter over one `PgPool` implementing the six-method
  `MailStore` port (`rule`, `set_rule`, `hold`, `held_list`, `take_held`, `sender_is_member`).
  A mirror `MemoryMailStore` implements the same port over `Arc<RwLock<State>>`.
- Every query runs through `tenant::with_tenant`: one transaction, `set_config(..., true)` for
  `boma.space_id` / `boma.system_reason`, commit on success, rollback on error. RLS `FORCE` on
  both tables is the isolation boundary; a System context bypasses it via `boma.is_system()`.
- Two tables (0040): `bot_mail_rules` (bot_id PK, space_id, policy CHECK hold|drop,
  allowed_senders jsonb, updated_at) and `held_mail` (id PK, space_id, bot_id, sender, subject,
  body, reason, created_at; index on space_id, bot_id, created_at DESC). Held mail is a durable
  intake queue, not an outbox: `take_held` is `DELETE … RETURNING` used for *both* release and drop.
- System paths are trusted: `hold` takes `space_id` from the caller's `NewHeldMail`, and `rule`
  selects by `bot_id` alone under a System context. The relay therefore asserts the bot/space
  pairing; the database never verifies it.
- `set_rule` is the only two-transaction method: upsert, then a fresh `self.rule(...)` read.
  A failure between them reports `Unavailable` although the write committed.
- Ports use primitive `String` for ids, sender, policy and RFC3339 timestamps (`boma.rfc3339`);
  the domain newtypes (`BotId`, `SpaceId`, `Email`) stop at the use-case boundary.
- `held_list` hardcodes `LIMIT 50` with no cursor/count and returns full bodies. `sender_is_member`
  matches `lower(u.email)`, though 0051 normalized stored emails to lowercase.
- No observability beyond `tenant::unavailable`'s warn, and no `#[cfg(test)]` module in this file;
  tenancy is covered externally (`tenancy.rs:284`, `m1.rs:2919`). The allowlist decoder silently
  drops non-string JSON entries via `filter_map(as_str)`.

## Guiding principle for v0.4.0

Make each store call one transaction; make the held queue a claim/ack outbox so consuming it is
atomic with the run it starts; push tenant ownership and types to the boundary. All items stay
inside infra/app/testkit — if any reaches the wire, record a DEVIATION in `docs/PARITY.md` first.

## Upgrades (5 concrete items, with risks)

1. **One-shot `set_rule` with `RETURNING`.** Fold the upsert and re-read into one statement that
   returns `boma.rfc3339(updated_at)` plus policy/senders, mapped straight to `MailRule`.
   *Risk:* tuple/format drift against `rule()`; a System-context caller currently re-reads under
   the same context, so behavior must stay identical. *Mitigation:* integration test asserting the
   returned rule equals a subsequent `rule()` read (including no-row default); no wire change.

2. **Held mail as a real outbox (the central reliability fix).** New migration adds
   `status` ('held'|'releasing'|'released'|'dropped'), `claimed_at`, `attempts` to `held_mail`.
   Replace destructive `take_held` with `claim_held` (`SELECT … FOR UPDATE SKIP LOCKED`, mark
   `releasing`) plus `ack_held` / `fail_held` after the downstream message + run commit; keep a
   `drop_held` terminal path. Use the held id as the existing message/run `client_nonce` so a
   retried claim cannot start two runs (usecases.rs:2495-2567 currently deletes first, then does
   two unrelated transactions — a crash loses the mail).
   *Risk:* new port methods ripple to `MemoryMailStore` and mocks; a lost ack could retain a row or
   double-start a run; a status CHECK must preserve old rows. *Mitigation:* idempotency via nonce,
   a `DEFAULT 'held'` + `USING` backfill, bounded `attempts`, and a crash-injection test. Internal
   only: keep the held-list/release/drop RPC shapes unchanged.

3. **Verify bot/space ownership instead of trusting the relay.** Enforce a composite
   `FOREIGN KEY (space_id, bot_id) REFERENCES bots(id, space_id)` (add the matching unique index)
   and bind `space_id` explicitly in `rule()` for System callers, so a System `hold` cannot file
   mail under an arbitrary space. *Risk:* extra index/FK maintenance on bots deletes; a hot-path
   extra round trip if done as a separate query. *Mitigation:* prefer the composite FK (one write,
   no extra query), keep the existing cascades aligned, and extend the `req_043` tenancy test with
   a mismatched (space, bot) `hold` that must fail.

4. **Typed port boundary.** Replace `String` ids/sender/policy with `BotId`, `SpaceId`, `Email`
   and a closed `MailPolicy` enum; keep RFC3339 strings only where the wire requires them.
   *Risk:* broad churn through `ports.rs`, `MemoryMailStore` and api DTOs; an enum's serde output
   could change JSON (DEVIATION). *Mitigation:* convert at the api boundary so the JSON stays
   `{"policy":"hold"|"drop"}`, land `BotId`/`SpaceId` first, keep the DB CHECK in sync with the
   enum, and re-run golden/contract suites after each step.

5. **Bounded reads, telemetry and local tests.** Parameterize `held_list` with a capped page size
   and cursor instead of `LIMIT 50`; emit `tracing` events for hold/claim/release/drop with
   `bot_id`, `space_id` and decision only (never sender, subject or body — REQ-054); add a
   `#[cfg(test)]` module for allowlist JSON decoding and a proptest for sender normalization.
   *Risk:* a page-size default changes only internal callers if capped in the use case; logging
   personal data is a compliance defect. *Mitigation:* ids-only log fields, keep ordering stable,
   and add a golden test for list ordering and page bounds.

## Cross-cutting risks / sequencing

- Sequence 5 (cheap safety net) → 1 → 3 → 4 → 2. Item 2 depends on the port/types settled in 4
  and is the only change with a new migration; land it last behind the full suite.
- Keep `policy IN ('hold','drop')`, the 20-sender use-case limit and any JSON schema identical in
  migration, domain and wire; today the CHECK is the only DB-side guard (GUESS: confirm no other
  writer exists before tightening).
- No secrets live here; never log message bodies or addresses. `boma.system_reason` remains a
  closed enum — any new System reason for mail claiming is a code change, not free text.
- Gate per step: `cargo fmt --check`, `clippy -D warnings`, `cargo test --locked` (infra
  tenancy/m1 + testkit), conformance golden and OpenAPI contract suites.

## Out of scope

Relay/webhook ingress (`mail_inbound`), membership source, REQ-048 messaging store, and api wire
DTOs belong to other modules; this proposal only reshapes the mail adapter, its port and schema.

## Evidence

CONFIRMED — `mail.rs:1-238`; `ports.rs:3308-3424`; `usecases.rs:2160-2580`;
`tenant.rs:1-160`; `0040_bot_mail.sql`, `0041_routines_email_senders.sql`,
`0051_lowercase_users_email.sql`; `memory.rs:7137-7360`; `tenancy.rs:284`; `m1.rs:2919`.
GUESS — that a DELETE-before-run crash actually loses mail in production (inferred from the three
separate transactions; no crash reproduction run); that `lower(u.email)` is currently unindexed.
