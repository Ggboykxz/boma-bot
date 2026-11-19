# 0299 — `crates/infra/src/messaging_http.rs` architecture proposal (v0.4.0)

Module: `crates/infra/src/messaging_http.rs` (110 lines, read this session on `before-m5`).
Lens: architecture / adapter structure. Spec anchor: REQ-048 (`spec/01-requirements.md`:391-397,
TRACE.md:65) plus D-05 outbound safety (spec/04-nonfunctional.md §4.5, via `net_safety.rs:1-13`).
Status: the "current shape" and issues are CONFIRMED by reading the module, the port
(`crates/app/src/ports.rs`:3677-3723), `net_safety.rs`, `model_http.rs`, callers in
`crates/app/src/usecases.rs` and `push.rs`, and wiring in `crates/bin/src/commands.rs`:166-169.
Upgrades and risks are PROPOSAL/GUESS (design inference, not implemented). English per repo disk
convention; spec / ADR / charter stay authoritative.

## 1. Current shape (CONFIRMED)

One struct, `HttpMessagingTransport { http: reqwest::Client }` (`:11-13`), implements the three
`MessagingTransport` methods around a single 30 s / no-redirect client (`:21-28`):

- `get_json` (`:33-50`) — GET + caller headers, `.json::<Value>()`; **no SSRF guard, no status check**.
- `post_form` (`:52-86`) — hand-rolled `percent_encode` + `format!` + `join("&")`, fixed
  `application/x-www-form-urlencoded`, `.json::<Value>()`; **no SSRF guard, no status check**.
- `post_json` (`:88-109`) — the only method that calls `check_outbound_url(url, allow_private)`
  (`:95-99`), then POSTs JSON and returns `response.status().as_u16()`.

Every failure collapses to `AppError::Unavailable` (`:42-45, 78-81, 104-107`), a SSRF refusal
becomes `AppError::Rule(InvalidName(reason))` (`:97-99`), and the module has no tests.
Observed callers today use only fixed public hosts (`slack.com`, `api.telegram.org`), so the
missing guards are latent, not currently exploited. The Telegram bot token is embedded in the URL
path by callers (`usecases.rs`:2813, 2868) — a secret in a string that errors/logs may capture.

## 2. Architecture issues (CONFIRMED)

1. **Guard is method-specific, not adapter-wide.** `ports.rs`:3709-3711 promises the deployment's
   HTTP client enforces the allow-list; only `post_json` does. `get_json`/`post_form` take a raw
   `&str` URL and no `allow_private`, so a future member-supplied target routed through either
   method bypasses D-05 entirely. Three methods, one safety contract, one enforcement point missing.
2. **Check/connect split (TOCTOU).** `net_safety.rs`:132-155 resolves the host and returns
   `CheckedOutbound.resolved`, but the transport discards it; reqwest re-resolves at `send()`. DNS
   rebinding between the two lookups defeats the validation. Redirect policy already prevents a
   second unchecked hop (`:12-13`) but not this race.
3. **Lossy error model.** `Unavailable` erases 4xx vs 5xx, permanent vs transient, and body;
   `get_json`/`post_form` parse a body even on non-2xx. Contrast `ModelTransportError::Http`
   (`ports.rs`:4021-4033, `model_http.rs`:28,46-55). The push module's planned
   `Delivered|Gone|Transient` sink (wave-10/0181, P1/P5) cannot be built on this result type.
4. **Four client builders, no shared kit.** The identical `Client::builder().timeout(30s).no
   redirects()` appears here, in `model_http.rs`:18-25 and 88-96, and in the MCP adapters. No
   shared pool limits, connect timeout, user-agent, or instrumentation hook; drift is certain.
5. **Unbounded response and duplicated encoding.** `.json::<Value>()` buffers with no cap; the
   form body is re-implemented from `model_http.rs`:132-142. The repo standard asks for body limits,
   request ids and metrics; this adapter has none (only `warn!` on failure).

## 3. Concrete upgrades for v0.4.0 (with risks)

