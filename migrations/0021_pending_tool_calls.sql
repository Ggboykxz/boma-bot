-- REQ-016/REQ-017: a tool call that needs approval suspends here.
--
-- The model asked for a tool. The worker records the call once and parks the run; a card asks a
-- person. On approval the call is executed exactly once from this row, its result is written as
-- a tool message on the thread, and the run is re-queued so the next pass picks it up. The
-- tenant isolation loop below covers the new table like the others.
CREATE TABLE pending_tool_calls (
    run_id text NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
    call_id text NOT NULL,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    name text NOT NULL,
    arguments jsonb NOT NULL DEFAULT '{}'::jsonb,
    status text NOT NULL DEFAULT 'pending',
    result jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    answered_at timestamptz,
    PRIMARY KEY (run_id, call_id)
);

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['pending_tool_calls'] LOOP
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
