# Brief 0048 — continue-in-a through the audit lens

Proposal only; no product code was changed. **CONFIRMED** = verified by a read/command here; **GUESS** = a
recommendation for the owner. Upstream was read as data at `/tmp/opencode/openbot-parity` (HEAD `559d5ac`,
2026-11-19). No upstream code, asset, wording, identifier or brand is reproduced; no Pro component is used.
Product strings stay Boma Bot. Brief: id 48, area `continue-in-a`, lens `audit` (`docs/openbot-swarm/briefs-1000.jsonl`).

## 1. The upstream capability in my own words (data only)

CONFIRMED: when a provider loses the session behind a turn (a missing/invalid-session condition, classified
per provider), the upstream product does not fail the turn. It retires the stale session, starts a replacement
seeded from the saved conversation, and the person's message still gets its answer. The same machinery covers a
configuration/tool change mid-thread; a context-overflow failure is classified separately and deliberately does
**not** ride the same recovery. Verified by reading the agent lifecycle and its session classification in the clone.

The recovery is almost invisible to an operator. It leaves a warn-level line naming the agent, the provider and
an outcome tag (resumed vs replaced); a failed read of an earlier-step handoff file is logged separately; an
unwritable close is logged separately. There is **no** typed event, **no** counter, **no** operator-facing review
surface, **no** redaction contract, **no** space scoping and **nothing** in any export. GUESS (provenance): the
area slug cannot be re-verified from the clone; the tree's only "continue in a …" occurrence sits mid-entry in a
release note about continuing in a new provider session, so this recovery is treated as the brief's capability.

The property worth naming: a transparent continuity recovery that is silent to the person and nearly silent to
the operator — impossible to count, scope or review after the fact.

## 2. Why audit is the deciding lens

CONFIRMED: Boma Bot has no persistent provider session to lose. Each pass re-packs the thread history (REQ-069),
and external executors open a fresh ACP session per run (`crates/app/src/acp_provider.rs`). The real degraded
continue paths are: (a) history compaction at three quarters of the window — today it writes only a `System`
note into the thread; (b) a retryable routine wake re-queued with backoff (REQ-024) — no audit row; (c) an ACP
session spawn/handshake failure — the run fails legibly (REQ-021). None of these leaves a typed, reviewable
fact. Silence about a transparent recovery is exactly what the audit lens exists to break (D-05: every escape
hatch is a typed `AuditKind`).

## 3. Grounding in our contract

- **REQ-058 (anchor).** CONFIRMED — `spec/01-requirements.md:473-479` (SHOULD): administration behind the role
  gate; a member without the admin role is refused; operating without SSH into the database. Server half already
  exists: `require_space_role`/`require_admin`; `admin_grants_create|revoke`, `admin_plugins_list|decide`,
  `admin_invitations_list|revoke` append typed rows (`admin.grant_created|grant_revoked|plugin_reviewed|…`) with an
  actor, target and JSON metadata (`crates/app/src/usecases.rs`, `crates/infra/src/stores/admin.rs`).
- **`docs/TRACEABILITY.md`.** CONFIRMED: a row exists only when a test with the REQ id in its **name** exists;
  `tools/trace_check.py` counts declarations and skips `docs/`, so prose earns nothing. The REQ-058 row (M9)
  already lists the admin tests; this slice extends that row's test cell **only after** the new test exists.
- **REQ-045 export path.** CONFIRMED: `audit_export` is owner/admin-gated JSONL v1, `redact_audit_value` walks
  nested values against `SECRET_TOKENS` (`sk-`, `ghp_`, `Bearer `, …), 4000-row cap, newest-window export.
  Redaction is a backstop, not permission to log inputs.
- **Typed kinds only.** CONFIRMED — `crates/domain/src/audit.rs`: the new variant joins the enum with a stable
  `as_str()` wire value and is **never** free text. Keep it **non** `security.`-prefixed so it lands in
  `audit.export`/admin lists, not the incident-only `security.events.list` (REQ-045).
- **No secret leakage.** Metadata is enums only; `target` is the run id; `actor_user_id: None` (system-originated,
  like `mail.dropped`); no prompt, history, addresses, provider/driver strings or arguments are written;
  `.env.example` stays the environment surface.
