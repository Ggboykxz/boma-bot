-- REQ-040: normalize stored addresses to lowercase.
--
-- Emails are compared byte for byte across the recovery flow (find by address) and the
-- invitation flow (caller address == invited address). New rows lowercase at
-- `Email::new`; this backfills the ones stored before the rule existed so a mixed-case
-- account can receive its recovery link.
UPDATE users SET email = lower(email);
