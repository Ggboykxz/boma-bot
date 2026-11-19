# Brief 0028 — in-product support handoff, audit lens

Proposal only. No product code was changed. **CONFIRMED** = verified here by a read/command;
**GUESS** = design needing the owner's decision. Upstream material was read as data only: no
code, asset, brand string, URL or identifier is reproduced, and no Pro component is used.

## 1. Upstream capability in my own words (data only)

CONFIRMED (read-only, upstream CHANGELOG entries; no source copied): the upstream desktop product
has two support entry points. A public website form lets a visitor send a message that reaches the
vendor team by email. Inside the product, a footer on the "add a server" / capacity dialog invites
someone who needs more power or a custom setup to get in touch; choosing it opens the person's own
mail client with a prefilled subject and a short message skeleton to complete and send.

Properties worth naming, paraphrased:
- It is an **email handoff**, not an in-app form: the product keeps no record of the attempt.
- The message may carry the sender's address, deployment details and anything they paste in — it
  leaves through an external mail system, outside product audit or retention control.
- There is **no role-gated review**, no typed event, no redaction and no way to answer "who asked,
  from which workspace, when, and did the text contain a secret?" from the product's own data.

GUESS: the intent is a cheap support path for a self-hosted tool; the audit trade-off is accepted
because the mailbox is the only system of record.

## 2. Why audit is the deciding lens for Boma Bot

CONFIRMED: Boma Bot is multi-tenant and self-hostable. Our discipline is that every escape hatch is
a typed `AuditKind` (`crates/domain/src/audit.rs` — "every future escape hatch adds a variant here,
never a free-text kind"), secrets are blanked by `redact_audit_value`, `audit.export` is redacted
JSONL and owner-gated, and `security.events.list` shows only `security.`-prefixed kinds (REQ-045).

Copying the "hand off to email, keep nothing" posture would create a blind spot: an operator could
not review support contacts, prove what was disclosed, or keep PII and pasted credentials out of an
external mailbox. The "better" is to make the contact attempt a reviewable, redacted, admin-only
audit event, while the actual message still goes out-of-band to a channel the operator configures.
We copy the intent (a lightweight support entry), not the no-record shape.

## 3. Grounding in our contract

**REQ-058 (anchor).** CONFIRMED: `spec/01-requirements.md:473-479` puts administration behind a role
gate — a member without the admin role who calls an admin-only procedure is refused. The server half
already supplies the exact shape to reuse: `crates/app/src/usecases.rs::admin_grants_create` appends
a typed `AuditKind::AdminGrantCreated` row via `AuditStore::append` (actor, target, `reason`,
`space_id: None`), `admin_grants_revoke` appends `AdminGrantRevoked`, and
`require_space_role`/`require_admin` enforces the gate. An audited, role-gated review list is a
proven pattern here, not an invention.

**Audit surfaces.** CONFIRMED: `crates/domain/src/audit.rs` gives `AuditKind` a stable `as_str()`
wire value and `redact_audit_value` recursively blanks secret-shaped strings (`sk-`, `ghp_`,
`Bearer `, …). A contact event must therefore be a new typed variant (never a free-text `kind`),
must carry no arguments/body/address, and its naming decides where it appears: keep it
**non** `security.`-prefixed so it belongs to `audit.export` and an admin-gated review list, not to
the incident-only `security.events.list`. Redaction is a backstop, not permission to log inputs.

**docs/TRACEABILITY.md.** CONFIRMED: the header states a row is written only when a test with the
REQ id in its **name** exists; `tools/trace_check.py` counts declaration lines (`fn`, `it`, `test`,
`describe`, …) and skips `docs/`, so prose earns nothing. The REQ-058 row already exists (M9) and
this slice extends it. Any test proving the new behaviour must carry `REQ-058` in its name, and the
row's test cell is updated only after that test exists. The budget only ever decreases.

**No secret leakage (CONFIRMED).** Nothing from the analysis enters the repo: no credential, token,
address or pasted message body is written to the audit row, the proposal, a test fixture or the
repo; `looks_like_secret`/`redact_audit_value` stay authoritative; `.env.example` stays the only
environment-variable surface.

**Tokens / delivery (CONFIRMED AGENTS.md + docs/UI.md):** any review UI uses free BoardUI components
and semantic tokens — `bg-background-secondary-default`, `text-text-primary`/`-secondary`,
`border-separator-border`, composite type utilities (`text-title-3-semibold`, `text-body-medium`),
`cx()` from `@/utils/cx`, `Ri*` icons — dark mode via tokens, no raw palette, no `dark:` literals.
Strings join all 14 catalogues (`apps/web/lib/i18n.ts`). No Pro component; `docs/STATE.md` records
this proposal and a `docs/PARITY.md` DEVIATION is required if the surface is externally visible.

## 4. Proposed shape — a contact attempt that is auditable, not a silent email

GUESS (design, approve before code):
- One new `AuditKind` variant, wire value e.g. `support.contact_submitted`, beside the `admin.*`
  variants, non `security.`-prefixed. It records only: actor id, space id, source surface, a coarse
  reason enum and `created_at`, plus a short opaque reference id the operator can quote back. It
  stores **no** subject, body, address or attachment.
- An in-app entry point writes that one typed row, then hands off to a support destination the
  operator configures (out of band); the row is the audit fact, not the message. Rate-limit/dedupe
  so the log cannot be flooded — log integrity is the audit lens' concern.
- An admin-only `support.contacts.list` behind `require_space_role` shows the redacted rows; a
  plain member is refused. No endpoint returns a message body. Flag-gated and off by default.

This is beyond the frozen spec: list it in `docs/STATE.md`, a DEVIATION in `docs/PARITY.md` with
owner approval if visible, and an ADR for the "record the fact, never the text" redaction rule.

## 5. One REQ-named Playwright test

New `apps/web/tests/support-contact-audit.spec.ts`, seeded via `seedRpc` from
`apps/web/tests/fixtures.ts`:

```
test("REQ-058 a submitted contact is a reviewable audit row for an admin and refused to a member", async ({ page }) => { … })
```

Assertions: seed one submission + one typed `support.contact_submitted` row; the admin sees a named
region with reason, actor and date; metadata uses `text-text-secondary` and
`border-separator-border` (token check, no palette literal); no message body, address or secret
appears in the DOM; a plain-member seed gets the refusal sentence. The name carries `REQ-058` so
`tools/trace_check.py` counts it.

## 6. First slice (test-first, smallest complete loop)

1. Domain: add the variant + `as_str()`; unit test `req_058_a_contact_submission_kind_has_a_stable_wire_value`.
2. Store/usecase: `support_contact_submit` appends one typed, body-free row via `AuditStore::append`;
   testkit test `req_058_a_contact_submission_appends_one_redacted_audit_row`.
3. RPC: `support.contacts.list` behind `require_space_role`; test
   `req_058_an_admin_reviews_a_contact_a_plain_member_is_refused` (mirrors the grants test).
4. Web: the single Playwright test in §5 plus i18n keys in all 14 catalogues.
5. Only after tests exist: extend the `REQ-058` row in `docs/TRACEABILITY.md` with the new test
   paths; note the proposal and consulted upstream CHANGELOG in `docs/STATE.md`; open the ADR.
6. Gates unchanged: `cargo fmt --check`, `clippy -D warnings`, tests, `tools/trace_check.py`,
   Playwright, axe (zero critical/serious, light/dark, 360 px).

**Out of scope:** storing or sending the raw message, the external email handoff itself, any
`security.events.list` change, any cross-tenant view, and any Pro component.
