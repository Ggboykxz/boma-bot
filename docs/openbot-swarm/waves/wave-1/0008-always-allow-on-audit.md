# Brief 0008 — remembered approval exemption, audit lens

Proposal only. No product code was changed. Claims are marked **CONFIRMED** (verified here by a
read/command) or **GUESS** (inferred). Upstream material was read as data; no upstream code,
asset, wording, identifier or brand is reproduced, and no Pro component is used.

## 1. The upstream capability in my own words (data only)

CONFIRMED (read only, `/tmp/opencode/openbot-parity/docs/architecture/{structure,computer-use,agents}.md`
and its `CHANGELOG.md`): the upstream desktop agent client lets a person answering a tool-permission
card record that decision so future calls of the same class in that context stop asking. Its
properties, paraphrased:

- The remembered choice is **client-side**: the server/team boundary freezes approval replies to
  accept/decline and stores no exemption; the client keeps the choice, keyed by tool and provider
  account, and replays it after a restart.
- It is **tier-scoped** ("ask first" only) and not irrevocable: on a config change, choices downgrade into warnings, never silent application.
- It covers only tool-consent prompts; forms, sign-in requests and screen-takeover prompts stay
  interactive and cannot be remembered.

GUESS: the underlying motivation is friction reduction for repeated identical tool calls, with the
client accepting the security trade-off locally rather than centralising it.

## 2. Why audit is the deciding lens for Boma Bot

CONFIRMED: our approval gate is real but strictly per-decision. `REQ-007` names the card region and
effect/reversal (`apps/web/tests/approval-card.spec.ts`), `REQ-017` records one answer once
(`crates/app/src/usecases.rs::answer_card`, `apps/web/tests/approval-card.spec.ts`), and `REQ-059`
relays the same gate for external agents. There is no remembered exemption and no audit kind for
granting one (`crates/domain/src/audit.rs`, 18 kinds listed).

The upstream "store nothing" posture is what an audit lens must refuse to copy: a silent bypass
leaves no record of *who* widened a permission, *what* effect, *when*, or *from which surface*.
Our "better" keeps the convenience but makes the remembered decision a reviewable grant.

## 3. Grounding in our existing contract

**REQ-058 (anchor).** CONFIRMED: `spec/01-requirements.md:473-479` puts admin-only surfaces behind a
role gate. The server half already provides the exact pattern to reuse —
`crates/app/src/usecases.rs::admin_grants_create` (line ~9083) appends a typed
`AuditKind::AdminGrantCreated` row through `AuditStore::append` (actor, target, `reason`,
`space_id: None`), `admin_grants_revoke` appends `AdminGrantRevoked`, and `require_admin` enforces
the gate. An audited grant is already a proven shape here, not a new invention.

**Existing audit surfaces.** CONFIRMED: `crates/domain/src/audit.rs` defines `AuditKind` with a
stable `as_str()` wire value, and the file comment states the rule: "every future escape hatch adds
a variant here, never a free-text kind." `redact_audit_value` recursively blanks secret-shaped
values; `REQ-045` exposes `audit.export` (JSONL, redacted) and `security.events.list` (only
security-prefixed kinds, owner/admin gate). So a consent event must be a new typed variant (never a
free-text `kind`), carry no arguments or secrets, and its naming decides which surface shows it.

**docs/TRACEABILITY.md.** CONFIRMED: a row is written only when a test whose *name* carries the REQ
id exists; `tools/trace_check.py` counts declaration lines (`fn`, `it`, `test`, …) and skips `docs/`,
so prose earns nothing and the budget only decreases. Tests must carry `REQ-058` in the name; the
traceability row is added only after they exist.

**BoardUI tokens.** CONFIRMED (AGENTS.md fenced block + `docs/UI.md:19-48`): any consent-review UI
uses `bg-background-secondary-default`, `text-text-primary`/`-secondary`, `border-separator-border`,
composite type utilities (`text-title-3-semibold`, `text-body-medium`), `cx()` from `@/utils/cx`,
and `Ri*` icons from `@remixicon/react`. Tokens flip in dark mode; no raw palette or `dark:`
literals. Existing keys `thread.card.approve` / `thread.card.region` show the i18n layer all new
strings join (14 locales, `apps/web/lib/i18n.ts`).

