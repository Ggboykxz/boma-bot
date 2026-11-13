-- REQ-046: write-only bot secrets, encrypted at rest.
--
-- A bot secret is a name and an encrypted value. `ciphertext` is AES-256-GCM with the
-- deployment's BOMA_MASTER_KEY (see docs/adr/019-secrets.md); the nonce is stored alongside.
-- The value is never returned by any query path visible to a person: list answers names only.
-- Deleting is idempotent: forgetting a secret removes the row exactly once.
CREATE TABLE bot_secrets (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    name text NOT NULL,
    ciphertext bytea NOT NULL,
    nonce bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (name ~ '^[A-Z][A-Z0-9_]{0,63}$'),
    UNIQUE (space_id, bot_id, name)
);

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['bot_secrets'] LOOP
        EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', tenant_table);
        EXECUTE format('ALTER TABLE %I FORCE ROW LEVEL SECURITY', tenant_table);
        EXECUTE format(
            'CREATE POLICY %I ON %I USING (space_id = boma.current_space_id() OR boma.is_system()) '
            'WITH CHECK (space_id = boma.current_space_id() OR boma.is_system())',
            tenant_table || '_tenant_isolation',
            tenant_table
        );
    END LOOP;
END
$$;
