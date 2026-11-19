# 0241 — `crates/domain/src/mcp.rs` — architecture (v0.4.0)

- Worker: 241 · Lens: architecture · Target: v0.4.0 · Proposal only; no code changed.
- Module: 313 lines, pure `std` domain rules for REQ-033 (one named MCP server per space).
- Spec anchors: `01-requirements.md:271` (remote HTTP + OAuth, opt-in local stdio);
  `05-known-defects.md:20` D-05 (private endpoints via deployment-wide flag: scope per space,
  explicit allow-list, audit each use); `AGENTS.md` "Types" (newtypes, invalid states
  unrepresentable), "Errors" (typed errors), "Tests" (property/fuzz for every external parser).
- Read this session: `mcp.rs`, `lib.rs:12-43,79-82`, `error.rs`, `acp.rs`, `connector.rs`, `ids.rs:1-70`,
  `app/ports.rs:2308-2360`, `app/usecases.rs:581-650,7190-7279`, `spec/01-requirements.md:271-278`,
  `spec/05-known-defects.md:14-23`, plus a grep of MCP call sites across all crates.
- CONFIRMED = read in this session; GUESS = inferred.

## Structure today (CONFIRMED)

- One enum `McpTransport { Http, Stdio }` with `as_str`, `Display`, `FromStr` — already symmetric.
- Five free functions over raw `&str`/`String`: `validate_mcp_server_name` (lowercase-kebab, ≤48,
  no dots), `sanitize_mcp_tool_name` (≤64, `[A-Za-z0-9_.-]`, no `..`/edge dot, returns owned
  `String`), `validate_mcp_server_url(url, allow_private)` (HTTPS unless flag, no credentials,
  four literal metadata hosts), `split_command` (whitespace tokenize + char allow-list),
  `program_allowed(program, allowed)` (basename-or-full-path equality).
- Every failure is `DomainError::InvalidName` — one catch-all for shape, parse and policy errors.
- Re-exported at `lib.rs:79-82`. Callers confirmed in `app/usecases.rs` (add/probe/dispatch),
  `infra/mcp_http.rs`, `infra/mcp_stdio.rs`, `testkit`, `api/rpc.rs`.
- Four `#[cfg(test)]` modules, all `req_033_*`; no proptest/fuzz despite three external parsers.

## Architectural findings (CONFIRMED unless noted)

1. **No validated types.** `validate_mcp_server_name` is called at `usecases.rs:592` and the value is
   then stored as bare `String`; `sanitize_mcp_tool_name` returns `String`. Nothing makes an
   unvalidated name/URL/command unrepresentable, so every caller must remember the check.
2. **Tool-key grammar lives outside the domain.** `approved_mcp_call` builds `mcp.<server>.<tool>`
   with `strip_prefix("mcp.")` + `splitn(2, '.')` (`usecases.rs:7195-7211`), re-sanitizes the tool,
   then resolves the server by equality against `summary.name` (the *display* name). Encode/decode
   and grammar are duplicated at probe/filter sites (`usecases.rs:896`, `infra/mcp_http.rs:174`).
3. **Naming/validation asymmetry.** `sanitize_mcp_tool_name` refuses rather than rewrites despite
   its name, and allows uppercase and `_` while server names forbid them. `split_command`'s doc
   claims it refuses a program that is neither absolute path nor bare name; the implementation
   checks only characters — a doc/impl drift worth resolving explicitly.
4. **Shallow SSRF guard.** URL validation is four exact host strings; trailing dot, decimal/hex
   IPv4 encodings, IPv6 forms, subdomains and redirects are not covered. `allow_private: bool` is a
   deployment-wide decision passed into the domain; D-05's per-space/allow-list/audit obligation is
   implemented in `usecases.rs:639-647,7229-7238`, not expressible in the domain signature.
5. **Flat error type.** Callers cannot distinguish "bad shape" from "refused by policy" without
   cross-module knowledge (`AppError::StdioDisabled` is used for the latter in `usecases.rs:601`).

## Concrete upgrades for v0.4.0 (each with a risk)

1. **Validated newtypes: `ServerName`, `ToolName`, `ServerUrl`, `StdioCommand`.** Follow
   `ids.rs::opaque_id!` style: `try_new` runs today's checks once, `as_str`/`Display` preserve the
   wire text; keep the five free functions as thin compatibility shims.
   Risk: largest blast radius — `ports.rs` fields, store SQL bindings, testkit and API DTOs change.
   Mitigate with staged constructors/accessors and byte-identical strings; land as one atomic commit.

2. **`McpToolKey { server, tool }` with paired `parse`/`Display`.** One domain home for the
   `mcp.<server>.<tool>` grammar, replacing the ad-hoc `splitn` and the re-sanitize in
   `approved_mcp_call`.
   Risk: tool names may contain dots (`issues.list`), so `splitn(2, '.')` is load-bearing; port it
   verbatim, add round-trip and rejection tests, and keep current error text (clients may match it).

3. **Typed MCP error variants instead of the `InvalidName` catch-all.** Add closed variants
   (`InvalidMcpName`, `InvalidMcpUrl`, `InvalidMcpCommand`, `McpEndpointNotAllowed`) and pin all
   current `Display` strings in a golden matrix first — see sibling proposal 0227 (`error.rs`).
   Risk: `DomainError` is workspace-exhaustive, so every match in `app`/`api` must be updated;
   keep messages byte-identical or file a `docs/PARITY.md` DEVIATION before shipping.

4. **Replace `allow_private: bool` with `PrivateEndpointPolicy` (typed, per-space allow-list).**
   Make D-05 representable at the domain decision boundary; normalize hosts (lowercase, strip
   trailing dot), refuse private/loopback/link-local literals unless listed, while leaving DNS
   resolution and the audit event in `app`/`infra` so the domain stays pure.
   Risk: stricter defaults can refuse URLs accepted today → DEVIATION and owner approval; needs a
   migration rule for existing `allow_private` rows and tests for each bypass encoding (GUESS:
   currently no such tests).

5. **Command model + parser hardening.** Make `program_allowed` take a validated `StdioAllowList`,
   fix the `split_command` doc/impl drift (decide absolute-path-or-bare-name enforcement), and add
   proptest/fuzz for the URL, command and tool-key parsers per `AGENTS.md`.
   Risk: enforcing the documented path rule may break relative commands such as `./bin/mcp` —
   confirm with the owner before tightening; newtypes ripple into `infra/mcp_stdio.rs`.

## Sequencing and open questions

- Sequence: 3 → 2 (error/key, low functional risk) → 1 (types) → 5 (parsers) → 4 (policy + parity note).
  Every step: REQ-033 in test names, then `cargo fmt --check`, clippy `-D warnings`, tests.
- Decision needed: is the stricter private-endpoint policy a DEVIATION needing approval? (Likely.)
- Decision needed: is resolving the server by display name (`usecases.rs:7215`) intentional? GUESS no.
- GUESS (unconfirmed): domain crate is `std`, so `LazyLock`/proptest dev-deps are acceptable.
- Non-goals: no IO/DNS/clock in this module, no HTTP/API envelope, no new feature outside REQ-033,
  no wire/DB format change without `docs/PARITY.md` and owner approval.