**P1 — One enforcement point for all three methods (do first).** Add a private
`send_checked(method, url, allow_private)` used by `get_json`, `post_form` and `post_json`; make
the port's `allow_private` explicit on every method (default `false` at callers). Risks: (a) port
change touches `ScriptedMessagingTransport` (`testkit/src/scripts.rs`:1049-1141) and all callers —
keep the double's behaviour byte-identical; (b) adding a DNS lookup to `get_json`/`post_form`
introduces a new `NotResolvableOrPrivate` failure where `Unavailable` was returned: map it exactly
as `post_json` does today or goldens change; (c) `allow_private=true` must still emit the D-05 audit
event and never allow metadata hosts; (d) no call site may start passing `true` for convenience.

**P2 — Pin the checked address to close rebinding.** Feed `CheckedOutbound.resolved` into
reqwest's per-request `resolve()` so connect uses the validated IP while host/SNI stay the URL's.
Risks: (a) GUESS — confirm `resolve` semantics and SNI behaviour on docs.rs for reqwest 0.13.5
before coding (AGENTS.md: never type APIs from memory); (b) IPv6 and IP-literal URLs need a
separate branch; (c) multiple A/AAAA answers must all be validated, not just the first; (d) treat
as a security hardening with an ADR note, and keep no-redirects; (e) an allow_private LAN target
may legitimately re-resolve — document the new strictness as CONFIRMED after a wiremock test.

**P3 — Typed transport result, status classification, secret-safe errors.** Mirror
`ModelTransportError`: `MessagingTransportError { Rejected(reason), Http{status, body: capped},
Unavailable }`, reject non-2xx in `get_json`/`post_form` with a 4 KiB body cap, and keep
`post_json -> u16` for port compatibility (add a typed variant only if 0181 P1 lands first).
Risks: (a) a public port change requires infra + testkit updates; (b) `usecases.rs` install flows
assume the old error shape — preserve every user-visible message; (c) never log the request URL
(Telegram token) or header values; log a host label only; (d) a DEVIATION is required if any
externally visible error/status changes.

**P4 — Shared outbound HTTP kit in `infra`.** New `http_client.rs` exposing the common builder
(30 s total + connect timeout, no redirects, bounded pool) plus one form-encoder helper, used by
messaging, model, Stripe and MCP. Risks: (a) a shared edit changes four adapters — freeze their
observable behaviour first with tests; (b) connect timeout and a user-agent are new wire behaviour
(DEVIATION check / spec silence check); (c) replacing the encoder with a crate dependency needs an
ADR and byte-parity proof against the current Slack/Telegram OAuth form; (d) keep rustls and the
`rustls`-only reqwest features.

**P5 — Wiremock contract + encoder property tests, REQ-048 in test names.** Add
`crates/infra/tests/messaging_http.rs` (wiremock 0.6.5 is already a dev-dependency): assert method,
path, headers and exact form bytes; no redirect followed; SSRF refusal on all three methods;
status classification P3. Add a proptest/fuzz case for `percent_encode` and `MAX_ERROR_BODY`.
Risks: (a) a test pinning current form bytes can freeze a bug — fix encoding only with byte-parity
evidence; (b) never point a test at a real platform (AGENTS.md); (c) no secrets in fixtures.

## 4. Ordering, invariants, non-goals

Order: **P1 → P2 → P3 → P4 → P5** (P5 proves P1-P3; P4 is the cross-adapter cleanup last).
Invariants: D-05 never weakens; metadata hosts stay unreachable; no secrets in URLs, headers, logs
or errors; fixed-platform callers keep working unchanged; no externally visible change without a
DEVIATION record and owner approval. Non-goals: no new dependency without an ADR, no new service,
no ADR/spec edit by this worker, no retry loop (the port deliberately returns a status).

## 5. Evidence

CONFIRMED — read this session: `messaging_http.rs`:1-110; `ports.rs`:3677-3723, 4021-4033;
`net_safety.rs`:1-155; `model_http.rs`:18-25, 28, 46-55, 88-96, 132-142; `error.rs`:10-34;
`usecases.rs`:2701-2713, 2813-2818, 2868-2882, 3601; `push.rs`:236 (grep); `commands.rs`:166-169;
`lib.rs`:52; `Cargo.toml`s (reqwest 0.13.5, wiremock 0.6.5, url 2.5.8); spec :391-397.
GUESS — reqwest `resolve`/SNI interaction; whether goldens pin exact install-flow errors; whether
spec sanctions a connect timeout / user-agent; whether goldens assert exact OAuth form bytes.
