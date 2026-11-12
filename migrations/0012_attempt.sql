-- REQ-012: how many attempts a run has consumed.
--
-- Every run starts at 1. A retry from `failed` re-queues the same run with one attempt added:
-- one conversation, one history of what was tried, no duplicate run rows.

ALTER TABLE runs ADD COLUMN attempt integer NOT NULL DEFAULT 1 CHECK (attempt >= 1);
