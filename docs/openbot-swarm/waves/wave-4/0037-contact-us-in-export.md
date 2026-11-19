# 0037 — contact-us-in × export parity proposal (Boma Bot)

- Brief: `id 37`, `contact-us-in` × `export` (`docs/openbot-swarm/briefs-1000.jsonl`). PROPOSAL only; no code changed.
- Provenance: upstream read as **data only**. Clone at `/tmp/opencode/openbot-parity` (the brief's
  `/tmp/openbot-parity` does not exist), HEAD `559d5ac`; consulted: `SUPPORT.md`,
  `contact-message.ts`, `contact.ts`, `email-templates.ts` (account service),
  `src/main/maintenance-service.ts`, `CHANGELOG.md`. No upstream code, asset, wording, address or
  brand enters Boma Bot; no Pro component. Claims are **CONFIRMED** (read/run) or **GUESS**.

## 1. Upstream capability, in my own words (data only)

The upstream product gives people **three** contact routes, and none of them auto-sends workspace data:

- A public-site contact form: name, address, company, a topic enum and a message; a same-origin POST,
  rate-limited per source address and in total inside a short rolling window (keys stored hashed);
  the message is delivered as mail to the vendor team; the product keeps nothing but the counters;
  no attachment mechanism exists — the visitor cannot attach a file at all.
- In-app "Contact us" entry points (server-add and licence flows): open the person's **own mail
  client** with a prefilled subject and skeleton body; the app sends and stores nothing.
- The support companion is a **diagnostics export**: the user explicitly saves, via a save dialog, a
  dated JSON file with a schema version, generation time, app version and platform, agent phase,
  authentication kind, capability flags, agent counts, per-agent queue-delivery **counters**, MCP
  server counts and transports, browser tab counts, memory/CPU resource summary, update history and
  a trace summary. It carries an explicit declaration that it holds no conversations, addresses,
  tokens, file contents or paths, and is written owner-only. The person alone decides whether to
  attach it to a report; the upstream changelog records that secret classes (tokens, private keys,
  cookies, credential-bearing URLs, provider keys) were stripped from logs, diagnostics and exports.

GUESS: the intended contract is "the person exports redacted **facts**, never content; the transport
is the person's own choice." What is deliberately absent is also the design: no auto-attachment, no
"send my logs" button, no account archive handed to support.

## 2. Why export is the deciding lens here

Boma Bot can hand a person their whole account (REQ-044) and hand an operator the space's audit trail
(REQ-045) — but there is **no scoped, redacted support bundle**. An operator who wants to help a
stuck member must either send the account archive (everything the member owns, including message
bodies — far too broad and not third-party safe) or paste ad-hoc text (unauditable, leak-prone).
Export parity means carrying the upstream's discipline: a small, explicit, redacted JSON artifact,
chosen and shared by the person, with the fact of its creation recorded in the audit trail.

## 3. Grounding in our contract

**REQ-044 (anchor).** CONFIRMED `spec/01-requirements.md`: export = archive with a manifest; import
refuses unknown version; deletion revokes sessions. CONFIRMED `crates/app/src/account_export.rs`:
`version: 1` manifest, 8 MiB per-entry / 32 MiB total budgets, refused sections reported never
silently dropped, archive rendered on demand (`render_zip` over an in-memory `Cursor<Vec<u8>>`);
`crates/api/src/public.rs` serves `GET /api/v1/me/exports/{id}.zip` cookie-gated, `no-store`,
`nosniff`; `export.start|status|download` live in `crates/api/src/rpc.rs`. Honest note: the archive is
**buffered, not streamed** today; a truly streamed response body is a GUESS improvement, out of this
slice. The bundle must reuse the same discipline, scaled down: explicit schema version, budgets,
refusal reporting, cookie-gated download, no silent drops.
**REQ-045 (audit JSONL).** CONFIRMED `crates/app/src/usecases.rs::audit_export`: owner/admin gate via
`require_space_role` (SPACE_ADMINS), 4000-row cap with a `truncated` flag, JSONL with a `version: "1"`
header line, oldest first, every metadata value passed through `redact_audit_value`;
`crates/domain/src/audit.rs` defines `looks_like_secret`/`redact_audit_value` and stable `as_str()`
wire values; non-`security.`-prefixed kinds belong to `audit.export`, never to `security.events.list`.
**docs/TRACEABILITY.md.** CONFIRMED: a row moves only when a test with the REQ id in its **name**
exists; `tools/trace_check.py` counts declaration lines (`fn`/`it`/`test`/…) and skips `docs/`. The
REQ-044 row exists (M9); it may be extended only after the test in §5 exists.
**No secret leakage (CONFIRMED).** The bundle carries only counts, versions, enums and platform
strings — never message bodies, addresses, hostnames, paths, tokens or environment values. Every free
string passes `redact_audit_value`; the serialized JSON is scanned by `looks_like_secret` and the
build fails closed on a hit. The bundle states its own "no personal content" declaration. Desktop
writes it owner-only. `.env.example` stays the only env-var surface. Nothing secret enters this
proposal, the repo, a fixture or a test name.
**Tokens / delivery.** Free BoardUI only; semantic tokens (`bg-background-secondary-default`,
`text-text-primary`/`-secondary`, `border-separator-border`), composite type utilities, `cx()`,
`Ri*` icons; dark mode rides tokens; all 14 catalogues in `apps/web/lib/i18n.ts`; no Pro component;
note in `docs/STATE.md`, a `docs/PARITY.md` DEVIATION if the surface is visible, one ADR for the
"record the fact, never the content" rule.

