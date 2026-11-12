-- REQ-008: emoji reactions on messages.
--
-- One row per (message, person, emoji): the count, and who reacted. The reactor is the sender;
-- the emoji is any short string (validated at the boundary).

CREATE TABLE message_reactions (
    thread_id text NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    message_id text NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    emoji text NOT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (message_id, user_id, emoji)
);

CREATE INDEX message_reactions_message_idx ON message_reactions (message_id);

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['message_reactions'] LOOP
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
