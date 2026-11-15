-- REQ-064 (improvement program): Web Push subscriptions and the per-bot policy.
-- Additive migration: no pre-existing table is touched.

CREATE TABLE notification_subscriptions (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    endpoint text NOT NULL,
    p256dh text NOT NULL,
    auth_secret text NOT NULL,
    user_agent text,
    fail_count integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT notification_subscriptions_keys_not_empty
        CHECK (length(btrim(endpoint)) > 0
               AND length(btrim(p256dh)) > 0
               AND length(btrim(auth_secret)) > 0)
);

CREATE UNIQUE INDEX notification_subscriptions_owner
    ON notification_subscriptions (space_id, user_id, endpoint);
CREATE INDEX notification_subscriptions_by_owner
    ON notification_subscriptions (space_id, user_id);

ALTER TABLE notification_subscriptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_subscriptions FORCE ROW LEVEL SECURITY;

CREATE POLICY notification_subscriptions_tenant ON notification_subscriptions
    FOR ALL
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

CREATE TABLE notification_policies (
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    notify_waiting boolean NOT NULL DEFAULT true,
    notify_completed boolean NOT NULL DEFAULT false,
    daily_cap integer NOT NULL DEFAULT 20 CHECK (daily_cap >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (space_id, user_id, bot_id)
);

ALTER TABLE notification_policies ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_policies FORCE ROW LEVEL SECURITY;

CREATE POLICY notification_policies_tenant ON notification_policies
    FOR ALL
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

CREATE TABLE notification_log (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    bot_id text NOT NULL REFERENCES bots (id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('waiting', 'completed')),
    sent_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX notification_log_by_day
    ON notification_log (space_id, user_id, bot_id, sent_at);

ALTER TABLE notification_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_log FORCE ROW LEVEL SECURITY;

CREATE POLICY notification_log_tenant ON notification_log
    FOR ALL
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
