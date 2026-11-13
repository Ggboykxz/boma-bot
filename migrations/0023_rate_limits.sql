-- D-01: one shared counter across replicas for the auth endpoints.
--
-- A bucket per (key, bucket): the window is baked into `bucket` (seconds since the epoch,
-- aligned to the window length), the counter is a plain hit count. Two replicas updating the
-- same row agree on the limit, which the process memory of one of them never could.
--
-- The table grows one row per key per window; expecting a 5-minute window, a few rows per
-- active client per day. A lazy cleanup deletes buckets older than 24 windows on every hit,
-- so a quiet client's history is pruned whenever any key is hit again.
CREATE TABLE rate_limits (
    key text NOT NULL,
    bucket bigint NOT NULL,
    hits integer NOT NULL DEFAULT 1 CHECK (hits >= 1),
    PRIMARY KEY (key, bucket)
);
