-- REQ-026: a computer per bot, started lazily, released when idle.
--
-- A computer is the sandbox a bot's browser and files live in: one row per (space, bot), at most
-- one per bot. `status` follows the provisioning vocabulary from the data model
-- (`queued, running, interrupted, completed, failed`) plus `stopped` for "not provisioned";
-- `screen_generation` increments on every actual boot and invalidates older screen capabilities
-- (REQ-030). `home_id` names the provider-side storage backing the bot's home — a Docker volume
-- name, a bucket prefix; it survives the container being stopped. `private` isolates a computer
-- from the space's trust boundary (REQ-031). `idle_minutes` and the limits are per computer and
-- bounded. computer_runs records which run holds the computer (at most one open per computer);
-- computer_execution_leases mirrors the run lease for computer ownership.

CREATE TABLE computers (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    provider text NOT NULL,
    external_id text,
    status text NOT NULL DEFAULT 'stopped'
        CHECK (status IN ('queued', 'running', 'interrupted', 'completed', 'failed', 'stopped')),
    screen_generation integer NOT NULL DEFAULT 0 CHECK (screen_generation >= 0),
    home_id text,
    private boolean NOT NULL DEFAULT false,
    memory_limit_mb integer NOT NULL DEFAULT 512 CHECK (memory_limit_mb BETWEEN 64 AND 4096),
    cpu_limit double precision NOT NULL DEFAULT 1 CHECK (cpu_limit BETWEEN 0.25 AND 4),
    idle_minutes integer NOT NULL DEFAULT 10 CHECK (idle_minutes BETWEEN 1 AND 1440),
    started_at timestamptz,
    last_used_at timestamptz,
    stopped_at timestamptz,
    created_at timestamptz NOT NULL,
    updated_at timestamptz NOT NULL,
    UNIQUE (space_id, bot_id)
);

CREATE INDEX computers_idle_idx ON computers (last_used_at)
    WHERE status = 'running' AND last_used_at IS NOT NULL;
CREATE INDEX computers_bot_idx ON computers (bot_id);

CREATE TABLE computer_runs (
    computer_id text NOT NULL REFERENCES computers (id) ON DELETE CASCADE,
    run_id text NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    started_at timestamptz NOT NULL,
    ended_at timestamptz,
    PRIMARY KEY (computer_id, run_id)
);

CREATE UNIQUE INDEX computer_runs_one_open ON computer_runs (computer_id)
    WHERE ended_at IS NULL;

CREATE TABLE computer_execution_leases (
    run_id text PRIMARY KEY REFERENCES runs (id) ON DELETE CASCADE,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    owner text NOT NULL,
    fence bigint NOT NULL DEFAULT 0,
    expires_at timestamptz NOT NULL
);

CREATE TABLE agent_homes (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text REFERENCES bots (id) ON DELETE CASCADE,
    storage_key text NOT NULL,
    archived_at timestamptz,
    created_at timestamptz NOT NULL,
    UNIQUE (space_id, storage_key)
);

CREATE TABLE browser_profiles (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text REFERENCES bots (id) ON DELETE CASCADE,
    storage_key text NOT NULL,
    checkpoint_at timestamptz,
    created_at timestamptz NOT NULL,
    UNIQUE (space_id, storage_key)
);

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['computers', 'computer_runs', 'computer_execution_leases', 'agent_homes', 'browser_profiles'] LOOP
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
