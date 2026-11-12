-- M2: a bot and a run carry the model pinned for them, and the run records the choice it made.
--
-- Bot gains its own model preference, usable for a pinned choice the deployment may not reach;
-- Run carries what was resolved at the start of the run, so the activity list and the worker can
-- name the model without a second lookup.

ALTER TABLE bots ADD COLUMN model_provider text;
ALTER TABLE bots ADD COLUMN model_id text;
ALTER TABLE bots ADD COLUMN model_pinned boolean NOT NULL DEFAULT false;

ALTER TABLE runs ADD COLUMN model_provider text;
ALTER TABLE runs ADD COLUMN model_id text;
ALTER TABLE runs ADD COLUMN model_pinned boolean NOT NULL DEFAULT false;
