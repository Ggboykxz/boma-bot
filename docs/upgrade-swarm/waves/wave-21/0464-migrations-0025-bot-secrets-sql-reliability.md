# 0464 — `migrations/0025_bot_secrets.sql` — reliability proposal (v0.4.0)

Worker #464 · lens: reliability · READ-ONLY review of `.worktrees/before-m5`. Proposals only; no code
changed. **CONFIRMED** = read in this tree this session; **GUESS** = inference needing a runtime test.
Anchors: `0025_bot_secrets.sql:1-34`, `0026_space_keys.sql:7-30`, `0002_m1.sql:27-42`
(`current_space_id`, `is_system`), `crates/infra/src/stores/secrets.rs:39-216`, `secret_box.rs`
(96-bit nonce / GCM), `docs/adr/020-secrets.md`, `spec/01-requirements.md:373-379` (REQ-046).
**Additive only:** every item below is a later migration / app change; this file is untouched.

## Scope note

One table (`:7-17`) — `bot_secrets(id, space_id, bot_id, name, ciphertext, nonce, created_at)` with
two cascading FKs, a `name` regex CHECK, and `UNIQUE (space_id, bot_id, name)`. A `DO $$ FOREACH`
loop (`:19-34`) enables + forces RLS and installs one policy keyed on
`space_id = boma.current_space_id() OR boma.is_system()`; `current_space_id()` is NULL with no tenant
set, so the policy fails closed (`0002_m1.sql:27-31`). The DEK lives in `space_keys` (0026), wrapped
under `BOMA_MASTER_KEY`; the app derives `space_id` from `bots` on every put/list/delete/reveal
(`secrets.rs:48-57,134-135,165,194`), so RLS is defence in depth and SQL predicates defence in fact.

## What is already right (keep)

- FORCE RLS + a closed-form policy: no tenant context ⇒ NULL ⇒ no rows; only an explicit
  `boma.system_reason` crosses spaces. CONFIRMED.
- Both FKs are `ON DELETE CASCADE`: deleting a bot or space leaves no orphan secret row. CONFIRMED.
- `created_at timestamptz NOT NULL DEFAULT now()` and the `UNIQUE` triple give a stable write key
  that the app uses as an upsert target (`secrets.rs:100-105`). CONFIRMED.
- The leftmost prefix of `UNIQUE (space_id, bot_id, name)` covers list/reveal/delete lookups, so no
  extra index is needed for today's queries. CONFIRMED.
- Per-space DEK (ADR-020): a row copied to another space cannot authenticate on reveal, so a
  misrouted ciphertext fails closed instead of decrypting. CONFIRMED by design; GUESS on a live test.

## Failure-mode inventory

F1 — No shape check on the encrypted payload (CONFIRMED). `ciphertext` and `nonce` are bare `bytea
NOT NULL`; any length, including empty, is accepted. `secret_box` seals with a 96-bit (12-byte)
nonce and a 16-byte GCM tag, but a corrupt row only fails later in `open_with_data_key`
(`secrets.rs:211-214`), surfacing as `Unavailable` (a fake dependency outage) on a legitimate run.
A multi-MB bytea also loads whole into `reveal`, then is exported as `BOMA_SECRET_<NAME>` into the
computer-shell process, where the OS per-entry env limit (MAX_ARG_STRLEN ~128 KB on Linux) rejects it
at exec time (GUESS on the exact threshold). No DB cap exists.

F2 — `space_id` is not bound to the bot's real space in the schema (CONFIRMED). The app reads
`SELECT space_id FROM bots WHERE id = $1` on every path, but nothing prevents a system-context write
(`boma.is_system()` bypasses RLS) from storing `(space_id = A, bot_id of B)`. The row is then visible
only to A while pointing at B's bot; `list`/`reveal` still find it via the bot subquery under one
tenant, so the inconsistency is silent rather than safely isolated. Same class as wave-20 #0436 F2;
a composite FK needs a redundant UNIQUE on `bots`, a trigger is cheaper.

