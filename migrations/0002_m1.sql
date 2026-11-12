-- M1 — the product's tables, and the row level security that keeps two spaces apart (REQ-043).
--
-- Every tenant table carries `space_id` and is protected by a policy that compares it with the
-- tenant the connection currently acts in. The tenant is a session setting named
-- `boma.space_id`, set per transaction by the infra crate (`boma_infra::tenant::with_tenant`).
--
-- The setting name is part of the contract: ADR-002's CI check greps for `boma.space_id`, and a
-- query that runs without it sees nothing at all. That is the intended failure mode — a forgotten
-- tenant context is an empty result, never a cross-tenant leak.
--
-- The `boma` schema holds the helpers the policies call. It holds no application data.

-- The tables live in `public`, whatever the search path of the session is.
--
-- A role named after a schema ("$user" in the default search path) would otherwise receive the
-- tables of this migration in its own schema: the helpers below are in `boma`, so a role called
-- `boma` would find every table of the product there instead of in `public`. The application
-- pool sets the same search path, so the migration and the product always agree.
SET search_path TO public;

CREATE SCHEMA IF NOT EXISTS boma;

COMMENT ON SCHEMA boma IS
    'Helpers for the row level security policies. Never application data.';

-- The tenant the connection acts in: the space id, or NULL when no tenant was set.
CREATE OR REPLACE FUNCTION boma.current_space_id() RETURNS text
    LANGUAGE sql STABLE
    AS $$
    SELECT current_setting('boma.space_id', true)::text
    $$;

-- Whether the connection runs on a system path, allowed to cross tenants.
--
-- The reason comes from a closed enum (`boma_domain::SystemReason`), never from free text: a
-- free-text reason would let any caller claim any exception (defect D-09). The value is empty
-- when the connection acts as a member, which is what makes the policies deny by default.
CREATE OR REPLACE FUNCTION boma.is_system() RETURNS boolean
    LANGUAGE sql STABLE
    AS $$
    SELECT COALESCE(current_setting('boma.system_reason', true)::text, '') <> ''
    $$;

