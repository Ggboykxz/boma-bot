-- REQ-010: group chats with several bots.
--
-- One group = one shared thread threaded through its bot members plus the space members that
-- follow it. Bot membership and person membership share one table to keep the door-keeping
-- rule single-sourced (the use cases read it once per message).
CREATE TABLE groups (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    name text NOT NULL,
    -- The conversation everyone shares; the thread's bot_id is the first bot member.
    thread_id text NOT NULL REFERENCES threads (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL,
    CONSTRAINT groups_name_not_empty CHECK (length(btrim(name)) > 0)
);

CREATE INDEX groups_space ON groups (space_id);

CREATE TABLE group_memberships (
    group_id text NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
    member_kind text NOT NULL CHECK (member_kind IN ('person', 'bot')),
    member_id text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, member_kind, member_id)
);

CREATE INDEX group_memberships_bot ON group_memberships (member_kind, member_id);

ALTER TABLE groups ENABLE ROW LEVEL SECURITY;
ALTER TABLE groups FORCE ROW LEVEL SECURITY;
ALTER TABLE group_memberships ENABLE ROW LEVEL SECURITY;
ALTER TABLE group_memberships FORCE ROW LEVEL SECURITY;

CREATE POLICY groups_tenant ON groups
    FOR ALL
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

-- A membership row inherits the tenant of its group; without it the kind+id pair would be
-- readable cross-tenant. `boma.in_space` takes the space id so the check stays index shaped.
CREATE POLICY group_memberships_tenant ON group_memberships
    FOR ALL
    USING ((SELECT space_id FROM groups WHERE groups.id = group_memberships.group_id) = boma.current_space_id() OR boma.is_system())
    WITH CHECK ((SELECT space_id FROM groups WHERE groups.id = group_memberships.group_id) = boma.current_space_id() OR boma.is_system());
