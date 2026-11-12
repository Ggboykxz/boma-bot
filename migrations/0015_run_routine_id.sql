-- REQ-024: attribute a run to the routine that woke it.
--
-- Nullable: only routine-woken runs carry it. ON DELETE SET NULL keeps old history readable
-- when a routine is removed; the trigger type is the record that it was a routine.

ALTER TABLE runs ADD COLUMN routine_id text REFERENCES routines (id) ON DELETE SET NULL;
