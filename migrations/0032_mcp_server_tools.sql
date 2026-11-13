-- REQ-033: the tools a probe discovered on an MCP server, per space.
--
-- A run's catalogue reads this table, never the network: a model turn must not pay a round
-- trip to every server before it can see its tools. A probe replaces the rows of its server
-- atomically.
CREATE TABLE mcp_server_tools (
    server_id text NOT NULL REFERENCES mcp_servers (id) ON DELETE CASCADE,
    name text NOT NULL,
    description text NOT NULL DEFAULT '',
    input_schema jsonb NOT NULL DEFAULT '{}'::jsonb,
    probed_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (server_id, name)
);

ALTER TABLE mcp_server_tools ENABLE ROW LEVEL SECURITY;
ALTER TABLE mcp_server_tools FORCE ROW LEVEL SECURITY;
CREATE POLICY mcp_server_tools_tenant_isolation ON mcp_server_tools
    USING (
        EXISTS (
            SELECT 1 FROM mcp_servers s
            WHERE s.id = mcp_server_tools.server_id
              AND (s.space_id = boma.current_space_id() OR boma.is_system())
        )
    )
    WITH CHECK (
        EXISTS (
            SELECT 1 FROM mcp_servers s
            WHERE s.id = mcp_server_tools.server_id
              AND (s.space_id = boma.current_space_id() OR boma.is_system())
        )
    );
