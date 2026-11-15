-- REQ-040: password recovery tokens.
--
-- One row per issued recovery request; consumed once. The hash is stored, never the raw
-- token; the deployment operator that has no outbound mail channel delivers the link by
-- stdout when BOMA_AUTH_PRINT_RESET_LINK=true, which is an opt-in for exactly this case.
CREATE TABLE password_reset_tokens (
    id text PRIMARY KEY,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    used_at timestamptz,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX password_reset_tokens_user ON password_reset_tokens (user_id, created_at DESC);
CREATE INDEX password_reset_tokens_active ON password_reset_tokens (token_hash) WHERE used_at IS NULL;

ALTER TABLE password_reset_tokens ENABLE ROW LEVEL SECURITY;
ALTER TABLE password_reset_tokens FORCE ROW LEVEL SECURITY;
CREATE POLICY password_reset_tokens_system_only ON password_reset_tokens
    USING (boma.is_system())
    WITH CHECK (boma.is_system());
