-- REQ-061 (improvement program): bots backed by a local ACP CLI executor installed on the
-- machine that runs the deployment. One row per bot; the executor is a known id from the
-- compiled-in registry. Absence of this row = the bot does not run ACP.
CREATE TABLE bot_acp_agents (
    bot_id text PRIMARY KEY REFERENCES bots (id) ON DELETE CASCADE,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    executor text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT bot_acp_agents_executor_not_empty CHECK (length(btrim(executor)) > 0)
);

CREATE INDEX bot_acp_agents_space ON bot_acp_agents (space_id);

ALTER TABLE bot_acp_agents ENABLE ROW LEVEL SECURITY;
ALTER TABLE bot_acp_agents FORCE ROW LEVEL SECURITY;

CREATE POLICY bot_acp_agents_tenant ON bot_acp_agents
    FOR ALL
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
