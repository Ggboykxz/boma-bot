# 0282 — `crates/infra/src/crypto.rs` reliability proposal (v0.4.0)

Module: `crates/infra/src/crypto.rs`
Lens: reliability
Anchors: REQ-040/041 (sessions), D-08 (assert security config), AGENTS.md Rust standards
(no blocking work on the async runtime, secrets in `secrecy`, constant-time comparison,
no panics in non-test code).

Claim status: "Current behavior" is CONFIRMED by reading this session — `crypto.rs` in full,
the port signatures (`app/src/ports.rs:4117-4165`), the only in-tree caller of the exported
helper (`stores/sessions.rs:47-62`), the auth reliability report (wave-9/0122), and the
duplicate helpers (`app/src/usecases.rs:1932`, `domain/src/messaging.rs:85`,
`testkit/src/scripts.rs:260`). Proposals and risks are GUESS (design inference; not
implemented or tested by this worker).

## Current behavior (CONFIRMED)

- Four adapters: `Argon2idHasher` (Argon2id default params, `try_generate_salt()`),
  `TokenGenerator` (32 B `SysRng` → base64url token; SHA-256 for storage),
  `IdGenerator` (19 B → 26 chars, counter fallback on RNG failure), `SystemClock`
  (RFC 3339, second precision, epoch fallback), plus public `constant_time_eq`.
- `hash`/`verify` are `async fn` but run Argon2 **synchronously on the caller's thread**.
- Token/ID/password values are plain `String`/`&str`; local byte buffers are never zeroized.
- RNG policy is split: token and password fail closed to `AppError::Unavailable`; the ID path
  silently degrades to a timestamp+pid+counter value and logs at `error!`.
- Two distinct OS RNG sources are used (`argon2`'s salt RNG and `rand`'s `SysRng`).
- No nonce/IV, no symmetric cipher, no persistent key lives in this module.

## Failure mode 1 — Nonce / token reuse (RNG malfunction)

No nonce-bearing primitive exists here, so classic nonce reuse is structurally absent; the
real analogue is *reuse or predictability of the token stream*. `generate()` fails closed if
`SysRng` errors, but nothing detects an RNG that silently returns repeated or all-zero bytes.
`new_id()` degrades to `fallback_id()` (l.148-157): uniqueness then rests on `now_utc()` +
pid + an atomic counter, so a clock rollback or a process restart with the same pid can
collide, and an ID used as an unguessable capability (invite id, l.802 tests) becomes
predictable. No test exercises either failure path (grep: no fallback/RNG-failure test).

Hardening: add a startup RNG health self-test (two independent draws must differ) and a
metric on every RNG failure / every ID-fallback, so a degrading CSPRNG is loud, not silent.
Keep token generation fail-closed. If any id is used as a bearer capability, move it to a
`Result`-returning port and fail closed — that is a cross-crate API change (ADR + DEVIATION).

Risks: a self-test can false-positive on a mocked RNG in tests — inject a test RNG, do not
assert on wall-clock timing. Converting `new_id` to `Result` touches every store and test
(hundreds of call sites); defer unless invitations truly rely on id secrecy. Paper fallback
IDs must be documented as non-secret or they become a latent auth bug.

## Failure mode 2 — Key / secret handling

Tokens act as bearer keys but are returned as `String` and hashed with a bare, unsalted
SHA-256 (fine only because tokens are 256-bit random — the invariant noted in wave-9/0122:
swapping in a low-entropy token breaks the whole scheme with no local test). The 32-byte
buffer in `generate()` and the SHA-256 state in `hash()` are dropped without zeroization;
`verify()` takes the password as `&str`. No secret value is logged here (CONFIRMED), which
must be preserved. Argon2 defaults are stored in the PHC string and `verify` honours stored
params — good — but there is no rehash-on-login, so raising cost later never protects
existing rows.

