# ADR-006 — Background jobs: a Postgres-backed queue with leases and fences

Status: proposed (awaiting owner go)

## Context

Runs, routine wakeups, webhook-triggered turns, e-mail ingestion and export jobs all need durable
dispatch with retry, coalescing and recovery (REQ-013, REQ-014, REQ-024). Correctness requires exactly
one executor per run: a stale worker must not be able to write. `04-nonfunctional.md` also requires
that a rolling restart loses no in-flight work. The reference uses a Graphile worker plus an
in-process starter, with a legacy path kept for upgrades.

## Alternatives

1. **A queue table in Postgres (`jobs`) with `SELECT … FOR UPDATE SKIP LOCKED`, a lease per job, a
   fence counter, and a reconciler.**
2. Redis-backed queue (a Streams or a list) with a visibility timeout.
3. A dedicated broker (RabbitMQ, NATS JetStream, SQS).
4. In-process `tokio::spawn` only, with a startup sweep.

## Scoring

| Criterion | 1 PG queue | 2 Redis | 3 broker | 4 in-process |
|---|---|---|---|---|
| 1 fits the spec | 1 | 2 (second datastore, durability story) | 3 | 5 (loses work on restart) |
| 2 maturity | 1 | 1 | 1 | 1 |
| 3 security | 1 (same credentials, same RLS) | 3 (second surface) | 3 | 1 |
| 4 performance | 1 (thousands/s is plenty here) | 1 | 1 | 1 |
| 5 simplicity | 1 (no extra service) | 3 | 5 | 1 |
| 6 typed contract | 1 | 1 | 2 | 1 |

## Decision

Option 1.

- `jobs(id, kind, payload jsonb, run_after, leased_by, lease_expires_at, fence, attempts,
  last_error, unique_key)` with a partial index on due, unleased rows.
- Claiming is `FOR UPDATE SKIP LOCKED` in a transaction that also writes the run's lease, so the
  queue and the run can never disagree.
- `unique_key` implements coalescing (REQ-024): N pending triggers for one thread collapse to one
  row, and a burst of routine fires keeps the earliest sensible slot.
- A reconciler sweeps expired leases back to `run_after = now()` and increments the run's recovery
  counter; it is safe to run in every replica.
- Workers are the same binary as the API with a different subcommand (`boma serve`, `boma worker`),
  so a self-hoster can run one process for both.

## Consequences

- No extra infrastructure: Postgres is the only dependency, which matches ADR-001's "only required
  datastore".
- Queue throughput is bounded by Postgres; the spec's workload is long-running agent turns, so this
  is not the bottleneck. If it ever is, a broker adapter behind the same port is a later ADR.
- Polling latency is tunable (`LISTEN/NOTIFY` wakes the worker, the poll is the fallback); the same
  listener that feeds realtime is reused, so no new moving part.
- The legacy Graphile compatibility path of the reference is deliberately not reproduced: one
  dispatch mechanism, same observable guarantees (`00-overview.md`, non-goals).
