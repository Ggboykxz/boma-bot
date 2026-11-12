-- REQ-023: routines are cron definitions that wake a bot.
--
-- A routine belongs to one bot and one space. `next_run_at` is computed in the routine's
-- timezone and stored as UTC; an active routine has one. `@once` is a special sentinel in
-- `crons`: it fires a single time and deactivates instead of rescheduling. `account_failures`
-- counts consecutive failures; it resets on success and pauses the routine past the threshold.

CREATE TABLE routines (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name text NOT NULL,
    prompt text NOT NULL,
    crons text[] NOT NULL,
    timezone text NOT NULL,
    active boolean NOT NULL DEFAULT true,
    notify boolean NOT NULL DEFAULT false,
    last_run_at timestamptz,
    next_run_at timestamptz,
    account_failures integer NOT NULL DEFAULT 0 CHECK (account_failures >= 0),
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    CHECK (name <> ''),
    CHECK (cardinality(crons) >= 1)
);

CREATE INDEX routines_due_idx ON routines (next_run_at)
    WHERE active AND next_run_at IS NOT NULL;
CREATE INDEX routines_thread_idx ON routines (thread_id);

CREATE TABLE routine_runs (
    id bigserial PRIMARY KEY,
    routine_id text NOT NULL REFERENCES routines (id) ON DELETE CASCADE,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    run_id text REFERENCES runs (id) ON DELETE SET NULL,
    outcome text NOT NULL CHECK (outcome IN ('fired', 'coalesced', 'skipped', 'failed', 'paused')),
    detail text,
    occurred_at timestamptz NOT NULL
);

CREATE INDEX routine_runs_routine_idx ON routine_runs (routine_id);
CREATE INDEX routine_runs_run_idx ON routine_runs (run_id);

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['routines', 'routine_runs'] LOOP
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
