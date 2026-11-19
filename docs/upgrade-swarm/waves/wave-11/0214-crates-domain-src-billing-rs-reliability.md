# Worker 0214 — `crates/domain/src/billing.rs`, lens: reliability (v0.4.0 proposal)

## Scope reality check (read first)

CONFIRMED from the file: this module is **only** the Stripe webhook signature surface (REQ-051) — a
`StripeSignature` parser, a constant-time HMAC-SHA256 verifier, a 300 s skew constant, and three unit
tests. It contains **no** amounts, **no** currency, and **no** state machine. The requested failure
modes "invalid amounts, currency drift, state transitions" therefore do not live here:

- Amounts / currency: REQ-018 spending limits and the seeded credit constant
  (`PRO_SEEDED_CREDITS_MINOR`, `app/src/usecases.rs:4135`) live in `app`/`infra`; ISO normalisation is
  asserted in `testkit/src/build.rs:6954-6972`. Range: GUESS that no domain `Money` newtype exists yet.
- State transitions: `activate_subscription` (`app/src/usecases.rs:4126`) is the transition, driven by
  a verified webhook. This module's reliability **gates** it, so a verifier defect has money impact.

This proposal hardens what is actually here and explicitly hands the three named modes to their real
owners. Per AGENTS.md, adding domain money types would expand scope: list it in `docs/STATE.md` as a
proposal rather than doing it silently.

## Failure modes, ranked

1. **DoS — signed overflow in the skew check (CONFIRMED, highest).** `billing.rs:69` computes
   `(now - sent).abs() > 300` on attacker-controlled `sent`. `sent` is parsed from the `t=` header with
   `parse::<i64>()`, so `t=-9223372036854775808` (i64::MIN) with `now≈1.8e9` overflows `now - sent`
   in debug (panic) and wraps in release; `.abs()` on i64::MIN overflows again. A single unauthenticated
   POST can panic the task. This is the only panic-capable path in the module.
2. **Replay → duplicate credit seeding / bad state transition (CONFIRMED by absence).** With no event-id
   dedupe in `billing.rs` or the use case, the same valid `checkout.session.completed` replayed inside
   the 5-minute window calls `activate_subscription` again. Whether that double-seeds credits depends
   on the store's upsert semantics (GUESS — not verified here), so the risk is unproven-but-real.
3. **Clock coupling → mass rejection (CONFIRMED).** `now` is `unix_seconds(&clock.now())`
   (`app/src/usecases.rs:4069`). Any host clock drift > 300 s rejects every legitimate webhook; the
   window silently turns an ops problem into lost activations with only a uniform 404.
4. **Two disagreeing parsers (CONFIRMED).** `api/src/public.rs:717 parse_signature_timestamp` takes the
   **first** `t=` via `find_map`; domain `parse_stripe_signature` takes the **last** via overwrite. A
   header `t=1,t=2,v1=…` currently fails closed (mismatch → false), but the divergence is a latent
   bypass if either parser is later "simplified". Duplicate `v1=` is also last-wins.
5. **No multi-`v1` rotation support (CONFIRMED in code, impact GUESS).** Stripe may send several `v1`
   digests during secret rotation; verifying only the last can reject otherwise valid events.
6. **Misclassified error (CONFIRMED).** Parse failure returns `DomainError::InvalidName`, polluting
   error metrics meant for name validation; it also embeds header text in the message.
7. **Out-of-scope named modes (CONFIRMED absent here).** Invalid amounts / currency drift need a domain
   `Amount(MinorUnits)` + ISO-4217 newtype and boundary validation in `app`/`infra`; state transitions
   need idempotency at the store. Neither belongs in a pure signature verifier.

## Concrete hardening (code-first)

- **Fix the overflow with widened arithmetic, not saturating tricks.** Replace `:69` with:
  `let delta = i128::from(now) - i128::from(sent); if delta.abs() > i128::from(WEBHOOK_MAX_SKEW_SECONDS) { return false; }`
  Two i64 values always fit in i128, so no panic in any profile. Keep the constant-time digest loop at
  `:86-90` untouched. Add a proptest over arbitrary `now`/`sent` asserting the function never panics.
- **One parser.** Make `api/public.rs` call `parse_stripe_signature` and pass the already-parsed
  `timestamp` (as `&str`) into the use case; delete the duplicate. In the domain parser, reject duplicate
  `t=`/`v1=` keys (duplicate → `Err`) instead of last-wins, and validate `t=` is numeric there so the
  parser is total and the verifier only does crypto.
- **Return a typed verification error.** Add `DomainError::InvalidStripeSignature` (no quoted header);
  map it to the same uniform 404 at the API edge so no information leaks. This is internal telemetry
  only — no external behaviour change.
- **Multi-`v1`: collect all digests and accept if any matches.** Preserves constant-time comparison per
  candidate; budget the loop (Stripe sends few). If Stripe rotation is not needed for v0.4.0, defer and
  record as GUESS in `STATE.md`.
- **Dedupe at the owner (not this module).** Add a unique index on Stripe event `id` / `provider_ref`
  in a versioned migration; make `activate_subscription` an insert-if-absent and return 200 on replay.
  Also alert on skew-window rejections so clock drift is observable rather than a silent 404.

## Risks / trade-offs

- Widening to i128: negligible cost; only behaviour change is that previously panicking inputs are now
  rejected — hardening, **not** a DEVIATION.
- Rejecting duplicate/uppercase/multi-key headers: safe if Stripe never emits them (GUESS); verify with
  a captured real header before enforcing, else it could reject live events.
- Multi-`v1` loop slightly widens the verification path; cap candidates to avoid a cheap CPU sink.
- Store dedupe needs a migration and an idempotency contract with the port — internal, but record it in
  `docs/PARITY.md`/`STATE.md` if the retry response shape changes.

## Tests to add (REQ id in the name)

`req_051_i64_min_timestamp_never_panics`, `req_051_overflow_does_not_bypass_the_window`,
`req_051_duplicate_t_or_v1_is_rejected`, `req_051_api_and_domain_parse_the_same_timestamp`,
`req_051_replayed_event_does_not_double_seed_credits` (integration, app/store).

**Recommendation for v0.4.0:** ship items 1, 3 (one-parser) and 6 now — small, provable, no API change;
treat dedupe and multi-`v1` as a follow-up ADR. (CONFIRMED: all line references from files read this session.)
