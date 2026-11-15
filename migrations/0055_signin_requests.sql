-- REQ-060 (improvement program): a bot asks a person for a sign-in to a site; the person grants
-- or refuses through the same approval card as every other write. Secrets never travel over the
-- RPC surface — this table is an audit trail of who granted which origin to which bot.
CREATE TABLE signin_requests (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    requested_by text NOT NULL,
    thread_id text,
    origin text NOT NULL,
    display_name text NOT NULL,
    status text NOT NULL DEFAULT 'pending',
    granted_by text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT signin_requests_status_valid CHECK (status IN ('pending', 'granted', 'refused')),
    CONSTRAINT signin_requests_origin_not_empty CHECK (length(btrim(origin)) > 0)
);

CREATE INDEX signin_requests_space ON signin_requests (space_id);
CREATE INDEX signin_requests_pending ON signin_requests (space_id, status);

ALTER TABLE signin_requests ENABLE ROW LEVEL SECURITY;
ALTER TABLE signin_requests FORCE ROW LEVEL SECURITY;

CREATE POLICY signin_requests_tenant ON signin_requests
    FOR ALL
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
