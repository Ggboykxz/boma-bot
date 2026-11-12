-- REQ-009: snoozing a conversation until it stops being urgent.
--
-- One row per (thread, person) with the snooze deadline. The roster reads it as a filter: while
-- `snoozed_until` is in the future the chat leaves the regular list and appears on the snoozed
-- shelf; when it passes, the chat is back without any job to run. Clearing the snooze deletes
-- the row, so a stale snooze can never hide a chat forever by accident.

CREATE TABLE thread_snoozes (
    thread_id text NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    snoozed_until timestamptz NOT NULL,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (thread_id, user_id)
);

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['thread_snoozes'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', tenant_table);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', tenant_table);
        EXECUTE format(
            'CREATE POLICY %I ON %I USING (space_id = boma.current_space_id() OR boma.is_system()) '
            'WITH CHECK (space_id = boma.current_space_id() OR boma.is_system())',
            tenant_table || '_tenant_isolation',
            tenant_table
        );
    END LOOP;
END
$$;
