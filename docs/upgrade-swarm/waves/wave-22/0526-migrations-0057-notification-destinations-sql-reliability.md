# 0526 — `migrations/0057_notification_destinations.sql` — reliability (v0.4.0)

Worker #526 · module `migrations/0057_notification_destinations.sql` · lens: reliability ·
READ-ONLY review of `rebuild-workspace/.worktrees/before-m5`. Proposals only; no code changed.
English. **Additive only** — every proposal is a new `006x` migration statement, test, comment or
observability change; none edits 0057 or any applied migration in place.
**CONFIRMED** = read in the tree this session; **GUESS** = inferred, needs a test.

## Ground truth (CONFIRMED by direct reads)

- 0057 is one additive `CREATE TABLE notification_destinations` + index `(space_id, kind)`,
  RLS `ENABLE`+`FORCE`, and one `FOR ALL` policy
  `space_id = boma.current_space_id() OR boma.is_system()` (USING = WITH CHECK).
- `ciphertext`/`nonce` are unchecked `bytea`; label has only a `btrim <> ''` CHECK; `created_by`
  has no FK; no `updated_at`. No `COMMENT` anywhere.
- `SecretBox` emits a 12-byte nonce + 16-byte GCM tag (`secret_box.rs:92-100`); the store seals
  under the **space DEK** (`space_dek`, `destinations.rs:81,226`), not the deployment master key.
- Registration validation is **string-only** (`usecases.rs:9863-9880`, `destinations.rs:86-102`):
  HTTPS prefix, no `@`, Slack host prefix. **No `net_safety::check_outbound_url` call.**
- Delivery **does** call the guard: `push.rs:236` → `messaging_transport.post_json(url,&[],&body,
  false)` → `messaging_http.rs:95` `check_outbound_url(url, allow_private=false)` (DNS, private/
  loopback/metadata ranges), with `redirect::Policy::none()`. Refused/unreachable = `warn` log,
  no cap consumed (`push.rs:239-251`).
- Helpers (`0002_m1.sql`): `current_space_id()` = `boma.space_id` GUC (NULL when unset);
  `is_system()` = non-empty `boma.system_reason` GUC. `sqlx::migrate!` (`db.rs:20`) is forward-only,
  one transaction/file, checksummed — editing 0057 in place is a deploy-wide outage.
- REQ-065/ADR-028: space-scoped sealed webhooks, title-only content, Owner/Admin management,
  best-effort delivery, shared UTC-day cap.

## Already right (keep, do not re-propose)

- RLS `ENABLE`+`FORCE` with matching USING/WITH CHECK; `ON DELETE CASCADE` from `spaces`.
- `list()` selects ciphertext but `row_into_domain` drops it; the only open path is
  `destination_webhook()`, returning `SecretString`. No read path returns the URL (CONFIRMED).
- Prefix checks reject obvious non-Slack hosts; delivery uses no-redirect and the shared guard.

## Failure modes

### Secret storage
- **S1 (high).** No shape CHECK on `ciphertext`/`nonce`: a truncated blob from a bad restore,
  partial write or future bug makes `open_with_data_key` fail at delivery, logged once per send
  and otherwise invisible (CONFIRMED absence; `push.rs:212-215`).
- **S2 (high).** Ciphertext is bound to the space DEK with no stored key/algorithm version. A
  space rekey (wave-15 U2 risk) or future AEAD change silently makes every row undecryptable;
  nothing marks rows needing re-wrap (GUESS on rekey mechanics, CONFIRMED no version column).
- **S3 (medium).** No `updated_at`: rotation is delete+create, leaving a window with no
  destination; failures/rotation leave no audit stamp (CONFIRMED).

### SSRF
- **S4 (high).** The URL is sealed, so SQL cannot validate it; registration checks strings only
  and skip `check_outbound_url`. A `https://169.254.169.254/…`, `https://127.0.0.1:443/…`, or a
  hostname resolving private is **accepted and stored**. Delivery then refuses it forever:
  Owner/Admin sees a "configured" channel that never delivers, with a generic warn (CONFIRMED).
