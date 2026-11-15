-- REQ-058: access grants.
--
-- A server-level allow-list of accounts that a hosted deployment recognizes: the owner decides,
-- a hosted deployment can end a person's access with one row, and never by shipping an update.
-- No tenant data; only the system can read or write.
CREATE TABLE access_grants (
    email text PRIMARY KEY,
    reason text NOT NULL,
    granted_by text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE access_grants ENABLE ROW LEVEL SECURITY;
ALTER TABLE access_grants FORCE ROW LEVEL SECURITY;
CREATE POLICY access_grants_system_only ON access_grants
    USING (boma.is_system())
    WITH CHECK (boma.is_system());
