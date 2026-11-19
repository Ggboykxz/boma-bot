# Upgrade proposal v0.4.0 — `crates/domain/src/signin.rs` (architecture lens)

Worker: 261 | Lens: architecture | Module: `crates/domain/src/signin.rs`
Status: proposal only — no code changed | Language: English
Scope: sign-in request model structure inside `boma-domain` (pure rules: no IO, no async, no clock).
Contract: REQ-060 (`spec/01-requirements.md:500-510`). Wire strings `pending`/`granted`/
`refused` and public field names are compatibility surfaces — keep them.

## 1. Current structure (CONFIRMED — read this session)

- Two plain data structs and one fieldless enum, 62 lines total: `SignInRequest` (7-30,
  11 public fields), `SignInRequestStatus` (34-41: `Pending | Granted | Refused`), and
  `NewSignInRequest` (45-62, 9 public fields). All derive `Debug, Clone, PartialEq, Eq`.
- All fields are settable and there is **no constructor, no validator, no transition method and
  no test** in the module. REQ-060 appears nowhere in domain test names.
- Rustdoc claims "The ask is recorded before a member may answer it" (43), but that invariant
  is enforced in the *adapter*, not here: `testkit/src/memory.rs:8913` rejects a second answer
  with `AppError::Conflict`; the Postgres store has no such domain guard.
- The status↔wire mapping is duplicated by hand in `api/src/rpc.rs:4120-4122` and
  `infra/src/stores/signin.rs:40-45`, with an ad-hoc fallback (`AppError::Unavailable`) for
  unknown values. `ticket.rs:25-56` shows the intended pattern (`as_str`/`parse`/`ALL`).
- `SignInRequest.id` is `String` and `thread_id` is `Option<String>` although `ThreadId` and
  the `sin` id prefix already exist (`usecases.rs:9739`). Timestamps are raw `String`s from the
  app clock (`usecases.rs:9738`).
- `origin` / `display_name` are validated *only* in the app layer
  (`usecases.rs:9725`, `req_signin_url_must_parse` at 9791+ uses `reqwest::Url`); the domain
  accepts anything, including plaintext secret material in these fields.
- `signin` is the **only** domain module not re-exported from `lib.rs` (compare 45-111): all
  40+ call sites use the verbose `boma_domain::signin::...` path.
- A different type with the same name exists at `api/src/authroutes.rs:98` (the
  `POST /api/auth/sign-in` body). Two `SignInRequest`s in one codebase is a real readability hazard.
- REQ-060 also requires a bot-scoped *grant* governing secret reuse and a vault-sealed
  credential; the domain models only the request, no grant/reuse aggregate (GUESS: the grant
  may live in `computer.rs` / infra; no domain type was found by grep).

## 2. Target structure for v0.4.0

Keep the module pure and additive. Desired shape:

```text
signin.rs   SignInRequestId (opaque_id!) ; status enum with as_str/parse/ALL
            and transition methods returning DomainError ;
            SignInRequest::grant(by, at) / ::refuse(by, at) consuming Pending ;
            NewSignInRequest::new(...) -> Result validating origin + display_name ;
            SignInGrant (space, bot, origin, granted_by, granted_at) produced on grant
lib.rs      pub use signin::{...}  (keep the module path as a re-export alias)
wire/db     unchanged strings; only the mapping code shrinks
```

No new crate dependency in `boma-domain`; validation stays dependency-free.
## 3. Concrete upgrades (ranked: value / risk)

### U1 — Own the status vocabulary and the legal transitions (high value, low risk)
Port the `ticket.rs` pattern into `SignInRequestStatus`: derive `Copy`, add `as_str()`,
`parse()` and `ALL`, then add `SignInRequest::grant(&self, by, at)` and
`::refuse(&self, by, at)` that return `DomainError` unless `status == Pending`. Both adapters
and the memory store then call one domain guard instead of three copies. Preserve the exact
wire strings; make `parse()` reject unknown values with `DomainError::InvalidName`.
Risk: the memory store currently compares enum values directly and returns `Conflict`;
map the new domain error to `AppError::Conflict` at the boundary to keep the RPC status.
Effort: half a day plus REQ-named tests (`req_060_status_vocabulary_round_trips`,
`req_060_a_resolved_request_cannot_be_answered_twice`).

