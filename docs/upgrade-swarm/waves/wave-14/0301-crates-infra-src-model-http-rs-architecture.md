# v0.4.0 architecture proposal — `crates/infra/src/model_http.rs`

Worker #301 · LENS=architecture · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: "current state" statements are CONFIRMED by direct reads of
`crates/infra/src/model_http.rs`, `crates/app/src/ports.rs:3972-4033`,
`crates/app/src/usecases.rs:3580-3624`, `crates/infra/src/{mcp_http,messaging_http,openai}.rs`,
`crates/infra/Cargo.toml`, `crates/infra/src/lib.rs`, `crates/bin/src/{config,commands}.rs` in this
worktree. Forward-looking items are proposals (GUESS until implemented and verified). English per
repo disk convention; no gate weakened.

## Current structure (CONFIRMED)

- One module holds two unrelated port adapters: `HttpModelTransport` (`ModelTransport`, REQ-052,
  lines 7-61) and `StripeBillingProvider` (`BillingProvider`, REQ-051, lines 67-178). Both are
  re-exported together at `lib.rs:53`; the file name describes only the first one.
- Each type builds its own `reqwest::Client` with the same recipe — 30 s total timeout, no redirects
  (lines 18-25 and 92-96). The identical builder is repeated in `mcp_http.rs` and
  `messaging_http.rs`; `openai.rs` repeats it with a 60 s timeout and no redirect policy. There is no
  shared factory, no separate connect timeout, no compression feature and no pool tuning.
- Neither constructor takes a base URL: the model probe receives a raw `url: &str` per call, and
  Stripe hardcodes `https://api.stripe.com/v1/checkout/sessions` (line 145). There is no
  `#[cfg(test)] mod tests` here, although `infra` already has `wiremock` 0.6.5 as a dev-dependency
  and `openai.rs` documents a loopback-wire-server test convention.
- The SSRF/outbound guard used by `mcp_http.rs` (`check_outbound_url`, D-05) is not applied to
  `get_json`. Today the URL comes from the domain's fixed `provider_endpoint` table
  (`usecases.rs:3582`), so exposure is bounded, but the port itself accepts any string.
- Auth is an untyped `&[(String, String)]` of arbitrary header name/value pairs (lines 35-40);
  the caller builds `Bearer <key>` as a plain `String` (`usecases.rs:3588-3596`). Stripe correctly
  uses `basic_auth` with `ExposeSecret` and a redacted non-exhaustive `Debug` (lines 74-80, 150).
- Error taxonomies diverge: model failures are `ModelTransportError::{Http, Unavailable}` with the
  upstream body capped at 4 KiB (`MAX_ERROR_BODY`, line 28) and later reflected to the client after
  a 200-char truncation (`usecases.rs:3605-3610`); Stripe collapses every failure (transport, non-2xx,
  bad JSON, missing `url`/`id`) to `AppError::Unavailable` (lines 154-175), losing operator detail.
- The Stripe form is hand-encoded with `boma_domain::percent_encode` (lines 132-142) because
  `reqwest` is built with only `rustls,json`; the `"pro"` plan gate and metadata value are duplicated
  string literals (lines 115, 129). Observability is failure-only `warn!` with no request id, latency
  or status metric and no user-agent.

## Guiding principle for v0.4.0

Keep both adapters thin, typed and testable at the boundary: one client-building policy, one file per
port, one injectable endpoint per upstream, and no raw secret or raw upstream body crossing the port
unchecked. Public trait signatures and the `lib.rs` re-exports stay stable.

## Upgrades (5 concrete items)

1. **Shared `HttpClientFactory` in `infra`.** Add a crate-internal `build_client(timeout)` (or a
   small config struct) used by `model_http`, `mcp_http`, `messaging_http`, `openai` and Stripe, with
   today's exact 30 s/60 s timeouts and redirect policy preserved per caller. Single place later to
   add `connect_timeout`, gzip/brotli, pool limits and a user-agent.
   *Risk:* changing connect/pool behaviour can alter latency or cause new failures; socket reuse
   differences may change upstream rate-limit patterns. *Mitigation:* land as a mechanical move with
   byte-identical builder settings, add a test asserting each adapter's timeout, and keep the full
   clippy/test gate. No wire change.

