# 0146 — `crates/api/src/public.rs` reliability proposal (v0.4.0)

Module: `crates/api/src/public.rs`
Lens: reliability
Spec anchor: REQ-034, REQ-037, REQ-042, REQ-044, REQ-045, REQ-047, REQ-048, REQ-050, REQ-051,
REQ-054; D-01/D-06 (per-IP counters); spec/04-nonfunctional.md §4.3 (body caps).

Status of claims: "Current behavior" is CONFIRMED by reading this file this session. Any inference
about downstream code (`usecases::*`, `crate::pages::*`, router-level body limit, proxy config) is
GUESS until checked in its owning module. Proposals and risks are GUESS (design inference, not
implemented or tested).

## Current behavior (CONFIRMED)

- Ten cookieless surfaces plus three cookie-gated ones share one shape: resolve the client IP with
  `resolve_client_ip(peer, headers, trusted_proxies)`, hit `usecases::hit_rate_limit` on a
  `kind:<ip>` key, check a body cap, validate, call a use case. Refusals are deliberately uniform
  (401/404/429) so ids and tokens cannot be enumerated.
- Caps: submissions 32 KiB, bot webhook 64 KiB, email 25 MiB, messaging 256 KiB, billing 64 KiB,
  CSP report 16 KiB, SCIM 64 KiB; `account_import` checks only non-empty (32 MiB is a doc comment).
- Limits: submissions 10/h, shared page 120/m, webhook 120/m, email 300/m, messaging 600/m, CSP
  60/h, SCIM 240/m, invite 60/h. A rate-store `Err` becomes an API error response (fail-closed);
  only `plugin_submissions` sends `Retry-After`.
- Credentials: `Bearer ` prefix passed to a use case (webhook, email), `Stripe-Signature` plus a
  textually extracted `t=` (billing), per-platform headers with a Slack challenge (messaging), and
  a constant-time double hash in `scim_bearer_ok`. `shared_page`/`invite_preview` treat the path
  token as a capability; a store failure there is a 500, never a fake 404.
- `csp_report` keeps only hosts from `blocked-uri`/`document-uri` via `host_of`, drops query strings,
  and caps 10 violations/report. Only `shared_page_html_response` attaches a CSP header
  (`crate::pages::SHARED_PAGE_CSP`); JSON responses carry `no-store` (and mostly `x-robots-tag`,
  `referrer-policy`) but no `nosniff`, except the export download.
- Several `Err(_) => deny()` arms (webhook, messaging, billing, callback, import) discard the cause
  with no log or metric. An oversized webhook body returns 401, where email/billing return 413.

## Failure modes

1. **Token validation.** `shared_page` and `invite_preview` accept an unbounded `Path<String>`
   token: a multi-kilobyte path still reaches the hash lookup, so hashing work and URL/log size are
   attacker-controlled. `bot_webhook`/`email_inbound` use a case-sensitive `strip_prefix("Bearer ")`
   and pass whatever follows to the use case; all failures collapse to one 401, so a credential
   store outage is indistinguishable from a wrong token (no signal, fail-closed on availability).
   `billing_webhook` extracts the first `t=` from the header with `parse_signature_timestamp` and
   passes that string separately while the use case re-reads the signature (GUESS) — an injected
   `t=` in a comma list can make the two disagree, and the freshness decision is only as good as the
   use case's own parsing. SCIM is the strongest path (both sides hashed, constant-time compare),
   but `scim_service_provider_config` skips `scim_guard` and only checks the flag, so config is
   readable by anyone once SCIM is enabled.
2. **Rate limits.** Keys are per IP only: a NAT'd bot or a valid bot behind many users shares one
   quota (false 429s), while IPv6 or proxy rotation multiplies allowance with no global/per-token
   second dimension. `resolve_client_ip` trusts `trusted_proxies`; if it is unset behind a proxy,
   `X-Forwarded-For` may pick the key (GUESS — depends on `client.rs`), defeating every limit. The
   check runs before validation, so malformed traffic burns quota. Fixed windows permit a burst at
   the boundary (two windows in seconds), and `count > LIMIT` effectively allows LIMIT+1 requests.
   Store errors are rejected (good) but silently, so a counter outage looks like a flood.
