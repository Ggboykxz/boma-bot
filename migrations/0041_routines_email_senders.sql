-- REQ-047: a routine may admit its own senders.
--
-- `email_senders` lists the exact addresses the routine trusts to wake it. Overlapping an
-- allowlist is deliberate: an address admitted by the bot's rule starts work as mail; an address
-- admitted only by a routine fires that routine instead.
ALTER TABLE routines
    ADD COLUMN email_senders text[] NOT NULL DEFAULT '{}';
