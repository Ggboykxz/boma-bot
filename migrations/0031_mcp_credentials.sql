-- REQ-033/REQ-046: sealed credentials of an MCP server, outside row-level RLS mutations.
--
-- Kept separate from 0029 so a recorded migration is never edited: this adds the columns the
-- store seals into, each nullable (a server has at most one kind of credential set at a time;
-- the use cases decide which).
ALTER TABLE mcp_servers
    ADD COLUMN bearer_token bytea,
    ADD COLUMN bearer_token_nonce bytea,
    ADD COLUMN oauth_authorization_url text,
    ADD COLUMN oauth_token_url text,
    ADD COLUMN oauth_client_id text,
    ADD COLUMN sealed_client_secret bytea,
    ADD COLUMN sealed_client_secret_nonce bytea;