2. **Split the module by port.** Move the probe to `model_transport.rs` and checkout to
   `billing/stripe.rs`; keep `pub use` of both names at `lib.rs:53` so the two production sites
   (`bin/commands.rs:171`, `bin/config.rs:594`) compile untouched. Keep REQ-051/REQ-052 in module
   docs and test names.
   *Risk:* git churn and possible import ambiguity only; no behaviour change. *Mitigation:* one
   mechanical commit, re-export check via `cargo check --workspace --locked`, grep for stale paths.

3. **Injectable endpoints + wiremock tests for both adapters.** Mirror `OpenAiConfig`: give Stripe a
   `StripeConfig { secret_key, price_pro, api_base }` defaulting to the current URL, and let
   `HttpModelTransport` be constructed with the loopback URL in tests. Cover success, 4xx with body,
   malformed JSON, missing `url`/`id`, and a non-`pro` plan for REQ-051/REQ-052.
   *Risk:* a new `BOMA_BILLING_STRIPE_API_BASE` setting widens config and an operator-set hostile
   base URL is a data-exfiltration path (secret goes in `Authorization`). *Mitigation:* validate
   HTTPS and a pinned host list at startup, keep the default unchanged (no DEVIATION), record the
   setting in `docs/STATE.md`, and never point tests at a third party.

4. **Guard the URL and type the auth headers.** Call `check_outbound_url` in `get_json` as
   `mcp_http.rs` does, and validate each header with `HeaderName::from_bytes` / `HeaderValue` before
   send; carry values as `secrecy::SecretString` and never log them.
   *Risk:* the guard adds a DNS lookup per probe and could reject a provider hostname accepted
   today; invalid-header errors would move from `Unavailable` to a typed rule. *Mitigation:* record
   the D-05 decision in `docs/STATE.md`, add integration tests for every known `provider_endpoint`
   and one loopback reject case, and confirm no `tracing` call can format the auth slice.

5. **Typed request/response boundary and a safer upstream-body carry.** Encode the Stripe form with
   `serde_urlencoded` (add via `cargo add` + `docs/DEPENDENCIES.md`, or enable the reqwest `form`
   feature) and parse the response into a small struct that tolerates Stripe's extra fields; promote
   the `pro` gate to one typed constant. Cap and sanitize (strip CR/LF/control bytes) the error body
   carried in `ModelTransportError::Http`, and give Stripe a typed internal error with status instead
   of blanket `Unavailable`.
   *Risk:* encoding changes (space `+` vs `%20`) are provider-visible, and the client-visible
   `InvalidName` message in `usecases.rs:3608` must not change without a `docs/PARITY.md` DEVIATION.
   *Mitigation:* differential-test the form body against a captured request, keep the app-layer
   message byte-identical, and log full bodies only at `trace` level with secrets excluded.

## Cross-cutting risks / sequencing

- Sequence 1 → 2 → 3 → 4/5. Step 1 is the low-risk base; 2 is mechanical; 3 creates the test seam
  that makes 4 and 5 verifiable. Steps 4 and 5 are independent after 3.
- No new dependency should enter without `cargo add`, cargo-deny and a `docs/DEPENDENCIES.md` row.
- All outbound calls keep a timeout, no redirects and no lock held across `.await` (REQ standards).
- Verification gate per step: `cargo fmt --check`, `clippy -- -D warnings`, unit + `tests/`
  integration, golden and contract suites; traceability via REQ-051/REQ-052 in test names.

## Out of scope for this module

Credential validation policy, the `provider_endpoint` table, the billing webhook/HMAC path and API
error formatting live in other crates; this proposal only reshapes the two HTTP adapters and their
shared client construction.

## Evidence

CONFIRMED — read this session: `model_http.rs:1-178`; `ports.rs:3972-4033`;
`usecases.rs:3580-3624`; `mcp_http.rs:1-60`; `messaging_http.rs:1-70`; `openai.rs:1-60`;
`crates/infra/Cargo.toml`; `lib.rs:30,53`; `bin/config.rs:575-610`; `bin/commands.rs:171`.
GUESS — that the domain `provider_endpoint` table is exhaustive for all configured providers; that
no golden fixture asserts the exact credential-refusal message (verify before step 5).
