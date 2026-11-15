-- REQ-044: one row per account export.
--
-- No spaces: a person requests their own data, crossing every space where it exists. RLS allows
-- system-reason reads only; the application checks ownership of the export row explicitly.
CREATE TABLE account_exports (
    id text PRIMARY KEY,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'ready', 'failed')),
    payload_bytes bigint NOT NULL DEFAULT 0,
    skipped text[] NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX account_exports_user ON account_exports (user_id, created_at DESC);

ALTER TABLE account_exports ENABLE ROW LEVEL SECURITY;
ALTER TABLE account_exports FORCE ROW LEVEL SECURITY;
CREATE POLICY account_exports_system_only ON account_exports
    USING (boma.is_system())
    WITH CHECK (boma.is_system());
