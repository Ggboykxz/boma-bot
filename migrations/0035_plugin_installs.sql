-- REQ-034: what a space installed from a plugin source.
--
-- One row per (space, source): reinstalling the same source replaces its skills (the install
-- step revokes and rewrites the plugin's own skills). `parts` records the installed component
-- counts; `skipped` records every part left out and why — the no-local-code rule is what makes
-- a plugin safe to install, and the naming of what was left out is its contract.
CREATE TABLE plugin_installs (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    source text NOT NULL,
    resolved_sha text,
    name text NOT NULL,
    parts jsonb NOT NULL DEFAULT '{}'::jsonb,
    skipped jsonb NOT NULL DEFAULT '[]'::jsonb,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (space_id, source)
);

ALTER TABLE plugin_installs ENABLE ROW LEVEL SECURITY;
ALTER TABLE plugin_installs FORCE ROW LEVEL SECURITY;
CREATE POLICY plugin_installs_tenant_isolation ON plugin_installs
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
