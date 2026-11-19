# v0.4.0 reliability proposal — `crates/app/src/metrics.rs` (counters under hostile input)

Worker #178 · LENS=reliability · target v0.4.0 · READ-ONLY analysis (no code changed).

Status of claims: "current state" statements are CONFIRMED by a direct read of
`crates/app/src/metrics.rs` and the harness rules in `AGENTS.md` in this worktree. Forward-looking
items are proposals (GUESS until implemented, tested and measured).

## Failure modes (CONFIRMED from the code)

1. **Cardinality blowup (high).** `add` (`:36-40`) inserts a new `BTreeMap` entry for every distinct
   key built by `key_of` (`:56-72`); there is no limit on series count, label count, key length or
   entry age, and no eviction. Any caller passing an unbounded value (user id, raw path, error text,
   timestamp) as a label grows the map monotonically for the life of the process -> heap exhaustion
   on a long-lived bot. The same unbounded map makes `render` (`:45-53`) O(n) slow and
   memory-doubling, amplifying the scrape problem below.
2. **Secret labels (high).** Label values are copied verbatim into the exposition (only `\`, `"`,
   and `\n` escaped, `:74-79`) and `render` *is* the body of `GET /metrics`. A token, email or prompt
   snippet used as a label is published to every scraper, permanently for the process lifetime.
   Label *names* are not escaped or validated at all (`:68`), so a name containing `\n`, `"` or `}`
   can inject or corrupt exposition lines (a fake series), breaking scrape correctness as well as
   confidentiality. Metric names are equally unvalidated.
3. **Scrape failures (medium).** `render` holds the only mutex for the whole build of one large
   `String`, stalling every `increment`/`add` on the request path for the duration of a scrape (no
   snapshot, no timeout). On a poisoned lock it returns an empty body (`:47-52`) with no signal, so
   Prometheus silently sees all series vanish; `add` also silently drops the increment (`:38`) —
   by design, but unobservable. `+= n` (`:39`) panics on overflow in debug (poisoning the mutex) and
   wraps in release, corrupting sums silently. `escape` allocates per label on the hot path, and
   no `# TYPE`/`# HELP` lines are emitted, so counters can be scraped as untyped.

## Guiding principle for v0.4.0

Keep the hand-rolled, dependency-free counter design and the exact promoted line format
(`<key> <value>` with sorted, escaped labels). Harden the input boundary and the scrape path; never
change a series name or value for well-behaved callers. Any wire-visible change is a DEVIATION to
record in `docs/PARITY.md` before shipping (AGENTS.md).

## Upgrades (5 concrete items)

1. **Cap cardinality at the boundary.** Add a `Metrics::with_limits { max_series, max_labels,
   max_label_bytes }` constructor (defaults preserve today's behavior) and have `add` drop — not
   allocate — a new series once a cap is hit, counting drops in an internal
   `boma_metrics_dropped_total`.
   *Risk:* silent drops hide real traffic; small defaults change promoted values (DEVIATION).
   *Mitigation:* expose the drop counter, log the first drop once at `warn`, keep limits opt-in in
   v0.4.0, and test that a cardinality flood leaves the map bounded.

2. **Validate and escape the whole exposition token.** Require `[a-zA-Z_][a-zA-Z0-9_]*` for metric
   and label names; escape `\r` alongside `\n`, `\\` and `"` in values; cap value byte length.
   *Risk:* a CR-bearing value or invalid name currently renders (malformed) and would now be
   dropped — a wire change for abusive callers. *Mitigation:* route rejections through the drop
   counter and add property tests that every rendered line parses as one series with a balanced
   label set.

3. **Redact secrets at the call boundary, not in the store.** Reject or map to a constant
   `"[redacted]"` any label whose name matches a configurable denylist (`token`, `secret`,
   `authorization`, `email`, `key`), and document the rule for every `increment` caller.
   *Risk:* a name heuristic misses secrets in benign-sounding labels and can redact harmless data;
   redaction changes existing series (DEVIATION). *Mitigation:* make the list opt-in via
   `with_limits`, default off in v0.4.0, and add a test that no rendered line contains a known token
   or email fixture.

4. **Make scrapes cheap and truthful.** Clone the map under a short lock, then render outside it so
   increments never block on a slow scrape; recover poison explicitly with `into_inner()` (or emit a
   stable sentinel) instead of an empty body; use `saturating_add` for `+= n`; emit
   `# TYPE <name> counter` lines and a Content-Type hint at the HTTP layer.
   *Risk:* cloning doubles peak memory on a large map and the snapshot can lag one scrape; poison
   recovery and `# TYPE` lines change response bytes (DEVIATION). *Mitigation:* cap series first
   (item 1), pin the exact body in golden tests, and record the header/body delta in PARITY.

5. **Test the failure modes, not just the happy path.** Add REQ id tests:
   `req_054_metrics_cardinality_is_bounded`, `req_054_metrics_labels_cannot_inject_exposition`,
   `req_054_metrics_secret_labels_are_redacted`, plus a poisoned-lock test asserting a non-empty
   parseable body and an overflow test for `saturating_add`.
   *Risk:* tests prove only the chosen policy, not that callers avoid unbounded/secret labels;
   that needs an audit of every `increment` call site. *Mitigation:* enumerate all call sites, keep
   REQ ids in test names (`tools/trace_check.py` counts declarations, not comments), gate each step
   with `cargo fmt --check`, `clippy -D warnings` and the unit suite.

## Cross-cutting risks / sequencing

- Sequence 4 -> 2 -> 1 -> 3: snapshot/overflow and validation are correctness fixes with no policy
  choice; caps and redaction are policy and need a PARITY entry and operator buy-in before any
  default changes.
- Nothing here needs a new dependency; items 1-3 are boundary/entry-point changes only. No series
  name or value changes for well-behaved callers.
- This module is pure in-memory state. The auth gate, headers and routing of `GET /metrics` live in
  the api crate (worker #137) and are out of scope here.
