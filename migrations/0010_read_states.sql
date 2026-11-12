-- REQ-009: per-member read position, the substrate of the unread divider.
--
-- One row per (thread, person): the highest sequence this person has read in this
-- conversation. Absent rows mean zero: a member who never read anything is
-- unread from the start. The unread badge counts what sits above it.

CREATE TABLE thread_read_states (
    thread_id text NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    last_seq bigint NOT NULL DEFAULT 0 CHECK (last_seq >= 0),
    updated_at timestamptz NOT NULL,
    PRIMARY KEY (thread_id, user_id)
);

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['thread_read_states'] LOOP
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
