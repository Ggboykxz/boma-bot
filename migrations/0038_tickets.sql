-- REQ-038: the ticket board, only present when the deployment enables it.
--
-- A board is a per-space counter of keys plus tickets and comments. The counter lives in
-- `board_state` (one row per space), so keys are `BOT-<n>`, monotonic and race-free under the
-- same transaction as the insert. The flag is a deployment concern: without BOMA_BOARD=true
-- the API surface is absent, and these tables are inert.
CREATE TABLE board_state (
    space_id text PRIMARY KEY REFERENCES spaces (id) ON DELETE CASCADE,
    next_seq integer NOT NULL DEFAULT 1
);

ALTER TABLE board_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE board_state FORCE ROW LEVEL SECURITY;
CREATE POLICY board_state_tenant_isolation ON board_state
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

CREATE TABLE tickets (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    key text NOT NULL,
    title text NOT NULL,
    description text NOT NULL DEFAULT '',
    state text NOT NULL DEFAULT 'backlog' CHECK (state IN ('backlog', 'todo', 'in_progress', 'done')),
    assignee_bot_id text REFERENCES bots (id) ON DELETE SET NULL,
    created_by text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    comment_count integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (space_id, key)
);

ALTER TABLE tickets ENABLE ROW LEVEL SECURITY;
ALTER TABLE tickets FORCE ROW LEVEL SECURITY;
CREATE POLICY tickets_tenant_isolation ON tickets
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

CREATE TABLE ticket_comments (
    id text PRIMARY KEY,
    ticket_id text NOT NULL REFERENCES tickets (id) ON DELETE CASCADE,
    author_kind text NOT NULL CHECK (author_kind IN ('member', 'bot')),
    author_id text NOT NULL,
    body text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX ticket_comments_ticket ON ticket_comments (ticket_id, created_at);

ALTER TABLE ticket_comments ENABLE ROW LEVEL SECURITY;
ALTER TABLE ticket_comments FORCE ROW LEVEL SECURITY;
CREATE POLICY ticket_comments_tenant_isolation ON ticket_comments
    USING (
        EXISTS (
            SELECT 1 FROM tickets t
            WHERE t.id = ticket_comments.ticket_id
              AND (t.space_id = boma.current_space_id() OR boma.is_system())
        )
    )
    WITH CHECK (
        EXISTS (
            SELECT 1 FROM tickets t
            WHERE t.id = ticket_comments.ticket_id
              AND (t.space_id = boma.current_space_id() OR boma.is_system())
        )
    );
