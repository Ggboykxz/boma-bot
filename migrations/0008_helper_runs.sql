-- M2: a run can delegate a scoped task to a helper run that reports its result back to
-- the parent (REQ-019).
--
-- The link is the parent run: the helper belongs to the same thread but its reply is
-- routed as a helper result, not as a chat message, so its internal steps never show up in
-- the conversation. The trigger marks it (`helper_result`); clients who know the trigger
-- can distinguish helper turns from chat turns, and the parent consumes the outcome.

ALTER TABLE runs ADD COLUMN parent_run_id text REFERENCES runs (id) ON DELETE SET NULL;

CREATE INDEX runs_parent_run_id_idx ON runs (parent_run_id) WHERE parent_run_id IS NOT NULL;
