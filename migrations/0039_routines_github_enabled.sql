-- REQ-050: repository events from the bot's webhook wake the routines that opt in.
--
-- `github_enabled` is off by default; the bot's webhook wakes the bot itself for every call, and
-- a repository-shaped payload additionally wakes the opted-in routines of the bot, each with its
-- own run, exactly as a due routine fires.
ALTER TABLE routines
    ADD COLUMN github_enabled boolean NOT NULL DEFAULT false;