F3 — RLS is space-level, not bot- or member-level (CONFIRMED, gap). The policy grants full DML to
any member of the space: no `user_id` or bot predicate. Correct app queries bind `bot_id`, but a
buggy/compromised query at the non-privileged role could list or delete another bot's secret in the
same space. There is no `boma.current_user_id()` helper yet (wave-19 #0412 H8), so a per-user policy
cannot be written today. FORCE also means maintenance under the table owner sees nothing without a
system reason — intended, but a trap for future data migrations.

F4 — Upsert overwrites are silent and last-write-wins (CONFIRMED). `put` uses `ON CONFLICT ... DO
UPDATE SET ciphertext, nonce, created_at = EXCLUDED...` with no version, no `updated_at`, no `WHERE`
guard (`secrets.rs:99-116`). Two members injecting a value during a run (a spec-supported flow) both
succeed; the older write wins or loses by timing, `created_at` is rewritten so list order changes,
and "never shown again" hides the clobber from the victim. No idempotency key or audit row, despite
the header's "exactly once" claim (`:5-6`) — the DB cannot tell a re-put from a first put.

F5 — Constraint violations are misclassified as outages (CONFIRMED). A bad `name` (regex CHECK), an
oversized `ciphertext`, or a bad `space_id`/`bot_id` FK raises a SQL error mapped through
`tenant::unavailable` (`secrets.rs:54,116,141,171,200`) → `Unavailable`/503. Input errors wear an
outage's costume: retry budgets burn and no 400 is returned. Same root cause as #0436 F5.

F6 — Migration mechanics (CONFIRMED). The single-element `FOREACH ... ARRAY['bot_secrets']` loop and
`format()` dynamic SQL add nothing; there is no `IF NOT EXISTS`, so a re-run against a tracked DB
fails on `CREATE TABLE`. Low severity (the tracker prevents re-runs) but relevant for disaster
recovery. Doc drift: ADR-020:45-47 says 0025 creates `space_keys`; the file creates only
`bot_secrets`, `space_keys` is in 0026 — fix the ADR, not the migration.

## Concrete hardening (prioritized, all additive)

P0 — Constrain the sealed payload. A later `ALTER TABLE ... ADD CONSTRAINT ... CHECK` for
`octet_length(nonce) = 12` and `16 <= octet_length(ciphertext) <= 65536` (cap sized to the shell env
limit; GUESS on the safe value — verify against `computer_shell` exec). Rejects corrupt rows at the
boundary instead of at reveal. Risk: a preflight query must confirm no existing row violates the new
CHECK or ALTER fails; a legitimately large secret would be refused (new 400 — DEVIATION if any
caller stores one, verify before shipping).

P0 — Bind `space_id` to the bot. `BEFORE INSERT OR UPDATE` trigger validating
`NEW.space_id = (SELECT space_id FROM bots WHERE id = NEW.bot_id)`. Risk: existing mismatched rows
(the F2 bug) must be cleaned before the trigger is added; one extra indexed lookup per put. No wire
change.

P1 — Immutable `created_at`, real `updated_at`. Add `updated_at timestamptz NOT NULL DEFAULT now()`
plus a `BEFORE UPDATE` trigger, and stop setting `created_at` in the upsert (cross-module edit in
`secrets.rs:102-105`). Add `AND bot_secrets.updated_at < EXCLUDED.updated_at` to the upsert (or a
server-side guard) so a stale concurrent write cannot clobber a newer one. Risk: app change needed;
rejecting same-instant writes changes behavior (DEVIATION) — prefer the guard over a time-based
rejection and record it in docs/PARITY.md.

P1 — Per-user RLS as defence in depth. Once `boma.current_user_id()` lands (wave-19 dependency), add
RESTRICTIVE policies `USING (user_id = boma.current_user_id() OR boma.is_system())` with matching
`WITH CHECK`; a bot-scoped predicate is not expressible without a bot→user mapping that does not
exist. Risk: maintenance paths must set a system reason; existing app queries already bind ids, so
no query change expected. No wire change.

P1 — Classify bad input at the HTTP boundary (400, not 503) and log a stable internal reason code for
GCM auth failure (space/bot id + request id, never ciphertext or plaintext — #0314 P6/P7).
Migration-adjacent only; keep the external error shape unchanged or record a DEVIATION.

P2 — Tests (integration/testcontainers, REQ-046 in the name): short/long nonce and oversized
ciphertext rejected by the CHECK; cross-space `(space_id, bot_id)` insert rejected under a member
role and by the trigger under system; cross-user access blocked by the new policy; cascade on
bot/space delete; concurrent puts keep the newer value and `created_at` stable; a rotated master still
reads old ciphertext (ADR-020 contract). Wire-shape golden unchanged.

## Open questions (route to docs/STATE.md)

- Safe max secret size for the shell env path — add a bytea cap or only the nonce CHECK?
- Does the `updated_at` + stale-write guard count as a REQ-046 behavior change needing owner approval?
- Does `boma.current_user_id()` land in v0.4.0, or is user-level RLS deferred?

Status: all findings CONFIRMED by source reading except the noted GUESS items; no runtime verification
performed (read-only worker). No file other than this proposal was written.
