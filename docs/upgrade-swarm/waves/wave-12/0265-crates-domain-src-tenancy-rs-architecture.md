# Upgrade proposal v0.4.0 — `crates/domain/src/tenancy.rs` (architecture lens)

Worker: 265 | Lens: architecture | Module: `crates/domain/src/tenancy.rs`
Status: proposal only — no code changed | Language: English
Scope: the tenancy domain model (`SystemReason`, `TenantContext`) in `boma-domain`, plus the
local-owner tenant (REQ-066) that this model currently cannot name.

## 1. Current structure (CONFIRMED — read this session)

- `SystemReason` (tenancy.rs:18-36): closed enum, 8 variants — Boot, Authentication,
  WebhookDelivery, ScheduledJob, DataMigration, SharedPageRead, ScimDirectorySync, AccountExport.
- Its closed vocabulary is written **four times**: the enum variants, `ALL: [Self; 8]` (40-49),
  `as_str()` (53-64), `parse()` (71-83). Adding a system path means editing all four; forgetting
  one silently drops it from `ALL` or from the parser.
- `TenantContext` (87-103): `Member { space_id: SpaceId, user_id: UserId }` or
  `System { reason: SystemReason, user_id: Option<UserId> }`, with accessors `space_id()`,
  `user_id()`, `is_system()`, `system_reason()` (105-138). Re-exported at `lib.rs:106`.
- **No local-owner concept exists in the domain.** REQ-066 lives in app/infra:
  `ensure_local_owner()` (`app/src/usecases.rs:140-204`) creates an ordinary account + default
  space and returns a `User`; `api/src/auth.rs:80-81` maps a sessionless loopback request to that
  user, which then becomes a plain `TenantContext::Member`. Once built, a context cannot say
  "this is the machine's implicit owner".
- Isolation is enforced *downstream*: `infra/src/tenant.rs:59-62` maps Member -> `in_space`,
  System -> `as_system`, setting RLS session values; `infra/tests/policy.rs` statically scans
  every `SystemReason::` use. The closed enum is the safety property — keep it closed.
- Tests (140-206) cover only REQ-043 and D-09; **REQ-066 has no domain-level test**.
- Risk scope measured: ~100+ `TenantContext::Member { .. }` patterns/literals across app, infra,
  api and testkit (grep this session). Many stores repeat the exact arm
  `Member { space_id, .. } => space_id.as_str().to_owned()` (connectors.rs:78, models.rs:79,
  mcp.rs:74, billing.rs:87/157, skills.rs:94, tickets.rs:92, plugin_installs.rs:71, ...).

## 2. Target structure for v0.4.0

Keep the closed enum and the three-layer defence (domain enum + static scan + RLS). Give the
vocabulary one source of truth, make the local owner an explicit principal, and centralize the
repeated member-only branching. No IO, async or DB dependency enters the domain crate.

**Contract to preserve:** `SystemReason::as_str()` wire values feed the audit log and the RLS
setting; `TenantContext`'s public paths must keep compiling for downstream crates. Any change to
accepted wire values is a DEVIATION (docs/PARITY.md) needing owner approval.

## 3. Concrete upgrades (ranked: value / risk)

### U1 — Single-source the `SystemReason` vocabulary (high value, low-medium risk)
Generate `ALL`, `as_str()` and `parse()` from one macro-declared list of `(variant, "wire")` pairs
(a local `macro_rules!`, no new dependency), keeping the public names and wire strings identical.
Add `FromStr`/`Display` mirroring ids.rs and a proptest asserting `parse(as_str(x)) == x` for all
`ALL`. Risk: it touches a public API that the `infra/tests/policy.rs` source scan and the audit
log rely on. Mitigation: freeze wire strings, add an REQ-043-named round-trip test first, and run
the policy test; zero wire change means no DEVIATION.

### U2 — Name the local-owner principal in the domain (high value, HIGH risk — stage it)
The local owner is still a Member in one space, so add a `PrincipalKind { Member, LocalOwner }`
marker on `TenantContext::Member` plus `is_local_owner()` and `member()` accessors — **not** a new
enum variant (a LocalOwner *is* scoped to a space, so a variant would wrongly imply cross-tenant
semantics). Add REQ-066-named domain tests; today none exist. Risk: changing the `Member` fields
breaks every struct literal and pattern above (~100 sites); `#[non_exhaustive]` would force
wildcard arms and destroy the exhaustiveness we rely on. Mitigation: purely mechanical,
compile-driven change; keep `space_id()`, `user_id()`, `is_system()` signatures unchanged so
read-only callers need no edit. GUESS (needs owner call): whether the distinction must exist in
the domain at all, or whether app-level `ensure_local_owner` is sufficient — default: add it.

### U3 — Typed scoped-member accessors, remove store duplication (medium value, low risk)
Add `member(&self) -> Option<(&SpaceId, &UserId)>` and `require_space(&self)` to replace the
~dozens of repeated `Member { space_id, .. } => ... ; System { .. } => Err(NotFound)` arms in the
stores named above. Risk: low and additive, but `require_space` must not smuggle app-level
`NotFound` semantics into the domain — return `Option`/a typed `DomainError::NoTenantSpace` and
let the app choose the HTTP response. Mitigation: migrate one store first, run REQ-043 and the
cross-tenant-not-found golden, then roll out.

### U4 — Constructors as the only invariant gate (medium value, high churn — defer)
Fields of a pub enum variant cannot be private, so callers can forge arbitrary
`System { reason, user_id }` combinations. Add `TenantContext::member(..)`, `::system(reason)`
and `::system_for(reason, user_id)` with `#[must_use]`, and (later wave) hide the enum behind a
struct with private fields. Risk: hiding the enum is the ~100-site break from U2 again; recommend
adding constructors now and deferring field privacy. Risk of leaving it: a future caller can pair
a system reason with an unrelated user id; the static scan does not catch that.

## 4. Decisions needed / open questions

- D1: Must the local owner be a distinct domain principal (U2), or is the current app-level model
  acceptable? Owner call; recorded GUESS until answered.
- D2: Is the ~100-site compile-driven churn for U2/U4 acceptable in v0.4.0, or deferred?
- D3: Confirm `as_str()` wire values are frozen (U1) so no DEVIATION is triggered.

## 5. Evidence and limits

CONFIRMED from files read this session: tenancy.rs (full), ids.rs, error.rs, lib.rs:106,
`app/src/usecases.rs:140-204`, `api/src/auth.rs:58-81`, `infra/src/tenant.rs` (full),
`infra/tests/policy.rs:4-54`, spec/01-requirements.md (REQ-037/042/043/044/045/066) and grep
counts for `TenantContext::Member`. GUESS items are labelled inline. No commands or tests were run
against the spec/goldens; the golden impact of U1/U3 is unverified and the implementing wave must
check it before changing behavior.