### U2 — Make the resolved state unrepresentable (high value, medium risk)
Today `granted_by: Some(_)` can coexist with `Pending`, and `Granted` can have `None` — the
type permits a grant with no granter. Change the enum to
`Pending | Granted { by: UserId, at: String } | Refused { by: UserId, at: String }` and
remove `granted_by` from the struct, or keep the field but add a checked constructor plus a
`debug_assert` invariant. Preferred: fold it into the variants so the compiler enforces it.
Risk: touches `api/src/rpc.rs` view mapping, `infra/src/stores/signin.rs:46-62` row mapping
and the DB row shape; a new DB column is a migration, not a refactor. Mitigation: keep the
existing `granted_by` column and map it into the variant (no migration), and add a
REQ-named test that the invariant holds for every `into_domain` path.

### U3 — Type the ids and validate the boundary fields (medium value, medium risk)
Add `SignInRequestId` via `opaque_id!`, change `thread_id` to `Option<ThreadId>`, and add
`validate_origin` / `validate_display_name` (non-empty, bounded, no control chars, origin
must be an absolute https URL with a host and no credentials/fragment — mirror the spec's
`example.com` and the app's current rule). Move `req_signin_url_must_parse` from
`app/src/usecases.rs` into the domain as a pure check; the app keeps only the `reqwest` call
if full URL grammar is required (GUESS: a hand-rolled check may diverge from `Url`).
Risk: ids are `serde(transparent)` so wire/DB strings stay unchanged, but `ThreadId::new`
rejects empty strings — check fixtures for `""` thread ids first. Stricter validation can
reject input the original accepts → DEVIATION in `docs/PARITY.md` if any fixture breaks.

### U4 — Model the grant, not only the ask (medium value, high risk — split wave)
REQ-060's reuse rule ("governed by a grant for that bot — the secret never renders as
plaintext") has no domain representation. Add an additive `SignInGrant` value object
`(space_id, bot_id, origin, granted_by, granted_at)` and
`SignInRequest::into_grant(self) -> Result<SignInGrant, DomainError>` that only succeeds on
`Granted`. This gives the vault/reuse path a typed key instead of three loose strings.
Risk: the reuse/vault flow is currently implemented elsewhere (GUESS: infra or the recording
browser path) and may already have its own key type; introducing a second one creates
duplication. Recommendation: grep and confirm the existing reuse key first; if one exists,
re-export it here instead of defining a new type. Owner decision needed before coding.

### U5 — Re-export and disambiguate the name (low value, low risk)
Add `pub use signin::{NewSignInRequest, SignInGrant, SignInRequest, SignInRequestId,
SignInRequestStatus};` to `lib.rs:74-77` (keep `mod signin` public for path stability), and
rename the *API* DTO at `api/src/authroutes.rs:98` to `AuthSignInRequest` (serde shape
unchanged; it is internal Rust, not a spec-published name — verify against
`spec/03-api-contract.md` before touching).
Risk: a mechanical rename of 40+ `boma_domain::signin::` paths is noisy but behaviour-free;
do it in a separate commit so a bad replace is easy to revert. If the API name turns out to
be part of a published contract, drop the rename and keep only the re-export.

## 4. Decisions needed / open questions

- D1 (owner): U2 folds `granted_by` into enum variants — acceptable internal API change?
- D2 (owner): is stricter origin/display_name validation (U3) an acceptable DEVIATION?
- D3: does a grant reuse key already exist outside this module (U4)? Needs a grep in infra.
## 5. Evidence and limits

CONFIRMED from files read this session: `signin.rs`, `ids.rs`, `lib.rs`, `ticket.rs` (full),
`infra/src/stores/signin.rs:1-70`, `app/src/usecases.rs:9700-9794`, `api/src/wire.rs:2110-2148`,
`api/src/authroutes.rs:90-109`, `api/src/rpc.rs:4114-4122`, `testkit/src/memory.rs:8851-8919`,
`spec/01-requirements.md:500-510`. GUESS items are labelled above. No commands, tests or
goldens were run — fixture impact of U2/U3 is unverified; the implementing wave must check
goldens before changing behavior.
