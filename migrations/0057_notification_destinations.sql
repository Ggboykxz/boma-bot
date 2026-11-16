-- REQ-065 (improvement program): space-scoped outgoing notification destinations, sealed like
-- bot secrets; the webhook URL is never returned by any other path.
CREATE TABLE notification_destinations (
    id            text PRIMARY KEY,
    space_id      text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    kind          text NOT NULL CHECK (kind IN ('slack_webhook', 'teams_webhook')),
    label         text NOT NULL,
    ciphertext    bytea NOT NULL,
    nonce         bytea NOT NULL,
    created_by    text NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT notification_destinations_label_not_empty
        CHECK (length(btrim(label)) > 0)
);

CREATE INDEX notification_destinations_by_space_kind
    ON notification_destinations (space_id, kind);

ALTER TABLE notification_destinations ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_destinations FORCE ROW LEVEL SECURITY;

CREATE POLICY notification_destinations_tenant ON notification_destinations
    FOR ALL
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
