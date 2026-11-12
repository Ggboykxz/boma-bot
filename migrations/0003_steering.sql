-- M2: the steering message travels with the run it steers (REQ-003).
--
-- A member who writes while a conversational run is active does not start a second run: the
-- message joins the attempt. The worker folds it in before answering, and the column says where
-- that pending material begins in the thread's sequence.
ALTER TABLE runs ADD COLUMN steering_from_seq bigint;
ALTER TABLE runs ADD COLUMN steering_consumed_seq bigint NOT NULL DEFAULT 0;
COMMENT ON COLUMN runs.steering_from_seq IS 'Seq of the first message still waiting to join this run''s attempt (REQ-003).';
COMMENT ON COLUMN runs.steering_consumed_seq IS 'Highest message seq a provider pass already answered; newer messages are pending steering (REQ-003).';
