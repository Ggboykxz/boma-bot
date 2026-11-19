# Worker #275 — `crates/infra/src/computer.rs` — Architecture (v0.4.0)

Scope: structural proposal for the computer-use infra adapter (REQ-026–REQ-031). Evidence
is static inspection of `computer.rs`, `crates/app/src/ports.rs`, `crates/bin/src/computer.rs`,
`crates/infra/src/cdp.rs`, `spec/01-requirements.md` and ADR-009 this session (CONFIRMED).
No runtime test was run by this worker; no behaviour change is proposed that avoids
`docs/PARITY.md` where required. Reliability hardening (caps, timeouts, overflow, orphan
reclamation) is covered by worker #276's reliability lens and is not restated here.

## Current structure (CONFIRMED)

One 604-line file holds four distinct concerns: (1) `DockerComputerRuntime` config plus
builder methods (l.18-79); (2) its `ComputerRuntime` implementation — lifecycle `ensure`
(l.192-325), `exec` (l.327-389), `screenshot` (l.391-453), CDP tools (l.91-182); (3) a
private CDP-port resolver (l.480-514); (4) the full `NoneComputerRuntime` (l.531-604).
`spec_to_container` logic is inline in `ensure`, so no part of the provider is testable
without a Docker daemon. Provider selection and runtime assembly live in a different
crate (`crates/bin/src/computer.rs::ComputerConfig::provision`, l.94-126), which reaches
back into infra builder methods (`with_command`, `with_published_cdp_port`).

## Upgrade 1 — Split the file into a `computer/` module tree

Move to `crates/infra/src/computer/{mod.rs, docker.rs, none.rs, cdp_endpoint.rs,
exec_stream.rs, spec_builder.rs}`; keep the `pub use` surface (`DockerComputerRuntime`,
`NoneComputerRuntime`) identical so `boma_infra` consumers compile unchanged. Re-export
from `computer/mod.rs` with the same crate-level rustdoc that names REQ-027. This restores
the one-concern-per-file rule used elsewhere in `infra/src` and lets ADR-009's planned
cloud/desktop adapters land as new siblings instead of growing a sixth 600-line file.

Risk: pure refactor, low; module paths change for any in-crate `super::` references only.
Guard with `cargo fmt --check`, clippy `-D warnings`, and the existing REQ-tagged tests.

## Upgrade 2 — Extract the duplicated Docker exec/stream pipeline

`exec` (l.334-388) and `screenshot` (l.402-452) repeat `create_exec` → `start_exec` →
attached-stream loop, differing only in command and output cap. Introduce a private
`ExecStream` helper in `exec_stream.rs` returning `Result<Vec<u8>, AppError>` with an
explicit cap argument; both methods become thin callers that decode stdout. This makes
the "frame command prints bytes on stdout" contract (l.400-401) a single documented seam.

Risk: error mapping must stay byte-identical (`Unavailable` on prepare/run/stream failure);
a shared helper can accidentally merge the two distinct `tracing::warn!` contexts. Keep
the caller-supplied target string in the helper's log fields. No wire change expected.

## Upgrade 3 — Put CDP endpoint resolution behind a typed port

`cdp()` (l.482-514) mixes Docker inspection, published-port parsing and websocket open.
Split into a `CdpEndpoint` newtype (`host_port: u16`) plus a private
`resolve_cdp_endpoint(external_id) -> Result<CdpEndpoint, AppError>`, then open the socket
in one place. The CDP tools (`observe`/`navigate`/`evaluate`/`click`/`type_text`/`scroll`/
`dom_snapshot`) call one `with_cdp(external_id)` combinator, removing the repeated
`self.cdp(...).await?.method(...).await` chain. This is the natural extension point for a
non-Docker provider that already knows its endpoint, as ADR-009 requires.

Risk: keep the bounded `Cdp::connect` retry (cdp.rs l.66-69) and the `Unavailable`
mapping intact; a new type must not change the golden `computer-status` response.

## Upgrade 4 — Move runtime assembly into infra as one validated settings object

Today `bin` mutates a built runtime with two builder calls and splits a command string on
whitespace (l.103-109). Replace with a `DockerSettings` struct in `computer/mod.rs`
(`docker_host`, `image`, `command: Vec<String>`, `frame_command`, `cdp_port`) whose
constructor validates once and returns either settings or `AppError::Unavailable`;
`bin` passes the already-typed `ComputerConfig` fields to it. Invalid states (empty command,
CDP port without a browser command, frame command without an image) become unrepresentable
at the adapter boundary rather than silent defaults.

Risk: constructor signature change is an internal API break; coordinate with
`ComputerConfig::provision`. No externally visible behaviour change if validation only
rejects what startup already rejects — otherwise log a DEVIATION and keep the strict
default. Do not duplicate fail-closed checks from `bin::ComputerConfigError`.

## Upgrade 5 — Extract a pure `build_container_body(spec, settings) -> ContainerCreateBody`

Lift the `ContainerCreateBody` construction (l.223-274) out of the async `ensure` into a
pure function in `spec_builder.rs`. Add unit tests for labels, the per-computer home bind,
exposed/published port pairing (l.239-254) and limit conversion without a daemon. `ensure`
keeps only Docker I/O and the 409 race branch. This gives the provider conformance suite a
deterministic, fast half while preserving the real-daemon integration test in
`crates/infra/tests/computers.rs`.

Risk: the pure function must not hide the overflow issue worker #276 flags — accept
already-validated limits from `DockerSettings`, never re-derive them. If validation moves
here, the change is externally visible only for invalid configs (fail-closed), but record
the gate in `docs/PARITY.md` to be safe.

## Cross-cutting gates

Order: 1 → 2/3 (independent) → 4 → 5. Each step: `cargo fmt --check`, clippy with `-D
warnings`, and the REQ-026/027/028/030 test names unchanged in `docs/TRACEABILITY.md`.
Add no new REQ id and no feature beyond ADR-009; if a provider adapter is added, that is a
separate ADR. Mark claims CONFIRMED only after the checks above are run by the build agent.
