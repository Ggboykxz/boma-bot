# Wave 13 — Worker 0281 — `crates/infra/src/crypto.rs` (architecture) — v0.4.0 proposal

Scope: read-only architecture review of `crates/infra/src/crypto.rs` (201 lines), its four port
definitions in `crates/app/src/ports.rs:4117-4165`, and its consumers (wiring in `bin/commands.rs`,
`infra/tests`, `api/automation.rs`, `api/public.rs`, `stores/sessions.rs`, `app/accounts.rs`,
`sibling helpers` `secret_box.rs` and `space_keys.rs`). All claims CONFIRMED by direct reading
unless marked GUESS. No behavior change is proposed unless explicitly noted. Nothing else modified.

## Current structure (CONFIRMED)

The file is a flat grab-bag of five unrelated concerns, grouped only by "small service, no DB":

- **Password**: `Argon2idHasher` wraps `Argon2<'static>`; `hash` generates a salt via `argon2`'s
  `try_generate_salt`, hashes to a PHC string; `verify` parses the PHC string and returns `bool`.
- **Tokens**: `TokenGenerator` fills 32 bytes from `rand::SysRng`, base64url-no-pad encodes for
  `generate`; `hash` is a bare SHA-256 of the token string, returned as `Vec<u8>`.
- **Ids**: `IdGenerator` fills 19 bytes from the same `SysRng`, encodes to 26 base64url chars with
  `prefix_`; on RNG failure it deliberately degrades to a unique-but-not-random `fallback_id()`
  (timestamp + pid + `AtomicU64`, `Relaxed`).
- **Clock**: `SystemClock` (not cryptography) renders RFC 3339 UTC with nanoseconds zeroed, with a
  `FALLBACK_TIMESTAMP` constant used if formatting fails. `now_plus_seconds` uses `OffsetDateTime +
  Duration`.
- **Primitive**: free `constant_time_eq` built on `subtle::ConstantTimeEq` with a length precheck.

Policy lives in module constants (`TOKEN_BYTES`, `ID_BYTES`, `ID_LENGTH`, `FALLBACK_TIMESTAMP`).
`lib.rs` re-exports the four structs but *not* `constant_time_eq`; the sessions store reaches it via
`crate::crypto::constant_time_eq`. There are **no tests in this file** (CONFIRMED), unlike its
sibling `secret_box.rs`, which has eight REQ-tagged tests. Error handling is uniformly
`AppError::Unavailable` on RNG/hash/parse failure; id generation cannot fail. Async is nominal only:
`PasswordHasher::hash`/`verify` are `async_trait` but do CPU work inline, with no `spawn_blocking`
anywhere in the workspace (CONFIRMED by grep).

## Architectural risks observed (CONFIRMED)

- **Constant-time equality is implemented four ways.** `crypto.rs` (`subtle`), `domain/messaging.rs:84`
  (hand-rolled), `testkit/scripts.rs:260` (hand-rolled), and two inline XOR folds in
  `api/automation.rs:62-69` and `api/public.rs:982-989`. Drift here is a security regression, and a
  maintainer cannot tell which copy is authoritative.
- **The port's error contract is false.** `TokenGenerator::hash` cannot fail and never touches the
  RNG, yet its rustdoc says "Returns an error when the system random source fails"; it returns a
  `Result` that forces `let Ok(..) else { return false }` noise at four call sites.
- **Async blocking.** Argon2id (default 19 MiB, two passes) runs on the Tokio worker; concurrent
  logins and the deliberate dummy hashes in `app/accounts.rs:139,231` can stall unrelated requests.
- **Clock panic surface.** `OffsetDateTime + Duration::seconds(seconds)` can panic on overflow; the
  trait returns `String`, so there is no error path. Absurd TTLs are GUESS-able from config but not
  currently validated at this boundary.
- **Wrong home.** `SystemClock` is a time service, not crypto; the module mixes three RNG consumers
  and two hashing schemes behind one import with no seam for a fake RNG in unit tests.

## Proposed upgrades for v0.4.0

1. **Split into a `crypto/` module tree behind a facade.** `crypto::password`, `crypto::token`,
   `crypto::id`, `crypto::rng` (the single `fill_bytes` helper), plus move `SystemClock` to a top-level
   `clock` service re-exported from `lib.rs`. Keep `crypto.rs` (or `crypto/mod.rs`) as a `pub use`
   facade so all `crate::crypto::*` paths and the `lib.rs` re-exports still compile. Preserve every
   REQ-tagged rustdoc verbatim. *Risk: LOW* — mechanical; one module per commit, compile between
   steps; no wire or API change.

2. **One constant-time comparator, used everywhere.** Promote the `subtle`-based implementation to a
   shared primitive in `boma-domain` (or `boma-app::ports` if the architect prefers — GUESS: layering
   call), keep a deprecated re-export at `crate::crypto::constant_time_eq` for one release, then
   replace the three hand-rolled copies and the two inline folds. Add a test that every consumer's
   result matches the primitive over equal/unequal/empty/different-length inputs. *Risk: LOW-MEDIUM*
   — wide but shallow edit (five call sites), no output change; ensure the shared function keeps the
   length precheck so behavior is identical.

3. **Offload Argon2 to the blocking pool and bound it.** Wrap `hash`/`verify` bodies in
   `tokio::task::spawn_blocking` (clone `Argon2<'static>` in — it is `Send + Sync`), and gate them
   behind a small `Semaphore` (e.g. 4 concurrent hashes) so a login burst cannot exhaust CPU/memory.
   Keep `AppError::Unavailable` semantics and add a `criterion` benchmark of hash latency before and
   after. *Risk: MEDIUM* — `async_trait` requires `'static`/`Send`; `testkit`'s scripted hasher and
   all `Arc<dyn PasswordHasher>` wiring must keep compiling; a too-small permit count adds latency
   under load, so measure before fixing the number. No observable API change.

4. **Repair the token-hash contract.** Introduce a `TokenHash([u8; 32])` newtype returned by value
   (SHA-256 is infallible), correct or remove the misleading error doc, and update the six call
   sites; or, for a smaller step, keep the `Result` but fix the doc and add `#[must_use]`. Route all
   three random sources (`argon2` salt, token, id) through the new `crypto::rng` helper with one
   error/logging policy, preserving the deliberate split: token failure is `Unavailable`, id failure
   falls back to the counter. *Risk: MEDIUM-HIGH* — touches the app-level port plus `accounts.rs`,
   `api/*`, `conformance/golden_world.rs`, `testkit` and tests; stage it behind a compatibility shim
   and compile between steps. Internal signatures only — no external DEVIATION expected.

5. **Make clock arithmetic total and cover the module with tests.** Replace `+ Duration::seconds`
   with `checked_add`, falling back to `FALLBACK_TIMESTAMP` on overflow, and validate configured TTLs
   at startup. Add unit/property tests for: RFC 3339 shape and zero nanoseconds, `now_plus_seconds(0)`
   equality, overflow fallback, 32-byte/26-char output lengths and base64url alphabet, id
   uniqueness (including the counter fallback), and comparator symmetry. *Risk: LOW* — new tests are
   free; the overflow fallback only changes behavior for currently-panicking, absurd TTLs.

## Sequencing and gates

Order 1 → 2 → 5 → 3 → 4 (pure structure, then correctness, then the port/type change last). Each step:
`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, then tests. Record
any externally visible error-string change in `docs/PARITY.md` before shipping; keep every rustdoc
on public items and the REQ-id naming rule intact. No new dependency is required except `criterion`
for step 3 (add with `cargo add`, docs.rs API check and `docs/DEPENDENCIES.md` license entry).
