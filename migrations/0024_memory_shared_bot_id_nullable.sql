-- REQ-022: a shared memory document must be writable.
--
-- 0013 encoded "shared" as the empty string, with `bot_id NOT NULL REFERENCES bots (id)`.
-- A foreign key enforces every non-NULL value: the empty string is not a bot, so no shared
-- document could ever be written. This migration moves the shared marker to NULL, which the
-- foreign key ignores, and replaces the CHECK with the equivalent NULL-shaped one. Existing
-- rows are untouched: the bug made shared rows impossible, so only bot rows exist.
ALTER TABLE memory_documents ALTER COLUMN bot_id DROP NOT NULL;
ALTER TABLE memory_documents ALTER COLUMN bot_id DROP DEFAULT;
ALTER TABLE memory_documents DROP CONSTRAINT IF EXISTS memory_documents_check;
ALTER TABLE memory_documents ALTER COLUMN bot_id SET DEFAULT NULL;

ALTER TABLE memory_documents
    ADD CONSTRAINT memory_documents_scope_bot_id_check
    CHECK ((scope = 'bot' AND bot_id IS NOT NULL AND bot_id <> '') OR (scope = 'shared' AND bot_id IS NULL));

-- The upsert of the store needs an arbiter for shared documents: the table's UNIQUE on
-- (space_id, scope, bot_id, path) cannot discriminate NULLs. A partial index does.
CREATE UNIQUE INDEX IF NOT EXISTS memory_documents_shared_key
    ON memory_documents (space_id, path)
    WHERE scope = 'shared' AND bot_id IS NULL;
