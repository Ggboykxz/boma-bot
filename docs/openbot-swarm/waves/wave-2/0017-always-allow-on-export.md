# 0017 — always-allow-on × export (parity proposal, Boma Bot)

- Brief: id 17, area `always-allow-on`, lens `export` (`briefs-1000.jsonl`).
- Status: **PROPOSAL only** — no product code, spec, golden, token or catalogue was changed.
- Provenance: upstream read as **data** (HEAD `386085ff`, 2026-11-18) in
  `docs/architecture/{structure,computer-use,agents,servers}.md`. Paraphrased; no upstream code,
  asset, wording, identifier or brand is reproduced; no Pro component, no second design system.
  Claims are **CONFIRMED** when verified here by a read/command, else **GUESS**.

## 1. The upstream capability in my own words (data only)

CONFIRMED: in one desktop agent product a person can promote a one-off tool-consent into a
standing "allow this class from now on" choice. Generic properties:

- The standing choice is held by the **tool runner / provider**, scoped per server, per tool and
  per account, in a user-owned config layer. The desktop app saves no consent of its own; the
  team/server boundary carries only a one-shot accept/decline, so a remote host that answers its
  own prompts never sends a card back to a client.
- Turning the rule on also resolves approval cards already waiting for that agent; changing it
  discards a cached provider session that still holds the old policy. Forms, sign-in and screen
  takeover prompts stay interactive.
- On **export/transfer** permissive preferences do not travel as live permissions. An export that
  carried an always-allow flag gets it **downgraded to a warning** at import, so a safer default
  wins; only values that keep the target at or below a fresh agent's permissions are applied.
  Secrets are emptied (values, keys, URL user/password/query/fragment) with each change listed;
  the archive is written to a temporary file and renamed only when complete, bounded in size and
  file count and checked by a digest.

GUESS: the intent is friction reduction for repeated identical tool calls, with the privilege
widening kept local and re-confirmed on transfer rather than silently inherited.

## 2. Why export is the deciding lens for Boma Bot

CONFIRMED: we have an approval gate (REQ-007/017/059) and a real export stack but no standing
always-allow policy yet (0008 and 0009 propose one; 0009 records GUESS that no approval-policy RPC
exists). REQ-044 is a flat, manifest-versioned zip rendered on demand, served cookie-gated with
`cache-control: no-store`, bounded at 8 MiB per entry / 32 MiB per archive, whose importer uses a
closed section allow-list, refuses escaping paths and unknown versions, and names what it refuses
(`crates/app/src/account_export.rs`, `docs/adr/024-operate.md`). REQ-045 `audit.export` is a
version-1 JSONL — header line then one event per line, oldest first, 4 000-row cap, with
`redact_audit_value` on metadata and an owner/admin gate (`crates/app/src/usecases.rs::audit_export`).

The export lens refuses the upstream "save nothing / do not carry it" posture as sufficient: if a
standing allow is ever on, export must not silently re-grant it elsewhere and must leave a
reviewable trail of who enabled it, for what effect, when and from where. The convenience is fine;
the silent, untraceable widening is not.

## 3. Parity contract (design — GUESS, needs owner approval)

- **Never a permission in the archive.** A standing allow is a space/member/effect-class policy,
  not portable data. It stays out of `ExportBundle` and out of the closed `allowed_entry` list;
  the REQ-044 archive and its manifest carry no approval/permission field. A hand-crafted archive
  that smuggles one in is refused with the entry named, like any unknown entry — the importer's
  escalation surface stays closed.
- **The trail, not the flag, is what exports.** Enabling/disabling a standing allow appends one
  typed audit event each (new `AuditKind` variants beside `admin.*`, e.g.
  `approval.standing_enabled` / `approval.standing_disabled`, with stable `as_str()` wire values,
  never a free-text kind — the rule in `crates/domain/src/audit.rs`). Those rows appear as regular
  lines in REQ-045 JSONL for owner review; keep them non-`security.`-prefixed so they do not
  pollute `security.events.list`.
- **No secret leakage.** The event stores only actor id, effect class, reason and source surface —
  no arguments, tokens, environment, URLs or header values; `redact_audit_value` is a backstop,
  not permission to log inputs. The archive already holds no credentials (profile has no password,
  no secret section). Deployment-scoped rows survive deletion (`account.deleted`, `space_id = NULL`).
- **Downgrade on import, honestly attributed.** If a future import ever sees an elevated approval
  preference, apply it only at or below the target's default and report the rest as named
  warnings/refused — never as an active grant, never fabricating a member's consent (pairs with the
  0009 "no fabricated consent" guardrail).
- Beyond the frozen spec → list in `docs/STATE.md`; any new wire/audit value is a DEVIATION in
  `docs/PARITY.md` pending approval (`AGENTS.md`).

## 4. One REQ-named test (first-slice gate)

In `crates/testkit/src/build.rs`, one test named with the id so `tools/trace_check.py` counts it
(`docs/` is skipped):

```
req_044_the_export_carries_no_standing_permission_and_cites_its_redacted_audit_line
```

Seed a standing-allow audit event (typed kind; metadata carrying a secret-shaped `bearer` plus an
effect class), then assert:

- `audit_export` → header version `"1"`; one event line with actor, effect class, `createdAt` and
  `bearer: "[redacted]"` (mirrors `req_045_the_owner_exports_the_space_audit_and_secrets_are_reduced`).
- `account_export_download` → the zip holds only the known sections and no permission/approval
  entry; the manifest has no permission field.
- Feeding that zip back with an extra `approvals.json` → import refuses it with the entry named and
  writes no active permission.

This pins the lens: the permission never travels, the decision is auditable and redacted, and a
smuggled permission is refused by name.

## 5. First slice (reversible, test-first, small)

1. Write the failing test above; no implementation yet.
2. Add the two typed `AuditKind` variants + stable `as_str()` wire values; a unit test for stable
   wire values (CONFIRMED naming rule from `trace_check.py`).
3. Append-on-change via `AuditStore::append` (actor, target, reason, source surface; `space_id`
   set for the space), with no secret-shaped metadata; a plain member stays refused the export.
4. **Do not** add a policy write RPC, an archive section, a manifest version bump or an import of
   elevated preferences here — those change permission semantics and need their own REQ id, ADR
   and owner approval (0008/0009). Record them in `docs/STATE.md` as later slices.
5. After the test exists: add the REQ-044/045 rows to `docs/TRACEABILITY.md`; note the proposal and
   consulted upstream docs in `docs/STATE.md`; ADR for "permission never in the archive, trail only".
6. Gates unchanged: `cargo fmt --check`, `clippy -D warnings`, tests, `tools/trace_check.py`,
   Playwright, axe light/dark/360 px. No dependency change.

## Guardrails

- Inspiration only; no upstream code, asset, wording, identifier or brand; no Pro; one design
  system and semantic tokens for any future UI.
- CONFIRMED only for reads/commands run this session; this document ran no build or test.
- No secret, token or credential is stored in, or exported from, any new field.
