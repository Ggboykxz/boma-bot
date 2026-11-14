-- REQ-048: messaging platforms.
--
-- A member installs the product into a workspace (Slack, Telegram, or a generic HMAC-signed
-- surface); a user messages the bot there, a run starts, and the reply is delivered to the same
-- conversation. The data model's thread kinds are: `bot` (one bot, one member context) and
-- `external` (a platform conversation). An external thread belongs to the bot of the
-- installation and is uniquely identified by (platform, workspace_id, conversation_id).
--
-- Replying is the platform's job: the worker delivers the bot's final message to the
-- installation when the run ends. A removed installation stops inbound work; its external
-- threads keep their history, like a bot archive keeps its thread (REQ-021).

ALTER TABLE threads
    ADD COLUMN kind text NOT NULL DEFAULT 'bot' CHECK (kind IN ('bot', 'external')),
    ADD COLUMN external_platform text,
    ADD COLUMN external_workspace_id text,
    ADD COLUMN external_conversation_id text;

CREATE UNIQUE INDEX threads_external_identity
    ON threads (external_platform, external_workspace_id, external_conversation_id)
    WHERE kind = 'external';

-- One installation per platform workspace, deployment wide: the ingress resolves an
-- installation by (platform, workspace) unambiguously, and a member sees it listed exactly once.
-- The platform credential is sealed under the space's data key on write, exactly like a bot
-- secret (REQ-046); the deployment's Slack signing secret is configuration, not data.
CREATE TABLE messaging_installations (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    installed_by text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    platform text NOT NULL CHECK (platform IN ('slack', 'telegram', 'generic')),
    workspace_id text NOT NULL,
    workspace_name text NOT NULL DEFAULT '',
    -- The per-installation platform credential, sealed: a Slack bot token (`xoxb-…`), the
    -- Telegram bot token, or the generic HMAC secret.
    token_ciphertext bytea NOT NULL,
    token_nonce bytea NOT NULL,
    -- A second sealed credential when the platform needs one: the per-installation Telegram
    -- webhook secret. NULL for Slack and generic.
    webhook_ciphertext bytea,
    webhook_nonce bytea,
    -- The generic platform's reply target. HTTPS; a private host is allowed only when the member
    -- opts in explicitly (`allow_private`), audited (D-05).
    reply_url text,
    allow_private boolean NOT NULL DEFAULT false,
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'removed')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX messaging_installations_platform_workspace
    ON messaging_installations (platform, workspace_id);
CREATE INDEX messaging_installations_bot ON messaging_installations (space_id, bot_id);

ALTER TABLE messaging_installations ENABLE ROW LEVEL SECURITY;
ALTER TABLE messaging_installations FORCE ROW LEVEL SECURITY;
CREATE POLICY messaging_installations_tenant_isolation ON messaging_installations
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

-- The Slack install handshake, single use: Slack redirects the browser to the callback with a
-- `code`; the state proves which space, bot and user started the flow, and is consumed exactly
-- once. Expires after ten minutes.
CREATE TABLE messaging_install_states (
    state text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE messaging_install_states ENABLE ROW LEVEL SECURITY;
ALTER TABLE messaging_install_states FORCE ROW LEVEL SECURITY;
CREATE POLICY messaging_install_states_tenant_isolation ON messaging_install_states
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
