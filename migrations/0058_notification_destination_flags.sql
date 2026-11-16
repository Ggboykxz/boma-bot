-- REQ-065 (improvement program): per-bot policy gains the destination kind flags; default off.
ALTER TABLE notification_policies
    ADD COLUMN destination_slack_webhook BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE notification_policies
    ADD COLUMN destination_teams_webhook BOOLEAN NOT NULL DEFAULT false;
