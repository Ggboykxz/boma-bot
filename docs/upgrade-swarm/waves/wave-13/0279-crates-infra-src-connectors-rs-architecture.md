# Upgrade proposal — `crates/infra/src/connectors.rs` (lens: architecture)

- Worker: 279
- Target version: v0.4.0
- Evidence basis: full read of the 281-line file plus targeted reads of `domain/src/connector.rs`
  (catalogue), `app/src/ports.rs` (`ConnectorRunner`), `app/src/usecases.rs` (`dispatch_connector_tool`),
  `infra/src/net_safety.rs`, `infra/src/lib.rs`, `spec/01-requirements.md` REQ-032/033 and
  `spec/05-known-defects.md` (D-05). Code facts are **CONFIRMED** by inspection; inferences are
  **GUESS**. Read-only worker: no code changed, no tests run.

## Current structure (as read)

- One `HttpConnectorRunner { http: reqwest::Client }` (30 s timeout, redirects off) implements the
  coarse `ConnectorRunner::run`: a `match (connector_id, tool)` with `("rss","read")` and
  `("http-json","get")` arms; anything else → `InvalidName("…does not have that tool")`. Caps:
  `MAX_BODY_BYTES` = 512 KiB (feed), `MAX_JSON_BYTES` = 16 KiB (JSON).
- `fetch()` guards then GETs; **the http-json arm re-implements the same guard + GET inline** to add
  the bearer header, duplicating URL check, content-type read, body read and size check.
- Feed parsing is hand-rolled string scanning (`parse_feed`, `tag_text`, `link_of`, `clean_text`:
  RSS 2.0 `<item>` + Atom `<entry>`, CDATA trim, six named entities). Three unit tests cover only
  parsing; none cover fetch, the guard path, the http-json arm or oversized bodies.
- Every failure — bad URL, private address, missing credential, oversized body, unknown tool — maps
  to `AppError::Rule(DomainError::InvalidName(…))`, surfaced at `usecases.rs:1065` as
  `{"error": "the connector refused: …"}`.

**Architecture gap:** the catalogue (`domain/connector.rs::builtin_connectors`) and the runner's
`match` are two independent sources of truth for which tools exist; adding a catalogue entry without
editing this match yields a runtime "does not have that tool". Both arms buffer the whole response
with `response.bytes().await` *before* checking the cap, so a hostile endpoint can force an
unbounded allocation. The `CheckedOutbound` returned by the guard is discarded (`fetch` line 43),
and no per-call telemetry exists.

## Target structure for v0.4.0

Keep `HttpConnectorRunner` as the public `ConnectorRunner` implementation (binary wires it at
`bin/commands.rs:146`; `lib.rs:44` re-exports it) but make composition deliberate:

```
HttpConnectorRunner { http, adapters: HashMap<(String,String), Box<dyn ConnectorAdapter>>, limits }
ConnectorAdapter (private trait)  // run(args, credential, &Client) -> Result<Value, ConnectorError>
RssReadAdapter / HttpJsonGetAdapter  // one per tool: feed parsing / bearer header + caps
ConnectorError (thiserror)        // typed; mapped to AppError once at the port boundary
ConnectorLimits { timeout, feed_cap, json_cap }  // validated, injected, defaults unchanged
```

Register adapters from `builtin_connectors()` at construction so catalogue and dispatch cannot drift;
keep the public port signature and `connector.<id>.<tool>` keys byte-identical.

## Concrete upgrades (with risks)

### 1. Replace the monolithic `match` with an adapter registry keyed off the catalogue (REQ-032)
**CONFIRMED:** dispatch is a hardcoded two-arm match, independent of `builtin_connectors()`. Change:
a private `ConnectorAdapter` trait, one adapter per tool, and a `HashMap` built by iterating the
catalogue's `tool_key()` values; `run` becomes one lookup then `adapter.run(...)`. Unknown key keeps
the exact current error string. Add a construction-time consistency check between catalogue and
registry. **Risks:** (a) must preserve the public `ConnectorRunner` signature and all wire shapes —
any change is a DEVIATION for `docs/PARITY.md`; (b) a strict startup assertion could block boot if a
catalogue entry has no adapter — **GUESS:** fail closed in tests, log-and-skip in production until
all adapters land; (c) `ScriptedConnectorRunner` in testkit implements the port, not the registry —
untouched, but verify its tests still pass.

