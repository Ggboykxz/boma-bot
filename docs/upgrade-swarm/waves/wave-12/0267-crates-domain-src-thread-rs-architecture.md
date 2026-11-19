# 0267 — `crates/domain/src/thread.rs` — architecture lens — v0.4.0 proposal

- Worker #267 · Lens: architecture · Target: v0.4.0 · Read-only review; no product code edited.
- Module: `crates/domain/src/thread.rs` (75 lines) at `before-m5` (v0.3.0-rc5).
- Contract read for context: `spec/02-data-model.md` (Thread row, thread types),
  `spec/01-requirements.md` REQ-004/005/048, `spec/06-decisions-needed.md`,
  `spec/05-known-defects.md`; consumers `crates/domain/src/lib.rs:42,107`,
  `crates/domain/src/group.rs`, `crates/domain/src/compaction.rs`,
  `crates/domain/src/ids.rs`, `crates/infra/src/stores/threads.rs:35,61`,
  `crates/infra/src/stores/rows.rs:302-314`, `crates/infra/src/stores/messages.rs:52`,
  `crates/infra/src/stores/events.rs:45`, `crates/app/src/ports.rs:126`,
  `crates/app/src/usecases.rs:4513,6387`, `crates/testkit/src/memory.rs`,
  `migrations/0002_m1.sql:155`, `migrations/0042_messaging.sql:13-20`.
- Every "current state" claim below is CONFIRMED by direct reads; forward-looking items are
  proposals (GUESS until implemented, tested and approved).

## What the thread domain model is today (CONFIRMED)

Two data structs (`ExternalThread`, `Thread`), two paging constants and one validator
(`MAX_PAGE_SIZE=100`, `DEFAULT_PAGE_SIZE=50`, `validate_page_size`), and one unit test. The module
is a pure data bag: it owns no constructor and no invariant method, and no domain code advances the
sequences — the store allocates them in SQL with `UPDATE ... RETURNING next_*_seq - 1`
(`stores/messages.rs:52`, `stores/events.rs:45`).

Architecture observations, ordered by severity:

1. **The thread "kind" is a split brain the domain cannot express.** The spec row is
   `botId ⊕ groupId ⊕ externalConversationId` — "exactly one of bot/group/external" — plus
   `historyCompactedUpToSeq`. The DB has a `kind` column (`bot|external`, `0042_messaging.sql:14`)
   and `group.rs` creates group threads, but the domain `Thread` has a mandatory `bot_id: BotId`
   and an `external: Option<ExternalThread>` and no kind, no group id and no compaction watermark.
   Invalid states are representable (bot + external both set; a group thread with no group id), and
   `rows.rs:312` decodes only `kind == "external"`, so a `kind = 'group'` row silently becomes an
   ordinary bot thread. The discriminator lives in three places (DB text, row decoder, domain
   `Option`) with no single source of truth.
2. **`created_at: String` and bare `i64` sequences say nothing.** The monotonic-per-thread rule
   (REQ-005/020) is carried by two unlabelled `i64` fields with no domain guard; testkit resets
   them with ad-hoc `if < next` logic. The timestamp is an unconstrained string, as in `external.rs`
   — a parsed, typed instant is the AGENTS.md "make invalid states unrepresentable" direction.
3. **Paging policy is parked in the wrong module.** `validate_page_size` / the two constants are a
   cross-cutting pagination concern imported as `boma_domain::thread::validate_page_size` by two
   distant use cases (`usecases.rs:4513,6387`). It shares no state with `Thread`, and "page" here
   collides conceptually with `page.rs` (bot-built documents, REQ-037) — two different meanings.
