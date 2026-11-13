-- REQ-032: connector installs, per space, once each.
--
-- A catalogue exists in code (see `builtin_connectors` in `boma-domain`); this table is what a
-- space installed from it. Credentials ride the per-space data key (ADR-020 machinery); listings
-- never carry them, `connected_credential` is the only read path, and `status` is the switch
-- the catalogue respects: a revoked install contributes no tools.
CREATE TABLE connector_installs (
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    connector_id text NOT NULL,
    auth_kind text NOT NULL CHECK (auth_kind IN ('none', 'api_key')),
    sealed_credential bytea,
    sealed_credential_nonce bytea,
    status text NOT NULL DEFAULT 'connected' CHECK (status IN ('connected', 'revoked')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (space_id, connector_id)
);

ALTER TABLE connector_installs ENABLE ROW LEVEL SECURITY;
ALTER TABLE connector_installs FORCE ROW LEVEL SECURITY;
CREATE POLICY connector_installs_tenant_isolation ON connector_installs
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
