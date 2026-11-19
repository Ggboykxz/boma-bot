# 0213 — `crates/domain/src/billing.rs` — architecture (v0.4.0)

- Worker: 213 · Lens: architecture · Target: v0.4.0 · Read-only review; no code edited.
- Module: 186-line pure domain module; spec anchors: REQ-051 (billing/credits/plan limits),
  REQ-018 (spending limits), `AGENTS.md` Rust standards (newtypes, typed errors, pure domain,
  property tests for external parsers), `docs/STATE.md:740–755`, ADR-023.
- Read here: `billing.rs`, `error.rs`, `lib.rs:51–53`, `app/src/ports.rs:3813–3892`,
  `app/src/usecases.rs:4045–4114`, `infra/src/stores/billing.rs:1–60`,
  `spec/01-requirements.md:415–421`, `spec/04-nonfunctional.md:52`.
- CONFIRMED = read in this session; GUESS = inferred.

## Structure today (CONFIRMED)

`billing.rs` is intentionally narrow: the *only* billing logic in the domain crate is verification
of the Stripe `t=,v1=` HMAC. It owns `WEBHOOK_MAX_SKEW_SECONDS = 300`, a borrowed `StripeSignature<'a>`
(`timestamp: &str`, `digest: &str`), `parse_stripe_signature` (split on `,`, last `t=`/`v1=` wins,
whitespace tolerated, `DomainError::InvalidName` on absence), and `verify_stripe_signature`
(parse → timestamp equality → ±300 s skew → lowercase-hex check → HMAC-SHA256 over `t + "." + body`
→ manual constant-time compare). Three `req_051_*` unit tests cover the happy path, tamper/wrong
secret/stale/garbage, and header tolerance. `lib.rs` re-exports all four items. The module is pure
and correctly layered: no IO, no async.

The architecture debt is what is *not* here: the billing domain model itself. Plan, subscription
state, credits, money and spending limits are stringly-typed structs in `app/src/ports.rs`
(`plan: String`, `status: String`, `cap_minor_units: i64`, `credits_minor_units: i64`, currency as
`String`), the activation rules live in `app/src/usecases.rs` (serde_json `Value` poking, lines
4079–4101), and persistence in `infra/src/stores/billing.rs`. So "billing" is split across three
crates with almost no typed vocabulary in the pure domain — invalid states (`plan = "gold"`,
negative credits, `status = ""`) are representable everywhere, against the `AGENTS.md` newtype /
make-invalid-states-unrepresentable rule. The verification core is sound and should survive v0.4.0.

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Introduce a typed billing vocabulary in the domain crate.** Add `Plan { Free, Pro }`,
   `SubscriptionStatus { Active, Canceled, PastDue }`, `MinorUnits(i64)`, `Currency` and a credit
   newtype; re-express `SubscriptionState`, `NewSubscription`, `SpendingLimit`, `SpendingSummary`
   as domain types that `app::ports` reuses. This is the core "billing domain model structure"
   upgrade and directly serves the newtype/money rules.
   *Risk:* broad compile-time churn across ports, infra/Postgres and in-memory testkit stores and
   the API wire layer. Keep all wire/DB representations byte-identical via `TryFrom`/`From` at the
   boundaries; unknown provider strings must map to a typed error, not silently default. Any
   observable change is a DEVIATION needing `docs/PARITY.md` + owner sign-off (GUESS on blast radius
   until a compile is run).

2. **Give signature parsing its own typed error.** `parse_stripe_signature` currently reports a
   malformed `Stripe-Signature` as `DomainError::InvalidName` — a name-validation variant reused for
   a crypto/format failure, which misleads callers and rustdoc. Add a dedicated variant (e.g.
   `InvalidStripeSignature`) or a local `SignatureError` converted at the boundary.
   *Risk:* low — `CONFIRMED` the only consumer (`billing_webhook`) reduces verification to a `bool`
   and never matches that variant, so internal API only; still update the `# Errors` rustdoc and
   re-export list, and re-run REQ-051 tests.

3. **Make `StripeSignature` structurally honest to the protocol.** Today duplicate `t=`/`v1=` are
   silently last-wins and whitespace/empty values are tolerated; Stripe can send multiple `v1`
   signatures during secret rotation. Parse into a typed timestamp `i64` and a non-empty set of
   digests, and verify against *any* digest in constant time, reusing the parsed timestamp instead
   of re-parsing it in `verify_stripe_signature`.
   *Risk:* changes the public `StripeSignature<'a>` shape and `lib.rs` export; constant-time
   iteration must not early-exit on the first match (timing), and the ±300 s replay rule must stay
   identical. Add a proptest over headers (AGENTS.md requires it for external-format parsers) and
   re-run REQ-051/REQ-043 uniform-refusal assertions.

4. **Separate "verify" from "interpret" as a pure domain step.** Move the JSON-to-command decoding
   out of `usecases.rs:4079–4101` into a pure helper returning a typed `CheckoutCompleted` command
   (plan, `space_id`, provider ref, seeded credits) with typed errors, while IO/store writes stay in
   app. That puts the whole billing pipeline — verify → interpret → apply — behind one typed model.
   *Risk:* the domain crate has no `serde` today; adding it is a dependency decision (cargo add,
   docs.rs, `docs/DEPENDENCIES.md`, cargo-deny) and must not pull IO/async into domain. If serde in
   domain is rejected, keep parsing in app but return the typed command so the boundary stays clean.

5. **Harden the skew check against arithmetic overflow.** `(now - sent).abs()` (line 69) is fed by
   `timestamp.parse::<i64>()`, so a huge hostile timestamp can overflow the subtraction/`abs` — in
   debug that panics (denied by clippy `panic` lint), in release it wraps and can wrongly pass the
   window. Replace with `checked_sub`/`saturating_sub` and a typed `Skew` newtype defaulting to the
   300 s constant; inject it rather than passing a raw `now: i64` from callers.
   *Risk:* signature change touches `usecases.rs:4070–4076` and all tests; verify extreme-i64 edge
   cases with unit tests and keep the ten-minute-replay refusal intact (CONFIRMED test at 135–141).

## Explicit non-goals

No IO/async or serde-into-domain shortcuts that break purity, no append-only credit ledger or manual
grants (deferred by ADR-023), no change to the uniform-404 refusal semantics or the ±300 s window,
no hardcoded copy, and no feature outside REQ-051/REQ-018. Deferred items (DEC-17) go to a STATE/ADR
proposal, never shipped silently. Verify with `cargo fmt --check`, `clippy -D warnings` and the
REQ-051 suite before claiming any change complete.
