-- REQ-046, spec DEC-10: envelope encryption for bot secrets (docs/adr/020-secrets.md).
--
-- Every space gets a random 256-bit data-encryption key (DEK), stored wrapped (AES-256-GCM)
-- under the deployment's master key (`BOMA_MASTER_KEY`). Every bot secret is sealed under its
-- space's DEK; rotating the master key re-wraps the DEKs only, never the secrets. The wrapped
-- DEKs are ciphertext; the plaintext key never persists.
CREATE TABLE space_keys (
    space_id text PRIMARY KEY REFERENCES spaces (id) ON DELETE CASCADE,
    wrapped_key bytea NOT NULL,
    nonce bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    rotated_at timestamptz
);

DO $$
DECLARE
    tenant_table text;
BEGIN
    FOREACH tenant_table IN ARRAY ARRAY['space_keys'] LOOP
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
