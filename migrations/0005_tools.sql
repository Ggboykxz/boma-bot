-- M2: the per-run tool catalogue (REQ-016).
--
-- A run receives the tools its bot may use, filtered by what is enabled for the space and for
-- the bot. The space's installs live here; the bot's own grant is a list of keys on `bots`.

CREATE TABLE tool_installs (
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    tool_key text NOT NULL,
    description text NOT NULL,
    kind text NOT NULL,
    enabled boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (space_id, tool_key)
);

-- The bot's grant: empty means the bot may use every enabled install of its space.
ALTER TABLE bots ADD COLUMN enabled_tools text[] NOT NULL DEFAULT '{}';

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['tool_installs'] LOOP
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
