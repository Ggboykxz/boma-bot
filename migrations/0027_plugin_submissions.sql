-- REQ-034: anonymous plugin submissions, reviewed by an operator before they install.
--
-- A submission is a row someone left without an account: a source, an optional name, an
-- optional contact. Nothing about it is a tenant's data — the review is an admin concern
-- (M9), so the table is deliberately outside row-level security, like `rate_limits`. The
-- contact address is the one piece of personal data; it is stored so the author hears back,
-- never returned by any API.
CREATE TABLE plugin_submissions (
    id text PRIMARY KEY,
    source text NOT NULL,
    name text,
    contact text,
    submitter_user_id text,
    status text NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'unconfirmed', 'approved', 'rejected', 'applied', 'revoked')),
    created_at timestamptz NOT NULL DEFAULT now()
);

-- Reviewed-newest-last order is the review queue's; nothing here is hot enough for more indexes.
CREATE INDEX plugin_submissions_status_created ON plugin_submissions (status, created_at);
