-- M0: one forward-only migration so `boma migrate` applies, records and is idempotent.
--
-- Domain tables arrive with their milestone, each with its RLS policy from the first commit
-- (REQ-043, ADR-002). This table exists so the migration pipeline can be observed end to end
-- without inventing a schema the spec does not describe yet.

CREATE TABLE IF NOT EXISTS app_meta (
    key text PRIMARY KEY,
    value jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

INSERT INTO app_meta (key, value)
VALUES ('schema', '{"milestone": "M0"}'::jsonb)
ON CONFLICT (key) DO NOTHING;

COMMENT ON TABLE app_meta IS
    'Deployment metadata written by migrations; never tenant data, never readable by a client.';