Hardening: zeroize the token buffer and hasher state (`zeroize::Zeroizing`) and pin a unit
test that a token is exactly 256 bits of entropy from `SysRng`. Add a `docs/PARITY.md` note
that `TokenGenerator` must stay high-entropy and comparison must stay constant-time.
Move rehash-on-login to the accounts use case (compare parsed params to current, rewrite on
successful verify).

Risks: wrapping return values in `secrecy::SecretString` is a wide ripple through the
`TokenGenerator` port and all callers (`usecases.rs`, `accounts.rs`, `api/*`, `testkit`) —
needs an ADR, not a v0.4.0 fast-follow. Rehash-on-login adds a DB write to the auth path;
gate it behind a params-drift check and a test.

## Failure mode 3 — Constant-time comparison (CONFIRMED defect)

`constant_time_eq` is `left.len() == right.len() && left.ct_eq(right).into()` (l.198-201).
The length check is variable-time and short-circuits `ct_eq` entirely for unequal lengths.
Session lookups pass two 32-byte SHA-256 digests (`sessions.rs:60`), so the equal-length
path holds today — safe by coincidence, not by contract. Four independent copies of this
helper exist; one could be fixed and the others left leaking (drift risk).

Hardening: hash both inputs to a fixed length before `ct_eq`, or make the helper require
equal-length inputs with a `debug_assert` and document the precondition. Consolidate the
four copies into one shared helper and keep this module's signature as the re-export. Add a
property test: equal inputs → true, one-byte flips → false, no panic on unequal lengths.

Risks: hashing each comparison adds allocation and microseconds on a hot auth path; prefer
the equal-length contract (zero cost) if callers can guarantee digests. Do not write a
wall-clock timing test — it is flaky. Consolidation changes crate dependencies → ADR.

## Failure mode 4 — Secret leakage / silent corruption (reliability view)

`verify()` maps an unparsable stored hash to `Unavailable` (503) and a wrong password to
`Ok(false)` (401). That is a correct distinction, but a corrupted hash row yields a
per-user 503 with no counter, so data corruption is invisible until users complain.
`SystemClock::render()` falls back to `FALLBACK_TIMESTAMP` (epoch, l.33/190) when the clock
cannot be formatted; because expiry is a lexical string compare (wave-9/0122), an
unrenderable clock silently expires **every** session — a total logout event with no signal.

Hardening: emit an internal counter/log for hash-parse failures and for clock-render
fallback (no token, password, or PII in logs — existing invariant). Prefer surfacing an
unrenderable clock as `Unavailable` rather than epoch; if the `Clock` trait must stay
infallible, at minimum alert loudly. Pin a regression test that the canonical RFC-3339
format is unchanged (the lexical compare depends on it).

Risks: changing `Clock::now()` to `Result<String, _>` is a trait-wide change (ADR). Counters
are internal-only and safe. Any expiry-format change is a DEVIATION awaiting approval.

## Reliability cross-cutting — blocking the async runtime (CONFIRMED)

`PasswordHasher::hash`/`verify` are async but perform Argon2 (~19 MiB, 2 passes, tens of ms)
inline; under concurrent logins this blocks Tokio worker threads and inflates p99 latency.
Move the Argon2 work into `tokio::task::spawn_blocking`, cloning owned inputs into the
closure and zeroizing them afterwards.

Risks: requires `Argon2: Clone` (it is `Clone`, CONFIRMED l.41) and owned `'static` data;
the extra secret clone must be zeroized. `spawn_blocking` adds scheduling overhead on an
otherwise idle single-core box — measure before/after with a benchmark.

## Priority for v0.4.0

1. Flip the length-check side channel and add the RNG-failure/clock-fallback counters (small,
   no interface change).
2. Wrap Argon2 work in `spawn_blocking` + zeroize local buffers (internal only).
3. Consolidate the constant-time helper and add the entropy/format invariant tests (ADR for
   the shared-crate dependency).

Each item needs REQ-tagged tests (`req_040_*`/`req_041_*` names). Any externally visible
auth or expiry change is a DEVIATION in `docs/PARITY.md` awaiting approval.