## 4. Proposed shape — a remembered consent that is auditable

GUESS (design, to be approved before any code):

- Two new `AuditKind` variants: `approval.consent_granted` and `approval.consent_revoked`, added
  beside the `admin.*` variants. Keep them **non** `security.`-prefixed: they belong to the full
  `audit.export` record and an owner-gated consent list, not to `security.events.list`, which is
  reserved for security incidents. If the owner wants them in the security feed, the wire value
  must be renamed with a prefix — an explicit decision, not a silent one.
- A consent is keyed by a **stable effect class from the domain** (e.g. the `ToolEffect` family,
  never a free-text tool string), scoped to `(space, member, effect class)`, and stores only:
  actor id, effect class, reason, source surface, `created_at`. No arguments or secrets; the
  existing `redact_audit_value` is a backstop, not permission to log inputs. Each grant emits
  one row, so `audit.export` can answer "who remembers what".
- Mirror the upstream downgrade behaviour as a safety property: when a tool grant / plugin install /
  permission changes (REQ-016/032/034), affected consents become *stale* and require
  re-confirmation rather than silently persisting — a warning in the UI, never a bypass.
- Revocation is explicit and audited, exposing a `consents.list` / `consents.revoke` pair next to
  `admin.grants.*`; ordinary members can manage only their own consents, owners can review all.

This is beyond the frozen spec, so AGENTS.md requires listing it in `docs/STATE.md` as a proposal
(and in `docs/PARITY.md` with owner approval if externally visible). It is a proposal, not a
shipped feature.

## 5. One REQ-named Playwright test

New file `apps/web/tests/approval-consent.spec.ts`, following the `seedRpc` pattern from
`apps/web/tests/fixtures.ts` (as `admin.spec.ts` and `approval-card.spec.ts` do):

```
test("REQ-058 a remembered approval decision is listed as a reviewable consent for the owner", async ({ page }) => { … })
```

Assertions: seed one consent plus one audit event; the owner sees a named region with the effect
class, actor and date; the row uses `text-text-secondary` for metadata and `border-separator-border`
for its hairline (token check, no palette literal); a stale consent shows a warning name, not an
active "on" state; a plain-member seed gets the refusal sentence. Test name carries `REQ-058` so
`tools/trace_check.py` counts it.

## 6. First slice (test-first, smallest complete loop)

1. Domain: add the two `AuditKind` variants + `as_str()` values; unit test
   `req_058_consent_kinds_have_stable_wire_values` (CONFIRMED naming rule from `trace_check.py`).
2. Store/usecase: `grant_consent` / `revoke_consent` append one typed audit row each via
   `AuditStore::append`; testkit test `req_058_a_remembered_consent_round_trips_with_one_audit_row`
   (mirrors `req_058_the_grants_create_list_and_revoke_round_trip_with_audit_rows`, `build.rs:7825`).
3. RPC: `consents.list|revoke` behind `require_space_role`/self-ownership; `rpc_surface.rs` test
   `req_058_an_owner_reviews_a_consent_a_plain_member_is_refused` (mirrors line 4043).
4. Web: the single Playwright test in §5, plus i18n keys in all 14 catalogues.
5. After the tests exist: add the `REQ-058` row to `docs/TRACEABILITY.md`, note the proposal and
   consulted upstream docs in `docs/STATE.md`, add an ADR for the expiry-on-change rule.
6. Gates unchanged in strictness: `cargo fmt --check`, `clippy -D warnings`, tests,
   `tools/trace_check.py`, Playwright, axe (zero critical/serious, light/dark, 360 px).

**Out of scope for this slice:** persisting consents client-side, any cross-team propagation,
remembering forms/sign-in/takeover prompts, and any Pro component.
