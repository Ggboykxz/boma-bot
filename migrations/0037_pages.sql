-- REQ-037: pages, and the versions of their source.
--
-- A page is a document of blocks (see `boma_domain::page`). Sharing by link mints a 256-bit
-- URL-safe token; only its hash is stored. The public read is a lookup by token, never by id;
-- revoke flips the page back to private and drops the hash. Versions are immutable rows.
CREATE TABLE pages (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text REFERENCES bots (id) ON DELETE SET NULL,
    created_by text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name text NOT NULL,
    visibility text NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'link')),
    token_hash bytea,
    current_version_id text,
    client_nonce text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX pages_token_hash ON pages (token_hash) WHERE token_hash IS NOT NULL;
CREATE UNIQUE INDEX pages_space_creator_nonce
    ON pages (space_id, created_by, client_nonce)
    WHERE client_nonce IS NOT NULL;

CREATE TABLE page_versions (
    id text PRIMARY KEY,
    page_id text NOT NULL REFERENCES pages (id) ON DELETE CASCADE,
    version integer NOT NULL CHECK (version >= 1),
    source jsonb NOT NULL,
    compiled_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (page_id, version)
);

CREATE INDEX page_versions_page ON page_versions (page_id, version DESC);

ALTER TABLE pages ENABLE ROW LEVEL SECURITY;
ALTER TABLE pages FORCE ROW LEVEL SECURITY;
CREATE POLICY pages_tenant_isolation ON pages
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

ALTER TABLE page_versions ENABLE ROW LEVEL SECURITY;
ALTER TABLE page_versions FORCE ROW LEVEL SECURITY;
CREATE POLICY page_versions_tenant_isolation ON page_versions
    USING (
        EXISTS (
            SELECT 1 FROM pages p
            WHERE p.id = page_versions.page_id
              AND (p.space_id = boma.current_space_id() OR boma.is_system())
        )
    )
    WITH CHECK (
        EXISTS (
            SELECT 1 FROM pages p
            WHERE p.id = page_versions.page_id
              AND (p.space_id = boma.current_space_id() OR boma.is_system())
        )
    );
