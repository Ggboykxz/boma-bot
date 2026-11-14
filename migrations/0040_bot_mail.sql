-- REQ-047: inbound mail for bots, with sender verification.
--
-- A bot may be reachable at `boma+{bot_id}@<deployment mail domain>`; a relay (SMTP in / a
-- provider webhook) POSTs the raw message to the deployment with a relay bearer. Senders are
-- verified: a member of the space (their account email) or an entry of the bot's allowlist.
-- Anything else is held for the member to release or drop, or dropped outright — the policy is
-- the bot's, and `hold` is the safe default.
CREATE TABLE bot_mail_rules (
    bot_id text PRIMARY KEY REFERENCES bots (id) ON DELETE CASCADE,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    policy text NOT NULL DEFAULT 'hold' CHECK (policy IN ('hold', 'drop')),
    allowed_senders jsonb NOT NULL DEFAULT '[]'::jsonb,
    updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE bot_mail_rules ENABLE ROW LEVEL SECURITY;
ALTER TABLE bot_mail_rules FORCE ROW LEVEL SECURITY;
CREATE POLICY bot_mail_rules_tenant_isolation ON bot_mail_rules
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

CREATE TABLE held_mail (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    sender text NOT NULL,
    subject text NOT NULL DEFAULT '',
    body text NOT NULL DEFAULT '',
    reason text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX held_mail_bot ON held_mail (space_id, bot_id, created_at DESC);

ALTER TABLE held_mail ENABLE ROW LEVEL SECURITY;
ALTER TABLE held_mail FORCE ROW LEVEL SECURITY;
CREATE POLICY held_mail_tenant_isolation ON held_mail
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
