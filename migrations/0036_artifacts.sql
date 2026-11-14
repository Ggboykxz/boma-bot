-- REQ-036: artifacts, one row per (space, rootArtifactId, version).
--
-- An artifact is bytes plus provenance: who made it (user, bot, group, run), what it claims to
-- be (declared), what the bytes proved (sniffed), and where the bytes live (content-addressed,
-- out of band: storage_key = hash). Versions form a chain rooted at the first artifact of the
-- family; (space_id, root_artifact_id, version) is unique so concurrent uploads of the same
-- family cannot mint duplicate versions.
CREATE TABLE artifacts (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    bot_id text REFERENCES bots (id) ON DELETE SET NULL,
    group_id text,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    run_id text REFERENCES runs (id) ON DELETE SET NULL,
    name text NOT NULL,
    description text NOT NULL DEFAULT '',
    declared_mime text NOT NULL,
    mime_type text NOT NULL,
    size bigint NOT NULL CHECK (size >= 0),
    hash text NOT NULL,
    storage_key text NOT NULL,
    root_artifact_id text NOT NULL,
    version integer NOT NULL CHECK (version >= 1),
    standalone boolean NOT NULL DEFAULT true,
    client_nonce text,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX artifacts_family_version ON artifacts (space_id, root_artifact_id, version);
CREATE UNIQUE INDEX artifacts_space_user_nonce
    ON artifacts (space_id, user_id, client_nonce)
    WHERE client_nonce IS NOT NULL;
CREATE INDEX artifacts_space_root ON artifacts (space_id, root_artifact_id, version DESC);
CREATE INDEX artifacts_storage_key ON artifacts (storage_key);

ALTER TABLE artifacts ENABLE ROW LEVEL SECURITY;
ALTER TABLE artifacts FORCE ROW LEVEL SECURITY;
CREATE POLICY artifacts_tenant_isolation ON artifacts
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
