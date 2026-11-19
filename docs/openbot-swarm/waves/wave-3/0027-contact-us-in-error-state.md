# 0027 — Contact us (inbound) × error state (parity proposal, Boma Bot)

- Brief: id 27, area `contact-us-in`, lens `error-state` (`docs/openbot-swarm/briefs-1000.jsonl`).
- Status: **PROPOSAL only.** Upstream was read as data (brief inventory, HEAD `386085ff`,
  2026-11-18); no upstream code, asset, wording, brand or Pro component enters Boma Bot. Product name
  and strings stay Boma Bot. Claims are **CONFIRMED** (read in `./spec`, `AGENTS.md`, tests this
  session) or **GUESS** (needs the owner's decision).

## 1. Upstream capability, in my own words (inspiration only)

The area named by this brief is an **inbound "Contact us" channel**: a person inside the product sends
a question, feedback or support request to the team through a short form. The pattern worth carrying:

- A compact form (topic/category, contact e-mail, message) validates fields **before** any request is
  made; a malformed entry is rejected with the reason and nothing is stored (validation-first).
- A valid submission is **persisted and routed** to a team queue and acknowledged with a confirmation
  or reference, so the sender can tell it landed.
- Repeated or double submissions are **idempotent** — one submission, one reference — rather than
  creating duplicate tickets.
- Failure is **honest and recoverable**: a rate limit, auth loss or backend outage shows an inline,
  user-legible error with a next step; the draft text survives and can be retried. Internal detail
  never reaches the screen.

Specific upstream field names, copy and routing are a **GUESS** — the brief area name is the signal;
none of it is copied. We answer with our own design below.

## 2. What it maps to in Boma Bot (GUESS — propose, do not assume)

There is **no contact screen** in the route map (`spec/08-ui-ux.md` §8.1 — CONFIRMED): "anything not
listed is not a screen." The nearest existing pieces are REQ-001 (send a message; `clientNonce`
replay returns the original result — CONFIRMED), REQ-034 (malformed source fails with a reason,
nothing installed — CONFIRMED), REQ-038 (feature-flagged ticket board) and REQ-058 (admin review of
submissions). A contact form is therefore a **new public surface** — a decision for the owner; write
the gap in `docs/STATE.md`. Simplest safe default: one thin "Contact us" route with a new
`support/submit` RPC that validates input, applies the same nonce-based idempotency as REQ-001,
returns a reference on success, and never performs a side effect on a validation failure. Do not
reuse the chat composer or fabricate a ticket in the board (that would mix unrelated jobs).

## 3. Error/recovery parity, grounded in our error envelope

The stable envelope is `{ error, code }` with codes `UNAUTHORIZED | FORBIDDEN | NOT_FOUND |
BAD_REQUEST | CONFLICT | TOO_MANY_REQUESTS | INTERNAL`; messages are user-safe, internal detail never
crosses, and tenant violations are `NOT_FOUND` (`spec/03-api-contract.md` §3.1, `openapi.yaml` —
CONFIRMED). Screen copy follows §8.9: one sentence of what happened, one of what to do, no stack/ids/
provider text, correlation id behind "Details", status/chip text plus icon never colour alone, errors
announced through the polite live region. Field errors are **inline above the field, no toast**
(`/sign-in` precedent, §8.2 — CONFIRMED).