## 4. Proposed shape (GUESS, approve before code)

- New session-gated RPC `export.support_bundle` (the caller's **own** facts — no admin role; unlike
  `audit.export` it carries nothing about other members or the audit trail itself).
- Returns one small JSON document: `schemaVersion: 1`, `generatedAt`, app version, platform/arch,
  capability flags, auth kind, per-bot run/queue **counters**, bot/thread/message **totals** (no
  names or ids), artifact and audit **counts**, provider **configured/not** booleans. No hostnames,
  IPs, paths or free text.
- One typed audit row per export — `support.bundle_exported` (non `security.`-prefixed) — with actor,
  space, schema version and counts only; never the bundle. The person attaches the file manually;
  nothing is sent, stored or emailed by the product.
- No change to the account archive or `audit.export`; the two paths stay orthogonal: REQ-044 for the
  person's bytes, REQ-045 for the operator's trail, this bundle for support.

## 5. One REQ-named test

`crates/testkit/src/build.rs`, `account_export_tests` module:

```
#[test] fn req_044_a_support_bundle_carries_no_message_body_no_address_and_no_secret_shaped_value() { … }
```

Seed an account with messages, artifacts and audit rows whose metadata contains secret-shaped strings;
build the bundle; assert: `schemaVersion == 1`; totals match the fixture; no fixture message body,
address or secret-shaped substring appears anywhere in the serialized JSON; exactly one typed audit
row was appended; `audit.export` remains refused to a plain member. The name carries `REQ-044` so
`tools/trace_check.py` counts it.

## 6. First slice (test-first, smallest complete loop)

1. Domain: pure `SupportBundleFacts` → JSON renderer in a new `crates/domain/src/support_bundle.rs`;
   unit tests (no REQ id in names).
2. Usecase `support_bundle_export` in `crates/app/src/usecases.rs`: aggregate counts under the tenant
   context, render, `redact_audit_value` the whole document as backstop, fail closed on
   `looks_like_secret`, append the typed audit row via `AuditStore::append`.
3. RPC `export.support_bundle` in `export` namespace (`crates/api/src/rpc.rs` + `wire.rs`) behind the
   same session gate as `export.start`; golden `support-bundle` pins shape, not values.
4. Web: one "Export diagnostics" action on the settings/support panel behind the existing diagnostics
   flag, semantic tokens, i18n ×14; axe, light/dark, 360 px.
5. Only after §5 exists: extend the `REQ-044` row in `docs/TRACEABILITY.md` with the new test path;
   record proposal + consulted upstream changelog in `docs/STATE.md`; open the ADR.
6. Gates unchanged: `cargo fmt --check`, `clippy -D warnings`, full tests, `tools/trace_check.py`,
   Playwright, axe.

**Out of scope:** any change to the account archive or `audit.export`; sending/emailing the bundle;
including hostnames, IPs, paths or audit rows; streaming the archive response (separate decision);
message bodies of any kind; Pro components; upstream wording, brand, addresses or URLs.