4. **`ExternalThread` couples the domain to `messaging::ChannelPlatform`** and carries raw
   `workspace_id`/`conversation_id: String` (empty for some platforms — see worker #247). It is
   derived from `kind` at the row boundary, so `Option<ExternalThread>` duplicates the discriminator.
5. **Behavioral invariants live outside the model.** REQ-004 "one continuous thread per bot" is
   enforced only by the infra query (`threads.rs:61`: `kind='bot' ORDER BY created_at LIMIT 1`) and
   app logic; the domain neither states nor checks it. Only one test exercises pagination, none the
   thread shape/invariant, and there is no proptest.

Pinned for v0.4.0 (do not break without a DEVIATION): the four public re-exports in `lib.rs:107`,
the exact `InvalidPageSize { size, max }` error shape, `boma.rfc3339(created_at)` row encoding, and
the wire/DB names `kind`, `external_platform`, `external_workspace_id`, `external_conversation_id`.

## Guiding principle for v0.4.0

Keep `thread.rs` IO-free and dependency-light, but make it the single owner of the thread *shape*:
one typed discriminator makes the "exactly one of" invariant unrepresentable-to-violate, and the
domain speaks in types rather than raw strings/ints. Sequence allocation stays in the store (it
needs a transaction) — the domain only names and validates. Any persisted/wire-visible change is a
DEVIATION (AGENTS.md): record it in `docs/PARITY.md` and wait for approval before shipping.

## Concrete upgrades, each with a risk

1. **Introduce a typed `ThreadKind` sum and derive `external` from it.** Replace the mandatory
   `bot_id` + `Option<ExternalThread>` with `kind: ThreadKind { Bot { bot_id }, Group { group_id },
   External(ExternalThread) }`, making the exclusive-or structural; add `history_compacted_up_to_seq`
   to match the spec row, and give `rows.rs` one exhaustive decoder for `kind`.
   *Risk:* this is the widest change — public field layout (lib.rs:107), `NewThread`, row decode,
   every `Thread { .. }` literal across testkit/infra/tests, and group-thread behavior all shift at
   once; adding `group` decoding changes what a stored group row means. *Mitigation:* stage it —
   land `ThreadKind` with a `From<&ThreadRow>` that keeps today's decode byte-for-byte, migrate
   call sites, then switch the decoder; keep the four re-exports as thin aliases where possible and
   file a DEVIATION for any externally visible field change.

2. **Name the sequences and the time.** Add a `MessageSeq`/`EventSeq` newtype (or one `ThreadSeq`
   parameterised by purpose) checked against the store's `RETURNING` boundary, and a parsed
   timestamp type for `created_at` that formats back to the same RFC3339 string.
   *Risk:* a newtype must still coerce to `i64` at SQL bind sites and to the golden's exact
   `createdAt` bytes; a timestamp dependency needs an ADR + `docs/DEPENDENCIES.md` row, and touching
   the field layout is a schema/wire DEVIATION. *Mitigation:* keep serde representations identical
   (transparent newtypes), add a round-trip test against `boma.rfc3339(...)`, and prefer a
   dependency-free RFC3339 wrapper if the crate already has one.

3. **Move pagination policy to a dedicated `pagination` module.** Relocate the constants and
   `validate_page_size` out of `thread.rs` (re-export from `thread` for one release to avoid a hard
   break), documenting the "page of history" vs `page.rs` "bot-built page" distinction.
   *Risk:* import churn in `usecases.rs` and a possible public-path change for downstream users;
   a naive re-export removal breaks the contract. *Mitigation:* keep `boma_domain::thread::*`
   re-exports as aliases, update in-repo callers to the new path, and add a test that both paths
   return the identical error.

4. **Add domain constructors and invariant tests for the thread shape.** Give `Thread`/`NewThread`
   a validating constructor (or `TryFrom`) that enforces tenant/id presence and the kind structure,
   plus proptest coverage that no arbitrary row decodes into an invalid thread and that paging
   validation holds at the boundaries (0, 1, 50, 100, 101).
   *Risk:* constructors add surface area and can be bypassed by public struct literals unless fields
   are made private — which is itself a breaking API change; proptest needs row shapes that mirror
   SQL constraints. *Mitigation:* keep fields public this release but add tests that assert the
   decoder + constructor agree, and reserve field privacy for the follow-up that lands with the
   kind migration (upgrade 1).

## Traceability / verification note (GUESS until run)

Each upgrade must add REQ-tagged tests (`REQ-004` continuity/kind, `REQ-005` seq + paging,
`REQ-048` external decode) per the repo's trace-check rule, then `cargo fmt --check`, clippy and the
unit/golden suites; any changed golden byte or DB/wire field goes to `docs/PARITY.md` for approval.