- **Honesty guard.** Boma Bot lacks the provider-session parity itself; the slice records the two existing
  degraded-continue facts (and leaves room for an executor-session path) — it does not claim a behaviour we do not have.

## 4. Proposed shape — a recovery fact that is auditable (GUESS, approve before code)

- One new `AuditKind` variant, wire value e.g. `run.continued_degraded`, beside the `admin.*` variants, non
  `security.`-prefixed.
- Appended by the worker exactly when a degraded continue happens: compaction-continue (`path=compaction`,
  `outcome=continued`), routine retry re-queue (`path=routine_retry`, `outcome=continued`); later, an executor
  session replacement (`path=executor_session`, `outcome=resumed|replaced`). Metadata: `{path, outcome}` enums
  only; `space_id`: the run's space; `target`: the run id; actor: `None`; best-effort append that never fails the run.
- **No new RPC or screen in slice 1:** the row rides the existing owner/admin-gated `audit.export` (REQ-045);
  counter `boma_runs_continued_total{path,outcome}` joins `boma_runs_finished_total`. A dedicated admin review
  list (behind `require_space_role`, mirroring `admin.grants.list`) is a later, owner-approved step.
- Beyond the frozen spec: `docs/STATE.md` entry now; a `docs/PARITY.md` DEVIATION and an ADR ("record the fact,
  never the text", the same rule as the support-contact lane) before anything externally visible; wire names,
  counter labels and the test module are GUESS. No new REQ id: the work attaches to REQ-058.

## 5. One REQ-named test

Testkit, `crates/testkit/src/build.rs::admin_tests`:

```rust
fn req_058_a_degraded_run_continuation_appends_one_redacted_audit_row() { /* … */ }
```

Assertions: force the compaction path (seed > ¾ of a small window) and the routine retry path; each yields
exactly one row with wire kind `run.continued_degraded`, `space_id` set, actor `None`, `target` = run id,
metadata exactly `{path, outcome}`; the seeded prompt carries a `sk-…`-shaped fragment and the row's JSON dump
contains neither it nor any prompt text; a plain member refused `audit_export` never sees the row, an owner does.
The name carries `REQ-058`, so `tools/trace_check.py` counts it.

## 6. First slice (test-first, smallest complete loop)

1. Domain: the variant + `as_str()`; unit test `req_058_a_continued_degraded_kind_has_a_stable_wire_value`.
2. App/worker: helper `record_run_continued(...)` (modelled on the existing audit appends, best-effort with warn
   on failure); call it from the compaction branch of `provider_pass` and from `auto_retry_routine_run`.
3. Metrics: `boma_runs_continued_total{path,outcome}` in `metrics.rs` style (closed label sets).
4. The testkit test in §5 — confirm it fails first, then green.
5. Only after tests exist: extend the REQ-058 row in `docs/TRACEABILITY.md`; note this proposal and upstream
   `559d5ac` in `docs/STATE.md`; open the ADR.
6. Gates unchanged: `cargo fmt --check`, `clippy -D warnings`, tests, `tools/trace_check.py`, Playwright/axe
   (no UI ships in this slice).

**Out of scope:** provider-session parity machinery, any new RPC/screen, `security.events.list` changes,
cross-tenant views, storing prompts or driver strings, any Pro component, copying upstream code or wording.

## 7. Evidence

CONFIRMED — upstream HEAD `559d5ac` (2026-11-19), read as data: the per-provider missing-session classifier, the
missing-session replacement branch in the thread lifecycle, the handoff-read failure logged separately, and the
agent-service warn hook (agent, provider, outcome resumed|replaced) with no audit row, counter or export.
Ours: `spec/01-requirements.md` REQ-058/REQ-045/REQ-024/REQ-069, REQ-021; `docs/TRACEABILITY.md` (header +
REQ-058 row); `crates/domain/src/audit.rs`; `crates/app/src/usecases.rs` (`audit_export`,
`auto_retry_routine_run`); `crates/app/src/worker.rs` (`provider_pass`, `fail_run`); `crates/app/src/acp_provider.rs`;
`crates/infra/src/stores/audit.rs`; `tools/trace_check.py`. GUESS — slug→capability mapping, wire value, counter
labels, the shared ADR.
