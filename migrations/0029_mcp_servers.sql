-- REQ-033: MCP servers a space connects to, and the OAuth sessions that authorize them.
--
-- Transport is `http` (streamable HTTP JSON-RPC, the only remote transport shipped) or `stdio`
-- (local command, spawned only when the deployment's MCP_STDIO_ALLOWED_COMMANDS lists the
-- binary — empty by default). `allow_private` is the per-space exception D-05 asks for: it
-- releases the private-address check for THIS server in THIS space, never deployment-wide; every
-- use of a private endpoint writes an audit event (see 0030).
CREATE TABLE mcp_servers (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    name text NOT NULL,
    transport text NOT NULL CHECK (transport IN ('http', 'stdio')),
    url text,
    command text,
    auth_state text NOT NULL DEFAULT 'none' CHECK (auth_state IN ('none', 'oauth_pending', 'connected', 'error')),
    allow_private boolean NOT NULL DEFAULT false,
    tool_count integer,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (space_id, name),
    CHECK ((transport = 'http' AND url IS NOT NULL AND command IS NULL)
        OR (transport = 'stdio' AND command IS NOT NULL AND url IS NULL))
);

ALTER TABLE mcp_servers ENABLE ROW LEVEL SECURITY;
ALTER TABLE mcp_servers FORCE ROW LEVEL SECURITY;
CREATE POLICY mcp_servers_tenant_isolation ON mcp_servers
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

-- One pending OAuth attempt per server; the verifier never leaves the vault (sealed here).
CREATE TABLE mcp_oauth_sessions (
    server_id text PRIMARY KEY REFERENCES mcp_servers (id) ON DELETE CASCADE,
    state text NOT NULL,
    sealed_verifier bytea NOT NULL,
    sealed_verifier_nonce bytea NOT NULL,
    expires_at timestamptz NOT NULL
);

ALTER TABLE mcp_oauth_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE mcp_oauth_sessions FORCE ROW LEVEL SECURITY;
CREATE POLICY mcp_oauth_sessions_tenant_isolation ON mcp_oauth_sessions
    USING (
        EXISTS (
            SELECT 1 FROM mcp_servers s
            WHERE s.id = mcp_oauth_sessions.server_id
              AND (s.space_id = boma.current_space_id() OR boma.is_system())
        )
    )
    WITH CHECK (
        EXISTS (
            SELECT 1 FROM mcp_servers s
            WHERE s.id = mcp_oauth_sessions.server_id
              AND (s.space_id = boma.current_space_id() OR boma.is_system())
        )
    );