- **S5 (medium).** Any non-app insert path (system backfill, restore, manual SQL, future
  migration) bypasses validation entirely; the schema has no representation of "validated"
  (CONFIRMED). GUESS: DNS-rebinding TOCTOU if reqwest re-resolves after `CheckedOutbound` —
  security-lens territory, flagged only.

### RLS
- **S6 (high).** Unset `boma.space_id` makes USING false: `list()`/`destination_webhook()` see
  zero rows and push silently drops; writes fail loudly. "No destinations" vs "no context" is
  indistinguishable (CONFIRMED; GUESS on worker GUC setup).
- **S7 (high).** `is_system()` is a non-empty GUC: any holder gets cross-tenant read of sealed
  ciphertext; `FORCE RLS` means a rekey/backup job without `system_reason` sees nothing/reports
  success on zero rows (CONFIRMED SQL; GUESS on job identity).
- **S8 (medium).** Owner/Admin-only is enforced **app-side** (`require_space_role`,
  `usecases.rs:9882`), not in RLS: a new read path that forgets the role check exposes sealed
  webhooks to any member. The single hand-copied policy can drift from 0056/0058 (CONFIRMED).

## Concrete hardening (additive; new `006x` file + tests; REQ-065 in every test name)

- **P1.** `COMMENT ON TABLE/COLUMN` binding REQ-065/ADR-028, AEAD-256-GCM under the space DEK,
  "URL never returned by any read path", and that role control is app-layer → S2/S8 documentation.
- **P2.** Seal-shape `CHECK (octet_length(nonce)=12 AND octet_length(ciphertext)>=16) NOT VALID`
  + `VALIDATE` after a read-only audit; no upper bound (keeps future payloads valid) → S1.
- **P3.** Nullable metadata `seal_version smallint`, `dek_version smallint`, `validated_at
  timestamptz`, `validation_rule text` written by `destinations.create()`; a read-only audit query
  lists rows with NULL/old versions for re-validation/re-wrap. **Do not store the host/URL**
  (disclosure, esp. Teams targets) → S2, S4, S5.
- **P4.** `updated_at timestamptz NOT NULL DEFAULT now()` as the in-place rotation seam, removing
  the delete+create gap → S3.
- **P5.** Observability (app, no wire change): distinct metric + structured warn when delivery
  refuses a stored webhook via `check_outbound_url`, so "unsafe/misconfigured URL" is not
  confused with "channel down"; alert on repeated refusals → S4.
- **P6.** `boma.require_space_context()` / `boma.as_system(reason)` guards at worker and
  backfill/rekey entry; CI assertion that the 0056/0057/0058 tenant predicates stay byte-identical;
  runbook: rekey/backfill must run in system context → S6, S7, S8.
- **P7.** REQ-065 tests: shape CHECK rejects a bad blob; two-tenant RLS on read + open; unset
  GUC fails closed; non-Owner cannot create/delete/read webhooks; a private/metadata URL stored
  by a backfill is refused at delivery with the P5 counter; fresh migrate → metadata NULL.

## Risks / trade-offs

- R1. P2 `VALIDATE` fails if any legacy row has a non-12-byte nonce/short blob — audit first;
  keep checks generous (CONFIRMED constraint, GUESS on legacy data).
- R2. P3/P4 columns are inert until the store writes them; ship the app change in the same release
  or they mislead. NULL legacy rows are expected and must be handled by the audit, not backfilled.
- R3. P3 leaks only "validation happened", never the URL/host; acceptable. A `dek_version` needs
  `space_keys` cooperation — cross-module, GUESS.
- R4. P5/P6 are additive app/ops changes; no API shape change → no DEVIATION, but a contract test
  must confirm a fresh migrate is unchanged on the wire.
- R5. `net_safety` resolves per send; tiny volume, but a hostile hostname can add DNS latency.
- R6. No in-place edit, no policy replacement, no kind-set or sealed-format change, no backfill.

## Suggested rollout for v0.4.0

Wave A (invisible): P1, P7, P5/P6 observability, P2 after audit. Wave B (with store/app changes):
P3, P4. All work in new `006x` files; verify with `cargo fmt --check`, `clippy -D warnings`, the
`req_065_*` infra tests and a migration up/down on a seeded DB. Nothing executed; all source
claims come from files read this session.
