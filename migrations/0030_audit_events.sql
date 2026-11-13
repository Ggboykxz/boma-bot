-- D-05: audit events for the escape hatches, starting with private MCP endpoints.
--
-- Every use of a private MCP endpoint writes one row: who, which server, which space, when.
-- The table is append-only, row-level secured like every other tenant table; the export and
-- retention story is M9 (REQ-045), which will read it, never rewrite it.
CREATE TABLE audit_events (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    actor_user_id text,
    kind text NOT NULL CHECK (kind IN ('mcp.private_call')),
    target text NOT NULL,
    metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX audit_events_space_created ON audit_events (space_id, created_at DESC);

ALTER TABLE audit_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_events FORCE ROW LEVEL SECURITY;
CREATE POLICY audit_events_tenant_isolation ON audit_events
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
