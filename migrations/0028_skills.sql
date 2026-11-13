-- REQ-035: taught and plugin skills, one row per (space, name).
--
-- Built-in skills are code, not rows. A skill is instructions, not code: it is loaded into
-- a run, never executed. `plugin_source` records where an installed skill came from so the
-- omission report can name it; taught skills carry NULL.
CREATE TABLE skills (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    name text NOT NULL,
    description text NOT NULL,
    instructions text NOT NULL,
    origin text NOT NULL CHECK (origin IN ('taught', 'plugin')),
    plugin_source text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (space_id, name)
);

ALTER TABLE skills ENABLE ROW LEVEL SECURITY;
ALTER TABLE skills FORCE ROW LEVEL SECURITY;
CREATE POLICY skills_tenant_isolation ON skills
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
