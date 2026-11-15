-- REQ-059 (improvement program): bots backed by an external agent executor speaking a
-- documented agent protocol. One row per external bot; the credential reference points at the
-- space's bot-secret vault, never stores a value. Absence of this row = the bot is internal.
CREATE TABLE bot_external_agents (
    bot_id text PRIMARY KEY REFERENCES bots (id) ON DELETE CASCADE,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    base_url text NOT NULL,
    agent_name text,
    secret_name text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT bot_external_agents_base_url_not_empty CHECK (length(btrim(base_url)) > 0),
    CONSTRAINT bot_external_agents_secret_name_when_present CHECK (
        secret_name IS NULL OR length(btrim(secret_name)) > 0)
);

CREATE INDEX bot_external_agents_space ON bot_external_agents (space_id);

ALTER TABLE bot_external_agents ENABLE ROW LEVEL SECURITY;
ALTER TABLE bot_external_agents FORCE ROW LEVEL SECURITY;

CREATE POLICY bot_external_agents_tenant ON bot_external_agents
    FOR ALL
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
