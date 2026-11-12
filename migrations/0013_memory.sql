-- REQ-022: memory documents and their append-only revisions.
--
-- A memory document is identified by its (tenant, scope, owner, path); its content is a
-- projection of the latest revision. Shared memory is space-scoped (bot_id empty); bot memory
-- belongs to one bot inside the space. The revision chain is append-only: the current
-- document is a projection, and revision numbers only increase.

CREATE TABLE memory_documents (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    scope text NOT NULL CHECK (scope IN ('bot', 'shared')),
    -- The owner bot; the empty string for the space-shared memory.
    bot_id text NOT NULL DEFAULT '' REFERENCES bots (id) ON DELETE CASCADE,
    path text NOT NULL,
    content text NOT NULL,
    revision bigint NOT NULL DEFAULT 1 CHECK (revision >= 1),
    updated_at timestamptz NOT NULL,
    UNIQUE (space_id, scope, bot_id, path),
    CHECK ((scope = 'bot' AND bot_id <> '') OR (scope = 'shared' AND bot_id = ''))
);

CREATE TABLE memory_revisions (
    id bigserial PRIMARY KEY,
    document_id text NOT NULL REFERENCES memory_documents (id) ON DELETE CASCADE,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    revision bigint NOT NULL CHECK (revision >= 1),
    content text NOT NULL,
    created_at timestamptz NOT NULL,
    UNIQUE (document_id, revision)
);

CREATE INDEX memory_revisions_document_idx ON memory_revisions (document_id, revision);

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['memory_documents', 'memory_revisions'] LOOP
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
