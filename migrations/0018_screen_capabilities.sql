-- REQ-030: screen bytes flow only through a sealed capability that names bot, computer and
-- generation, valid only while that computer is current.
--
-- The capability is a bearer token; only its hash is stored. Resolving a capability answers
-- the space the computer belongs to and the screen generation the capability was minted against;
-- a computer whose generation moved (any real restart) invalidates every capability it issued.

CREATE TABLE screen_capabilities (
    token_hash bytea PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    computer_id text NOT NULL REFERENCES computers (id) ON DELETE CASCADE,
    screen_generation integer NOT NULL CHECK (screen_generation >= 0),
    created_at timestamptz NOT NULL
);

CREATE INDEX screen_capabilities_computer_idx ON screen_capabilities (computer_id);

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['screen_capabilities'] LOOP
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
