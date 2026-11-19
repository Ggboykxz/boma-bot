# 0039 — contact-us-in × privacy (parity proposal, Boma Bot)

- Brief: `id 39`, area `contact-us-in`, lens `privacy` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only**. No product code, spec, golden, catalogue, dependency or brand change.
- Provenance: upstream read as **data only** (`/tmp/opencode/openbot-parity`, HEAD `559d5ac`):
  `routes/v1/contact.ts`, `server/*contact*|*analytics*|*auth-repository*`, `components/contact/*`.
  No brand, wording, address or asset crosses over.

## 1. Upstream capability, in my own words (inspiration only)

A no-sign-in public "contact us" door: (optional name/company, required email, fixed topic from a
four-entry catalog, message ≤ 2000 chars), POST same-origin JSON. Privacy-relevant, generic:

- Cross-origin posts are refused (403); name/company coerced to single lines ≤ 120 chars
  (header-injection guard), email normalized, topic must resolve or the post is invalid.
- 15-minute-window rate limit: per-sender 3, global 15, keyed by `sha256("contact:ip:"+ip)` stored
  as `key_hash` in an `auth_rate_limits` table; over-limit → 429 + retry hint.
- **Nothing persisted.** The message is delivered as email via SMTP (Cloudflare Email fallback),
  `replyTo` = sender, `Auto-Submitted` headers; no message row.
- Analytics: coarse referrer attribution to hosted OpenPanel; `contact_form_submitted{topic?,result}`
  and `email_sent{kind:"contact",result}` (no account id). Never name/email/message/IP.
- Informational privacy note + link by the send button; no consent checkbox.
- Honest weaknesses (data, not to copy): an **unsalted** hash of a low-entropy IPv4 space is
  brute-forceable; the SMTP relay and the analytics host are third parties seeing content/metadata.

## 2. Privacy parity grounded in local-first (CONFIRMED)

REQ-066 (`:524`): loopback bind, operator's machine the only trust anchor, no sign-in round trip. That fixes scope:

1. **No vendor endpoint, no phone-home.** Local-first has no hosted OpenPanel. The affordance is
   **local**: a `mailto:` to the deployment's configured support address, or a relay **the operator
   configured and owns** — mail leaves only to a destination under the operator's own agreement.
2. **Local-first ⇒ source "IP" is the operator.** IP limiting is meaningless on loopback; any
   limiter keys on the authenticated actor (or is skipped). A future public deployment uses a
   **keyed** hash with a server-side secret — never the unsalted upstream shape.
3. **Minimal, enumerated fields.** If a form is adopted: optional name/company, required email,
   fixed topic catalog, message — nothing else (no referrer/attribution/fingerprint/UTM).
   Same-origin only; single-line coercion and catalog validation as generic hygiene.