### 2. Unify the guarded request and enforce caps while streaming (security/reliability)
**CONFIRMED:** the guard + GET is duplicated; `response.bytes().await` buffers the entire body before
either size check. Change: one `fetch_guarded(url, credential: Option<&SecretString>, cap)` primitive
that runs `check_outbound_url`, applies the bearer header only when present, and pulls `chunk()`
frames accumulating bytes with an early abort at `cap`. The rss adapter passes 512 KiB, http-json
16 KiB. **Risks:** (a) the http-json adapter must still return `{status, contentType, body}` and
keep parsing JSON-or-lossy-text exactly as now; (b) preserving the 30 s timeout and redirect policy
is mandatory (parity); (c) chunked reads change allocation profile but not output — prove with
wiremock that an oversized body is rejected without full buffering; (d) keep `net_safety`'s
documented per-call host-resolution model (D-05).

### 3. Introduce typed connector errors; map to `AppError` once at the port
**CONFIRMED:** six distinct failures all become `DomainError::InvalidName`. Change: a private
`thiserror::Error` enum (`BadArguments`, `BlockedUrl`, `Unavailable`, `TooLarge`, `NoCredential`,
`UnknownTool`), converted to `AppError` in one `From` impl at the `ConnectorRunner` boundary, with
the **same human-readable strings** so `usecases.rs:1065` stays byte-identical. **Risks:** (a) any
message change is externally visible — lock the current strings with golden-style assertions before
refactoring; (b) do not leak resolved addresses or the guard's reason verbatim if it contains
internal detail; keep the existing reason text for parity but route it through the typed variant;
(c) keep `AppError::Unavailable` for network failures unchanged.

### 4. Replace or fuzz-harden the feed parser (correctness; AGENTS.md parser rule)
**CONFIRMED:** `block_end` uses `min("</entry>".len(), "</item>".len())` (8 vs 7), an off-by-one for
Atom; `parse_feed` takes the first `</item>`/`</entry>` regardless of the active marker, and
`clean_text` decodes only six named entities (no numeric `&#NNN;`/`&#xHH;`). AGENTS.md requires a
property/fuzz test for every external-format parser, and there is none. Change: add `proptest`
cases first (random tag soup must never panic and must respect `limit`), then either adopt a vetted
crate (`quick-xml` / `roxmltree`, added with `cargo add` + docs.rs + `docs/DEPENDENCIES.md`) or fix
the scanner in place with a real tokenizer for entities. **Risks:** (a) a DOM parser changes
whitespace/CDATA semantics — preserve `title/link/summary` and the "malformed feed returns what
parsed" behavior exactly (DEVIATION check); (b) new dependency needs license review; (c) keep
`req_032_*` test names in the trace gate.

### 5. Add per-call telemetry and inject typed limits/config instead of magic constants
**CONFIRMED:** only two `tracing::warn!` lines exist; caps/timeout are module constants and no
structured span records connector, tool, status, bytes or latency. Change: one `tracing::info_span!`
per call with `connector`, `tool`, `status`, `bytes`, `elapsed_ms` (never the raw URL or token) and
a `ConnectorLimits` struct injected at `new()`, defaults 30 s / 512 KiB / 16 KiB. **Risks:** (a) no
secrets/PII in logs — URLs may carry query tokens; log only the host that `CheckedOutbound` already
validates (**GUESS**); (b) keeping defaults identical avoids a DEVIATION; (c) avoid logging inside
the credential-bearing adapter; (d) cheap counters/timings must not become a DoS vector themselves.

## Risks & ordering

- #3 and #1 are behavior-preserving if strings/keys stay fixed — do them first (with #4's proptest
  scaffold), then #2, then #5. The spec mandates only catalogue/install behavior (REQ-032), not
  these internals (**GUESS**), so structural refactors must stay invisible to clients.
- Any wire/string/key change is a DEVIATION: log in `docs/PARITY.md` and wait for owner approval
  (`AGENTS.md`). Keep `req_032_*` names and the `lib.rs:44` re-export stable.
- Re-run `cargo fmt --check`, `clippy -D warnings` and tests in the implementing session — none
  of that was run by this reader.

## Tests to add (REQ ids in names)

- `req_032_registry_matches_catalogue_and_unknown_tool_error_is_stable`.
- `req_032_oversized_body_is_rejected_while_streaming` (wiremock, both caps).
- `req_032_http_json_keeps_status_content_type_and_body_shape`.
- `req_032_feed_parser_never_panics_on_arbitrary_bytes` (proptest) + Atom/RSS entity cases.
- `req_032_connector_call_emits_structured_span_without_url_or_token`.

Status: proposal only; no code changed (READ-ONLY worker).
