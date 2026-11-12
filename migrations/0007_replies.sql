-- REQ-008: replies and quotes ride the message row.
--
-- replyToMessageId names the message being answered (same thread); replyQuote carries the short
-- quote of its text the client renders. Both are optional: a message that is not a reply simply
-- has NULL for both.

ALTER TABLE messages ADD COLUMN reply_to_message_id text REFERENCES messages (id) ON DELETE SET NULL;
ALTER TABLE messages ADD COLUMN reply_quote text;