-- The canonical rendering of a timestamp: RFC 3339 in UTC, second precision.
--
-- It is the shape `boma_infra::crypto::SystemClock` writes, so a timestamp read back from the
-- database compares correctly with one the application produced (the session expiry is compared
-- as text).
CREATE OR REPLACE FUNCTION boma.rfc3339(at timestamptz) RETURNS text
    LANGUAGE sql IMMUTABLE
    AS $$
    SELECT to_char(at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS"Z"')
    $$;

COMMENT ON FUNCTION boma.current_space_id() IS
    'The space the current transaction acts in. Reads `boma.space_id`, the setting the CI check greps for.';
COMMENT ON FUNCTION boma.is_system() IS
    'Whether the current transaction runs on a system path (`boma.system_reason` is not empty).';
COMMENT ON FUNCTION boma.rfc3339(timestamptz) IS
    'RFC 3339 in UTC with second precision, the format the application writes and compares.';

-- ---------------------------------------------------------------------------------------------
-- Tables that are not tenant scoped.
--
-- A person, their session and the organization that owns the spaces are global: they are not
-- owned by a space, so no row level security applies to them. The repositories that read them
-- still work, and the application checks the membership itself.
-- ---------------------------------------------------------------------------------------------

CREATE TABLE users (
    id text PRIMARY KEY,
    email text NOT NULL UNIQUE,
    name text NOT NULL,
    email_verified boolean NOT NULL DEFAULT false,
    locale text NOT NULL DEFAULT 'en-US',
    password_hash text NOT NULL,
    created_at timestamptz NOT NULL,
    deactivated_at timestamptz
);

COMMENT ON TABLE users IS
    'The people who sign in. Global: not owned by a space, so no row level security applies.';

CREATE TABLE organizations (
    id text PRIMARY KEY,
    name text NOT NULL,
    created_at timestamptz NOT NULL
);

COMMENT ON TABLE organizations IS
    'The billing entities that own spaces. Global: not owned by a space.';

CREATE TABLE organization_members (
    organization_id text NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role text NOT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (organization_id, user_id)
);

COMMENT ON TABLE organization_members IS
    'Who belongs to which organization. Global: not owned by a space.';

CREATE TABLE sessions (
    id text PRIMARY KEY,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL
);

COMMENT ON TABLE sessions IS
    'The sign-in sessions. Only the hash of the token is stored, never the token itself.';

-- ---------------------------------------------------------------------------------------------
-- The tenant tables.
--
-- Every one of them carries `space_id` and is protected by a policy that compares it with
-- `boma.space_id`.
-- ---------------------------------------------------------------------------------------------

CREATE TABLE spaces (
    id text PRIMARY KEY,
    organization_id text NOT NULL REFERENCES organizations (id) ON DELETE CASCADE,
    name text NOT NULL,
    is_default boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL
);

COMMENT ON TABLE spaces IS
    'The tenants. A space is its own tenant: its primary key is the space id.';

CREATE TABLE space_members (
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role text NOT NULL,
    created_at timestamptz NOT NULL,
    PRIMARY KEY (space_id, user_id)
);

CREATE TABLE bots (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name text NOT NULL,
    description text,
    color text,
    -- The conversation of this bot. Plain text, not a foreign key: the bot and its thread are
    -- written together in one transaction, and a cycle of two foreign keys cannot be satisfied.
    thread_id text,
    archived_at timestamptz,
    created_at timestamptz NOT NULL
);

CREATE TABLE threads (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- The sequence the next message takes. The store allocates it, in the same transaction as
    -- the row it belongs to, so two concurrent sends never claim the same number (REQ-005).
    next_message_seq bigint NOT NULL DEFAULT 1,
    -- The sequence the next event takes, allocated the same way (REQ-005, REQ-020).
    next_event_seq bigint NOT NULL DEFAULT 1,
    created_at timestamptz NOT NULL
);

CREATE TABLE messages (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    seq bigint NOT NULL,
    role text NOT NULL,
    blocks jsonb NOT NULL DEFAULT '[]'::jsonb,
    bot_id text,
    author_user_id text,
    -- Plain text: the message is written before the run it starts, so this cannot be a key.
    run_id text,
    -- The nonce that makes a send idempotent. The partial unique index below is what makes a
    -- retried send find the message it already wrote (REQ-002).
    client_nonce text,
    created_at timestamptz NOT NULL,
    UNIQUE (thread_id, seq)
);

CREATE UNIQUE INDEX messages_thread_client_nonce_key
    ON messages (thread_id, client_nonce)
    WHERE client_nonce IS NOT NULL;

CREATE TABLE runs (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    status text NOT NULL,
    trigger text NOT NULL,
    client_nonce text,
    source_message_id text,
    lease_owner text,
    -- The fence token: it grows on every claim, so a worker that was replaced cannot write
    -- (REQ-012).
    lease_fence bigint NOT NULL DEFAULT 0,
    lease_expires_at timestamptz,
    started_at timestamptz,
    completed_at timestamptz,
    error text,
    created_at timestamptz NOT NULL
);

CREATE UNIQUE INDEX runs_thread_client_nonce_key
    ON runs (thread_id, client_nonce)
    WHERE client_nonce IS NOT NULL;

CREATE TABLE events (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    thread_id text NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    seq bigint NOT NULL,
    type text NOT NULL,
    payload jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL,
    UNIQUE (thread_id, seq)
);

CREATE TABLE invitations (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    email text NOT NULL,
    -- The hash of the token; the token itself is never stored (REQ-042).
    token_hash bytea NOT NULL,
    role text NOT NULL,
    expires_at timestamptz NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    created_at timestamptz NOT NULL
);

-- ---------------------------------------------------------------------------------------------
-- The policies.
--
-- A tenant table is readable and writable only by the tenant the connection acts in, or by a
-- system path. `FORCE ROW LEVEL SECURITY` makes the rule apply to the table's owner too, so a
-- deployment that owns its tables with the application role is protected as well.
-- ---------------------------------------------------------------------------------------------

-- The `spaces` table is its own tenant: its primary key *is* the space id, so its policy
-- compares `id` with the current tenant instead of a `space_id` column. Every other tenant
-- table compares its own `space_id` column.
ALTER TABLE spaces ENABLE ROW LEVEL SECURITY;
ALTER TABLE spaces FORCE ROW LEVEL SECURITY;

CREATE POLICY spaces_tenant_isolation ON spaces
    USING (id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (id = boma.current_space_id() OR boma.is_system());

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY[
        'space_members',
        'bots',
        'threads',
        'messages',
        'runs',
        'events',
        'invitations'
    ]
    LOOP
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

COMMENT ON POLICY spaces_tenant_isolation ON spaces IS
    'A space is visible to the tenant that is that space, and to system paths.';

-- ---------------------------------------------------------------------------------------------
-- The indexes the product's queries need.
-- ---------------------------------------------------------------------------------------------

CREATE INDEX space_members_user_id_idx ON space_members (user_id);
CREATE INDEX bots_space_id_idx ON bots (space_id);
CREATE INDEX threads_bot_id_idx ON threads (bot_id);
CREATE INDEX messages_thread_seq_idx ON messages (thread_id, seq DESC);
CREATE INDEX runs_status_created_at_idx ON runs (status, created_at);
CREATE INDEX runs_bot_id_idx ON runs (bot_id);
CREATE INDEX runs_thread_id_idx ON runs (thread_id);
CREATE INDEX events_thread_seq_idx ON events (thread_id, seq);
CREATE INDEX invitations_token_hash_idx ON invitations (token_hash);
CREATE INDEX invitations_space_id_idx ON invitations (space_id);
CREATE INDEX sessions_user_id_idx ON sessions (user_id);
