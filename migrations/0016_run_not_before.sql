-- REQ-024: failures retry with backoff.
--
-- A run the system re-queued for a later attempt (the automatic retry after a failed routine
-- wake) may not be claimed before this instant. Interactive retries (REQ-012) and freshly
-- created runs leave it NULL: they are due as soon as they are queued.

ALTER TABLE runs ADD COLUMN not_before timestamptz NULL;
