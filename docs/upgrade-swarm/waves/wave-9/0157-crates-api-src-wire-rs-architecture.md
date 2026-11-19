# 0157 — `crates/api/src/wire.rs` architecture proposal (v0.4.0)

Worker #157 · MODULE=`crates/api/src/wire.rs` · LENS=architecture · READ-ONLY analysis (no code changed).

Status of claims: every "current structure" statement is CONFIRMED by reads/greps of `wire.rs`,
`apps/web/lib/api/rpc.ts`, `crates/api/src/lib.rs`, `docs/adr/003-interface.md` and `AGENTS.md` in
this worktree. All upgrades are proposals (GUESS until implemented, tested and measured).

## Current structure (CONFIRMED)

- `wire.rs` is one flat 2302-line module with 157 `pub struct` declarations; views, replies and
  request bodies are peers in declaration order (feature-by-feature, not grouped by RPC namespace).
  `#[cfg(test)] mod tests` (`:1754-1877`) sits *in the middle* of the file; ~16 more structs follow
  it (`:1879-2302`).
- Each struct is a REQ contract statement: doc comments name REQ ids (32 distinct in the file), the
  header pins the two golden rules (always-present fields serialize even when empty; genuinely
  absent fields are omitted), and field names + serde attributes *are* the wire schema. `lib.rs:34-37`
  re-exports 8 of these types; `rpc.rs:28` imports a large `wire::{...}` list and calls the `From`
  conversions; `realtime.rs:21` borrows `EventView`. No handler owns a wire shape locally.
- The same contract is stated a second time, by hand, in `apps/web/lib/api/rpc.ts` (1684 lines,
  ~100 `interface` declarations; 21 distinct REQ ids). Its own header (`:1-2`) admits it is
  hand-written "until the OpenAPI procedure codegen lands (ADR-003)", yet ADR-003 (`:10-12, 46-48`)
  still mandates generated clients and forbids hand-written duplicates. Only 2 of ~157 structs
  (`LocalDiscoveredModelView`, `ModelsDiscoveredReply`) derive `utoipa::ToSchema`, so the generated
  OpenAPI document cannot currently be the canonical source for either side.
- Structural seams: `GroupMessageView` embeds `MessageView` via `#[serde(flatten)]` (`:2000`); four
  untyped `serde_json::Value` holes (`payload:97`, `skipped:846`, `source:1153`, `metadata:1487`)
  are opaque in both languages; the TS side compensates with runtime normalizers (e.g.
  `normalizeMessage`, `cardId` cleanup at `rpc.ts:342-361`). The REQ-id sets differ between Rust and
  TS (32 vs 21), so traceability is partial on both surfaces.

## Guiding principle for v0.4.0

One canonical wire schema (the generated OpenAPI document), one discoverable home per procedure's
Request/Reply pair in Rust, and a TypeScript mirror reduced to generated shapes plus thin
hand-written transport/behavior. Every field name, camelCase key and null-vs-absent rule stays
byte-identical; any externally visible delta is a DEVIATION in `docs/PARITY.md` awaiting approval.

## Upgrades (5 concrete items)

1. **Make the generated schema the single source of truth.** Derive `utoipa::ToSchema` on all view,
   reply and request structs (or generate the wire structs from `spec/openapi.yaml`), then generate
   the TS `interface`s in `rpc.ts` from that schema, fulfilling ADR-003. Keep `call<T>()`, `RpcError`,
   the cookie/`x-space-id` transport and the normalizers hand-written in a separate client-core file.
   *Risk:* naive generators flatten `Option<T>` to `T?`, destroying the `null`-vs-absent contract
   (e.g. `ThreadHeadReply.lastMessageAt: string | null` vs `SendReply.runIds?: string[]`).
   *Mitigation:* add explicit schema extensions for optionality, generate side-by-side first, diff
   every interface against the mirror, and switch imports only after golden + contract tests pass.

2. **Group the flat file into namespace modules behind a stable facade.** Split into
   `wire/{messages,computer,marketplace,admin,mail,models,...}.rs` plus `wire/mod.rs`; have `mod.rs`
   re-export every existing name so `lib.rs`, `rpc.rs` and `realtime.rs` keep identical paths. Move
   the mid-file test module out with it. *Risk:* a `pub use` churn or a changed path silently breaks
   a handler import; `#[serde(flatten)]` and shared base views must retain exact field order and
   attributes. *Mitigation:* facade-first — add re-exports before moving bodies, no handler edits,
   then gate on the full compile, golden replay, REQ-named test suite and OpenAPI contract test.

3. **Attach a per-procedure contract inventory to the wire layer.** Define one `wire::contracts`
   table mapping each `(namespace, procedure)` to its Request/Reply types and REQ id, consumed by the
   contract test and by `tools/trace_check.py`. *Risk:* 283 procedures of hand-maintained metadata
   becomes a second lying source that drifts immediately. *Mitigation:* derive the inventory from
   the typed `rpc.rs` handler signatures with a macro or build script, fail CI on any procedure with
   no mapped contract or an unmapped REQ id, and keep the table generated, never edited by hand.

4. **Delete the duplicated mirror once generation lands.** Reduce `rpc.ts` to generated types +
   client core; add a CI step that regenerates the types and diffs them (non-zero diff fails the
   build) so "keep the shapes in sync" becomes a mechanical gate. *Risk:* today's mirror contains
   hand-normalized quirks (`cardId`, permissive `Block` union) that codegen may not reproduce; each
   mismatch could be a real wire difference needing a DEVIATION entry. *Mitigation:* triage every
   diff with a golden/differential run, encode unavoidable client-side adaptations as generator
   extensions or normalizers in the client core, never by patching generated files.

5. **Give the four opaque blobs named, localized contracts.** Introduce `EventPayload`,
   `AuditMetadata`, `PageSource` and `PluginSkip` newtypes/schemas in the relevant namespace modules
   with documented REQ contracts, mirroring them as generated TS types. *Risk:* typed shapes can
   reject payload variants the original tolerated — an externally visible DEVIATION — and the
   redaction rule (REQ-045) must not be undone by reintroducing secret-shaped keys.
   *Mitigation:* start as transparent wrappers with zero validation, add validation only with golden
   evidence, and assert REQ-045 redaction on `AuditEventView.metadata` in a dedicated test.

## Cross-cutting risks / sequencing

- Sequence: 2 → 5 → 1 → 4 → 3. Group and name the shapes first, then make the schema canonical,
  then generate the TS mirror, then add mechanical traceability — each step is independently
  revertable and keeps the old names alive until its gate passes.
- No new dependency for items 2/3/5; item 1/4 needs a TS codegen tool (e.g. openapi-typescript) —
  ADR + licence check + `cargo-deny`/npm review before adoption.
- Gate every step with `cargo fmt --check`, `clippy -D warnings`, unit + integration tests, golden
  replay and the OpenAPI contract test; preserve REQ ids in test declaration names (comments do not
  count for `trace_check.py`).

## Out of scope for this module

Serde round-trip/default/unknown-field hardening and the `Eq`/`f64` anomaly are the reliability lens
(worker #158, `0158-...-wire-rs-reliability.md`); route/middleware composition is worker #149
(`router.rs`); per-procedure authorization lives in `auth.rs`/`rpc.rs`. This proposal only restructures
how wire types are organized, sourced and mirrored.
