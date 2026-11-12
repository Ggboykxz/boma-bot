-- REQ-029: a member can take exclusive control of the bot's screen, and hand it back.
--
-- `takeover_by` names the member holding control; everything the bot would drive through the
-- computer refuses while one is set. `takeover_at` is when the hold began. Both clear on release.

ALTER TABLE computers
    ADD COLUMN takeover_by text,
    ADD COLUMN takeover_at timestamptz;