| Failure condition | Code | Where it surfaces | Recovery |
|---|---|---|---|
| Empty/invalid e-mail, over-long or blank message | `BAD_REQUEST` | Inline, above the offending field | "Enter a valid e-mail." / "Add a message."; keep every field value, persist nothing (REQ-034) |
| Repeated submits / rate limit | `TOO_MANY_REQUESTS` | Inline error + retryable status chip | "Too many requests." + "Wait a moment and try again."; retry reuses the same nonce — one reference (REQ-021, REQ-001) |
| Session expired / form requires sign-in | `UNAUTHORIZED` | Inline error in the form | "Sign in to continue."; preserve the draft across the sign-in redirect; no silent submit |
| Queue disabled or caller's space not found | `NOT_FOUND` (other tenant uniform) | Inline error | "That contact queue isn't available."; no id, no existence leak |
| Double click / replay while first submit in flight | `CONFLICT` or nonce replay | Confirmation area | "Already sent." + show the original reference; never create a second submission |
| Backend/unavailable | `INTERNAL` (generic text) | Inline error + polite live region | "We couldn't send that." + "Try again or write to us later."; correlation id behind "Details"; draft intact |
| Attachment too large / unsupported type | `BAD_REQUEST` | Inline on the attachment row | Remove that file, keep the message text and resubmit |

Approvals/effect rules are untouched: a contact submission creates no tool effect, so REQ-017's
exactly-once key is not needed; idempotency is the nonce reference only.

## 4. UI surface and BoardUI tokens (CONFIRMED names from AGENTS.md / components tree)

- A single "Contact us" route reachable from the shell; build the form on **installed BoardUI form
  components** under `components/base/` (they extend `react-aria-components` — no raw `<input>`).
  No new design system, no Pro, no second library.
- Error text: `text-text-error-primary`, `text-body-regular`, `text-caption-1-semibold`; border
  `border-border-error-default`; surface `bg-background-secondary-default`; page ground
  `bg-background-full`; hairline `border-separator-border`; focus
  `outline-none focus-visible:ring-2 focus-visible:ring-border-focus-ring`. Cards `rounded-3xl`,
  inputs `rounded-md`–`rounded-xl`. Merge with `cx()`; icons from `@remixicon/react` as component
  references; retry chip uses text + icon. Dark mode rides the tokens — no `dark:` literals, no raw
  palette classes. RTL uses logical `ms`/`me`, `text-start`; copy from the catalogue (REQ-057).
- Motion 120–180 ms, respect `prefers-reduced-motion`; the busy/disabled submit state is named, never
  a spinner that outlives its work (§8.1).

## 5. One Playwright test (test-first; REQ id in the name)

Add to a new `apps/web/tests/contact-us.spec.ts`, using `seedRpc` from `apps/web/tests/fixtures.ts`
(CONFIRMED files):

```ts
test("REQ-021 a failed contact-us submission shows a legible retryable error and keeps the draft",
  async ({ page }) => {
    // seed support/submit -> { code:"TOO_MANY_REQUESTS", error:"..." } via seedRpc
    // assert: inline error carries one cause + one action; retry chip is text+icon (not colour alone);
    //         "Details" hides the correlation id; no toast; all field values still present;
    //         the polite live region announces the error; keyboard reaches Retry and one resend
    //         with the same nonce yields a single reference; axe: no critical/serious finding.
  });
```

The name carries `REQ-021` (Failures are user-legible) so `tools/trace_check.py` counts it; it asserts
state, not a golden click path. A `REQ-034` validation (BAD_REQUEST, nothing persisted) variant can
be added in a later slice.

## 6. First slice

1. Write that test first (red). Replay `spec/golden/unauthorized.json`,
   `cross-tenant-not-found.json` and `plugin-submission-invalid.json` for the envelope/validation
   contract (CONFIRMED goldens).
2. Backend: typed `thiserror` variants mapped to the stable envelope; `support/submit` validates the
   schema (BAD_REQUEST), applies `clientNonce` idempotency (REQ-001), rate-limits (TOO_MANY_REQUESTS),
   returns a reference, and emits only generic INTERNAL text — no internal detail.
3. Frontend: the Contact us form with token-based inline errors, live-region announcement, "Details"
   disclosure, keyboard path and 360 px / dark / reduced-motion checks; no Pro, no new library.
4. Done when the test is green, goldens replay, `cargo fmt --check`, `clippy -D warnings` and Playwright
   pass; record the "new route + REQ gap" decision in `docs/STATE.md`.