4. **The notice is honest and local.** One line by the button stating where the mail goes (the
   operator's relay) and that nothing is stored locally — BoardUI tokens, `t()` copy, `<bdi>` ids,
   link to the deployment's own privacy doc. No consent theatre.

**GUESS:** no contact route or support RPC exists today — the `Support` nav item is a dead `href="#"`
(`dashboard-sidebar.tsx:533`, `NavItem`'s `href` default). Destination (mailto vs relay vs `/contact`) is an owner decision.

## 3. Secrets are never logged (CONFIRMED)

REQ-046 (`:373`): a secret is usable by a tool but "not returned in any message or log"; REQ-053
(`:433`): readiness without secret leaks; REQ-045 (`:365`): audit rows export without secrets.
`crates/domain/src/audit.rs:79-147` (`redact_audit_value`) is the export backstop;
`crates/api/src/wire.rs:1486` redacts at the wire boundary. Pins:

- A contact submission is **never** logged with body/name/email/message — only outcome, fixed
  topic id, correlation id. The message is user content, not a secret, but it is logged nowhere.
- Redaction is a backstop, not license to log: a secret-shaped token pasted into a message stays
  `[redacted]` in any export — never a raw echo. Audit rows keep actor/space/target/metadata only.

## 4. Content-free notifications (REQ-064, CONFIRMED)

REQ-064 (`:512`) pushes only on park/complete/fail; payload exactly `{v,kind,title,tag,url}` with a
fixed three-title catalog, pinned by `req_064_payload_is_content_free` (`crates/testkit/src/build.rs:9471`);
per-bot policy, UTC-day cap, dead-subscription deletion; REQ-065 (`:610`) destinations carry the same
five keys — never tool names, message text or user names. Contact parity pins:

- The contact surface raises **no push by itself**. If a "message to support sent" confirmation is
  ever wanted, it is in-app only, or a REQ-064 five-key push — **never** the message, topic label,
  email, name, thread or tool; policy and daily cap unchanged; destinations get the same five keys.
- The contact flow never enriches a REQ-064 push; nothing about support mail travels in any
  notification, on either sink. Result enums (`succeeded|failed|rate_limited`) are the only
  enumeratable signal.

## 5. One REQ-named test

New `apps/web/tests/contact-us-privacy.spec.ts`, distinct from `notifications-panel.spec.ts`:

```ts
test("REQ-064 the contact flow emits no content push, no third-party egress, no secret leak", async ({ page }) => {
  await applySeed(page, "thread-empty");
  const seen: Array<{ url: string; body: string | null }> = [];
  page.on("request", (r) => seen.push({ url: r.url(), body: r.postData() }));
  await openSupportSurfaceAndType(page, "sk-live-SECRET-VALUE hello"); // GUESS: bridge stubbed as in notifications-panel.spec.ts
  await submit(page);
  for (const r of seen) {
    expect(r.url).not.toMatch(/analytics\.|openpanel|openbot\.run/i); // local-first: no third-party analytics
    expect(r.body ?? "").not.toContain("sk-live-SECRET-VALUE");        // secret never leaves in egress
  }
  expect(await page.locator("body").innerText()).not.toContain("sk-live-SECRET-VALUE"); // never re-rendered raw
});
```

Pins the lens under the REQ-064 name (`tools/trace_check.py`). Keep the name and assertions; adjust selectors only.

## 6. First slice (test-first, small, reversible)

1. Write the failing REQ-064 test above; stub the support RPC/bridge until the surface lands.
2. **No transport yet:** give `Support` a real, local destination — owner decision between a
   `mailto:` (zero new infra) and the operator's configured relay — never `href="#"`; adopt sibling
   0026's `/contact` page only if the owner wants it.
3. If a page is adopted: §2 fields only, BoardUI semantic tokens + `.boma-focus`, `t()` copy, privacy
   line linking the deployment's own doc, no analytics call of any kind, same-origin submit.
4. Logging/audit unchanged in shape; redaction backstop stays; no secret or content in any line.
5. Contract note in `docs/STATE.md`; no new REQ, RPC or storage without owner approval (AGENTS.md);
   external behaviour change ⇒ DEVIATION in `docs/PARITY.md` first.
6. Gates: web typecheck/lint/build, Playwright (REQ-064 + `notifications-panel.spec.ts`), axe
   light/dark/RTL at 360 px, `tools/trace_check.py`; no dependency change.

## 7. Boundaries

No upstream code, asset, wording, address or brand; upstream is an oracle, never a source. No
third-party analytics, relay or inbox; no Pro, one design system; no new REQ/RPC/storage without the
owner; no raw IP or unsalted hash; pushes never carry content; nothing ships from this proposal alone.

## 8. Evidence

CONFIRMED — `spec/01-requirements.md` REQ-045:365, REQ-046:373, REQ-053:433, REQ-064:512, REQ-065:610,
REQ-066:524; `spec/04-nonfunctional.md` §4.5/§4.6; `crates/domain/src/audit.rs`, `crates/testkit/src/build.rs:9471`,
`crates/api/src/wire.rs:1486`; `docs/adr/027-notifications.md`; `notifications-panel.spec.ts`; `dashboard-sidebar.tsx:533`;
upstream files listed in the header (data only). GUESS — support destination, surface shape, test hooks, storage, owner decisions.
