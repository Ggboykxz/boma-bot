# v0.4.0 reliability proposal — `crates/infra/src/stores/notifications.rs` (worker #366, lens: reliability)

Scope: the Postgres adapter for Web Push subscriptions and per-bot policy (REQ-064). Read in this
session; claims about what the code *does* are CONFIRMED by inspection, claims about runtime impact
are GUESS unless a test is named. All hardening below is proposal-only; no code was changed.

## What already protects us (CONFIRMED)

- Every method runs inside `tenant::in_space`, which opens a transaction, sets `boma.space_id`
  locally, and commits only on `Ok`; `note_failure`'s UPDATE-then-DELETE is therefore atomic.
- All SQL is parameterized; reads/writes are scoped by `space_id` (+ `user_id`) except the delete
  noted in F3; RLS is the backstop.
- `note_success` resets `fail_count` and `subscribe` uses an UPSERT keyed on
  `(space_id, user_id, endpoint)`, so re-registration does not create duplicate rows.
- `daily_cap` defaults to 20 in both `set_policy` and `NotificationPolicy::defaults`.

## Failure modes

### 1. Cap bypass

- **C1 — TOCTOU between `count_today` and `record_send` (CONFIRMED code, GUESS impact).** The cap
  is enforced by callers that `count_today` in one transaction and `record_send` in a *different*
  one. N concurrent sends for the same `(space, user, bot)` can all observe `count < cap` before
  any log row lands, then all insert — the cap is a soft, racy limit. This store offers no atomic
  "reserve a slot" primitive.
- **C2 — asymmetric `daily_cap` clamping (CONFIRMED).** `set_policy` binds
  `u32::try_from(v).unwrap_or(i32::MAX)`, so any value above `i32::MAX` is silently *raised* to
  `i32::MAX` (effectively unlimited); `policy_into_domain` reads it back with
  `u32::try_from(row.daily_cap).unwrap_or(0)`, silently *lowering* a negative stored value to 0.
  The two failure directions disagree, and neither is surfaced to the caller.
- **C3 — day boundary and `sent_at` split (CONFIRMED).** `count_today` uses a UTC
  `date_trunc('day', ...)` window while `record_send` relies on the column default for `sent_at`.
  A send straddling midnight is counted in one day and logged in the next, so both days undercount.
- **C4 — log id is caller-supplied with a plain INSERT (CONFIRMED).** `record_send` does not use
  `ON CONFLICT`. A retried send with the same id either errors (mapped to `Unavailable`) or inserts
  a duplicate, depending on whether a PK exists; a duplicate inflates the count and pushes the user
  past the cap on the next retry.

### 2. Dedup / idempotency

- **D1 — re-subscribe resurrects dead endpoints (CONFIRMED).** The UPSERT sets `fail_count = 0`.
  Browsers typically call `subscribe` on every page load, so an endpoint deleted after three
  permanent failures is recreated and immediately reused — an endless send/fail/revive loop.
  There is no tombstone keyed on endpoint to suppress this.
- **D2 — no idempotent send marker (CONFIRMED).** `record_send` is the only "was this delivered"
  record and is not idempotent (C4). Without `ON CONFLICT DO NOTHING RETURNING ...`, a retry cannot
  tell "first record" from "already recorded", so at-least-once delivery becomes duplicate pushes.
- **D3 — policy UPSERT is idempotent by accident (CONFIRMED, benign).** `set_policy` uses
  `COALESCE` per field and a conflict target, so replay is safe; keep this pattern when touching it.

### 3. Content leakage

- **L1 — secrets live in a `Debug` domain type (CONFIRMED).** `NotificationSubscription` derives
  `Debug` and holds `auth` and `p256dh` as plain `String`. `subscription_into_domain` copies both
  into that struct, and `list()` returns it. Any `tracing::debug!("{sub:?}")` or error path that
  formats the value emits the push auth secret. Compare the care taken in `tenant::unavailable`,
  which deliberately keeps driver detail at debug only.
- **L2 — `list()` returns secrets to every caller (CONFIRMED).** The store has no "metadata-only"
  read; the API's `notifications.status` is careful (only VAPID public key), but the push path and
  any future listing endpoint receive the full secret. Secrets should be wrapped (`secrecy`) or
  split into a separate reveal method, so a listing cannot serialize them.
