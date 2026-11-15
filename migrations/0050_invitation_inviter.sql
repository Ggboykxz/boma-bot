-- REQ-042: record who issued one invitation.
--
-- The invite page answers "who invited you?" without any account data leaking to an anonymous
-- caller: the token is the only credential, the space name and the inviter's display name are
-- all that is offered, and any other field stays invisible.
ALTER TABLE invitations
    ADD COLUMN inviter_user_id text REFERENCES users (id) ON DELETE SET NULL;
