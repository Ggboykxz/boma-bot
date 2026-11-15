-- REQ-052: model credentials per space, and the person's model preference.
--
-- A credential is sealed under the space's data key, exactly like a bot secret; the catalogue
-- view and the credential list never carry it, only whether the provider is connected. The
-- preference is per person (the account-level default), not per space: it is the member's
-- standing choice for which model runs for them.
CREATE TABLE model_credentials (
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    provider text NOT NULL,
    -- `connected` after a live validation call; `invalid` is never stored: a failed connect
    -- stores nothing (REQ-052).
    status text NOT NULL DEFAULT 'connected' CHECK (status IN ('connected')),
    ciphertext bytea NOT NULL,
    nonce bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (space_id, provider)
);

ALTER TABLE model_credentials ENABLE ROW LEVEL SECURITY;
ALTER TABLE model_credentials FORCE ROW LEVEL SECURITY;
CREATE POLICY model_credentials_tenant_isolation ON model_credentials
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

-- Per-person model preference; the spec's resolution order reads person after space and
-- before the deployment default (REQ-015). Keyed by the user; not tenant scoped because a
-- person's preference is theirs across spaces.
CREATE TABLE user_model_preferences (
    user_id text PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    model_key text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
