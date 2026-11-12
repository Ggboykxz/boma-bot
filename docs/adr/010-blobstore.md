# ADR-010 — Artifacts and pages: filesystem object store behind a trait, S3-compatible as an adapter

Status: proposed (awaiting owner go)

## Context

Artifacts are versioned binary blobs served through the API (REQ-036); pages are compiled documents
with immutable versions (REQ-037, REQ-039). `04-nonfunctional.md` fixes body caps and says content is
"stored out of band". The reference stores objects outside Postgres and returns base64 through the
procedure interface.

## Alternatives

1. **A `BlobStore` trait with a filesystem implementation (content-addressed by hash) and an
   S3-compatible adapter.**
2. Store blobs in Postgres (`bytea` or large objects).
3. Filesystem only.
4. S3 only.

## Scoring

| Criterion | 1 trait + fs | 2 in DB | 3 fs only | 4 S3 only |
|---|---|---|---|---|
| 1 fits the spec | 1 | 2 (bloats backups, row limits) | 2 | 3 (self-host needs a bucket) |
| 2 maturity | 1 | 1 | 1 | 1 |
| 3 security | 1 (path confined, no user-controlled paths) | 1 | 1 | 1 |
| 4 performance | 1 | 3 | 1 | 1 |
| 5 simplicity | 2 | 1 | 1 | 2 |
| 6 typed contract | 1 | 1 | 1 | 1 |

## Decision

Option 1. Keys are `space/<space_id>/artifact/<sha256>`; the hash is computed on upload and verified
on read. The trait has `put`, `get`, `delete`, `stat` with a streaming interface so a 25 MiB upload
never sits in memory. Deletion is a two-phase operation recorded in the database
(`HomeDeletionNotice`-style) so a failure to unlink is visible rather than silent.

## Consequences

- Default self-hosted deployment needs one directory; the README documents backup expectations
  (database plus object directory, both required for a consistent restore).
- Content addressing gives deduplication for free and makes "same bytes, same version chain" testable.
- The API never serves an artifact by path, only by id under a tenant check; MIME is sniffed on
  upload (fixing D-03) and the sniffed type — not the client's claim — is what the contract returns.