- **L3 — unscoped delete (CONFIRMED).** `note_failure` deletes with `WHERE id = $1` only. The
  preceding UPDATE is scoped to space+user; the DELETE is not. RLS currently makes a cross-tenant
  hit unlikely (GUESS: globally unique ids), but the row is deleted based on a caller id with no
  space/user guard — add the same predicate for defence in depth.

### 4. Zombie keys

- **Z1 — non-permanent failures never retire (CONFIRMED).** `note_failure` deletes only when
  `permanent && count >= 3`. Transient failures increment `fail_count` without bound and never
  remove the row, so dead endpoints stay in `list()` and consume send attempts and cap slots.
- **Z2 — `updated_at` is bumped on failure (CONFIRMED).** Both failure and success set
  `updated_at = now()`, so "recently touched" cannot distinguish a healthy subscription from a
  zombie; no `last_success_at` exists to age out dead keys.
- **Z3 — counter wrap hides zombies (CONFIRMED).** `subscription_into_domain` converts the i32
  `fail_count` with `unwrap_or(0)`. An overflowed/negative counter reads back as 0, making a
  persistently failing endpoint look freshly healthy.
- **Z4 — missing row reports `Ok` (CONFIRMED).** When the UPDATE matches nothing, `note_failure`
  returns `NotificationFailureOutcome::Ok`. Callers cannot tell "already deleted" from "recorded",
  so a zombie that vanished mid-flight is silently treated as success.

## Concrete hardening (proposal, with risks)

1. **Atomic cap reservation.** Add one query that inserts the log row with `ON CONFLICT DO NOTHING`
   under a unique `(id)` key and returns whether it inserted, then enforce the cap in the same
   transaction via `SELECT count(...) FOR ...`/advisory lock per `(space, user, bot, day)`.
   *Risk:* contention on a per-bot lock and a behaviour change (exactly-once vs at-least-once);
   needs a DEVIATION note if the cap becomes strict. Prefer a DB-side constraint or advisory lock
   over a Rust mutex (single-process assumption).
2. **Clamp caps explicitly.** Replace both conversions with a checked helper: reject `daily_cap >
   i32::MAX` at the API boundary and reject/store-clamp negative values; log a warning on clamp.
   *Risk:* rejecting a previously accepted value is externally visible — check spec before shipping.
3. **Fix the day window.** Compute the UTC day once in the argument and pass the same `sent_at`
   from `record_send`, or count by an explicit `day` column. *Risk:* migration/backfill needed.
4. **Idempotent `record_send`.** `INSERT ... ON CONFLICT (id) DO NOTHING RETURNING id` and expose
   the inserted flag so callers can retry safely (D2/C4).
5. **Stop reviving tombstones.** Do not reset `fail_count` on conflict; keep a `retired_at` marker
   and have `subscribe` either restore only after explicit user action or mint a new id.
   *Risk:* a legitimate browser re-registration could be blocked — needs a product decision.
6. **Retire zombies.** Add a bounded transient threshold and a `last_success_at`; age out rows
   stale beyond N days, and scope the DELETE with space+user (L3). *Risk:* over-aggressive expiry
   drops working subscriptions on quiet bots; choose N conservatively and make it configurable.
7. **Protect secrets.** Wrap `auth` in `secrecy::SecretString`, redact `Debug`, and add a
   metadata-only projection for `list()`; never log `endpoint`/`p256dh` either. *Risk:* touches the
   domain type and its tests; API wire DTOs must be checked so no secret is serialized.
8. **Honest outcome.** Return a distinct outcome when `note_failure` matches no row (Z4).

## Verification (not yet run)

Add integration tests in `crates/infra/tests/notifications.rs` (real DB, per REQ-064 naming):
`req_064_cap_is_atomic_under_concurrency`, `req_064_record_send_is_idempotent`,
`req_064_resubscribe_does_not_revive_retired_endpoint`, `req_064_list_redacts_auth_secret`,
`req_064_failure_delete_is_tenant_scoped`, `req_064_zombie_expires_after_inactivity`.
Run `cargo fmt --check`, `cargo clippy -- -D warnings` and the notification test target after each
change. No test result is claimed here — nothing in this proposal was executed.
