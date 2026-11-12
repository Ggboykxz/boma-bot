-- M2: approval and question cards, and the exactly-once effects they authorize.
--
-- A card is a pending question or approval on a thread (REQ-007, REQ-017). Answering a card
-- closes it; the run that waited on it resumes. The effect an approval authorizes is recorded
-- once per effect key, so a retried or replayed answer never executes it twice (REQ-017).

CREATE TABLE cards (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    run_id text REFERENCES runs (id) ON DELETE SET NULL,
    kind text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    answer text,
    created_at timestamptz NOT NULL,
    answered_at timestamptz
);

CREATE UNIQUE INDEX cards_thread_run_key
    ON cards (thread_id, run_id)
    WHERE run_id IS NOT NULL AND status = 'pending';

CREATE TABLE external_effects (
    run_id text NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
    effect_key text NOT NULL,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    kind text NOT NULL,
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    applied_at timestamptz,
    PRIMARY KEY (run_id, effect_key)
);

-- The tenant isolation loop covers the two new tables the same way it covers the others.
DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['cards', 'external_effects'] LOOP
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