3. **Payload leakage.** `email_inbound` is the one endpoint that returns a use-case error string
   (`AppError::Rule` → `error.to_string()`); if that string echoes parsed message content it becomes
   a reflection/enumeration channel. `host_of` uses `rfind('@')` over the *entire* post-scheme text,
   not just the authority: a `blocked-uri` whose query contains `@` can make the token after that
   `@` be stored as the "host" (e.g. `https://h/p?t=abc@LEAK` yields `LEAK`). `summarize_violation`
   stores up to 100 chars of attacker-supplied `directive` verbatim (log/store injection). The
   export download builds `content-disposition` from a raw `Path<String>` id with only `/` replaced;
   quotes/semicolons in the id can alter the filename parameter. HTML rendering of shared-page
   `title`/`blocks` lives in `pages.rs` (GUESS): if it does not escape, the CSP is the sole backstop.
4. **CSP.** A CSP header exists only on the shared-page HTML branch (both 200 and 404); every JSON
   error and every other public response has none, and none except the export sets `nosniff`. The
   policy's strength (`unsafe-inline`, `frame-ancestors`, `base-uri`, `form-action`) is defined in
   `pages.rs` and is unverified here (GUESS). `csp_report` trusts any caller's report without
   checking that `document-uri` belongs to our origin, so the store can be poisoned with fabricated
   hosts; combined with the free-text directive this pollutes the very telemetry meant to catch XSS.

## Concrete hardening for v0.4.0 (GUESS)

1. Cap path tokens (`shared_page`, `invite_preview`) to a documented length (e.g. 256 chars) before
   the lookup and treat overlong tokens as the same uniform 404/401; add a property test that a
   long token never reaches the use case. *Risk:* an existing token longer than the cap would stop
   working — confirm the token format in `pages.rs`/invite code first.
2. Centralize credential checks: one helper for `Bearer ` parsing (trim scheme, allow one or more
   spaces) and one for comparing secrets in constant time via `tokens.hash`; make billing verify a
   single parsed timestamp shared with signature verification, and add `scim_guard` to the SCIM
   config route or document why it is public. Emit an internal counter for auth-store failure vs
   wrong credential (never the token). *Risk:* relaxing `Bearer` parsing is externally visible —
   record any change in `docs/PARITY.md` (DEVIATION gate).
3. Add a second rate-limit dimension (per bot/installation or per token hash) for webhook, email and
   messaging, and surface a `Retry-After` on all 429s with the actual remaining window. Log counter
   failures as 503 with a request id. *Risk:* new keys change quota semantics and need a config
   knob; verify `client.rs` proxy handling with a test that a spoofed `X-Forwarded-For` is ignored
   unless the proxy is trusted.
4. Fix `host_of` to slice the authority up to the first `/`, `?` or `#`, then strip userinfo with
   `rfind('@')` *within that authority*; add REQ-054 regression cases with `@` and tokens in the
   query. Drop or constrain `directive` to a known CSP token alphabet (or escape it) and sanitize
   the export id to `[A-Za-z0-9._-]` before building `content-disposition`. *Risk:* rejecting unusual
   but legitimate URIs (ports, IPv6, IDN) — test those explicitly.
5. Add `X-Content-Type-Options: nosniff` to every response built in this file, and verify (in
   `pages.rs`) that shared-page HTML escapes `title`/`blocks` and that `SHARED_PAGE_CSP` forbids
   `unsafe-inline`/`unsafe-eval` and sets `frame-ancestors`, `base-uri`, `form-action`. *Risk:* a
   stricter CSP can break a legitimate integration; change it only with a test and a DEVIATION note.
6. Normalize denial statuses (413 for oversized bodies everywhere, uniform 404/401 otherwise) and
   replace silent `Err(_) => deny()` arms with typed errors plus an internal log/metric, so outages
   are observable without changing the client-visible wording. Add an explicit body cap to
   `account_import`. *Risk:* status-code changes are externally visible — gate behind `docs/PARITY.md`.

## Constraints

- Preserve uniform refusals and anti-enumeration wording (REQ-037/042/045/047/048/051); diagnostics
  stay internal, with no token, secret, relay content or PII in logs.
- Keep the no-store, x-robots-tag, referrer-policy and CORS-wildcard invariants; these endpoints are
  cookieless, so `*` is safe, but never add credentials to them.
- Every externally visible behaviour change (status codes, token acceptance, CSP) needs a
  `docs/PARITY.md` entry and approval before shipping; test-first per REQ id per `AGENTS.md`.
