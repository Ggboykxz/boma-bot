-- REQ-018 + REQ-051: the money spine.
--
-- A space caps monthly model spend (REQ-018); every approval of a purchase is recorded here,
-- never before it. Subscriptions carry the plan of a paid space (REQ-051); a deployment that
-- does not offer billing never writes this table, and the free plan is the default.

CREATE TABLE space_spending_limits (
    space_id text PRIMARY KEY REFERENCES spaces (id) ON DELETE CASCADE,
    enabled boolean NOT NULL DEFAULT false,
    currency text NOT NULL DEFAULT 'USD' CHECK (char_length(currency) = 3),
    cap_minor_units bigint NOT NULL DEFAULT 0 CHECK (cap_minor_units >= 0),
    updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE space_spending_limits ENABLE ROW LEVEL SECURITY;
ALTER TABLE space_spending_limits FORCE ROW LEVEL SECURITY;
CREATE POLICY space_spending_limits_tenant_isolation ON space_spending_limits
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

CREATE TABLE purchase_payments (
    id text PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    card_id text REFERENCES cards (id) ON DELETE SET NULL,
    item text NOT NULL,
    amount_minor_units bigint NOT NULL CHECK (amount_minor_units >= 0),
    currency text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX purchase_payments_month ON purchase_payments (space_id, created_at DESC);

ALTER TABLE purchase_payments ENABLE ROW LEVEL SECURITY;
ALTER TABLE purchase_payments FORCE ROW LEVEL SECURITY;
CREATE POLICY purchase_payments_tenant_isolation ON purchase_payments
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());

-- One row per space: the plan a paid space is on, where the payment provider left it.
-- `provider_ref` is the provider's checkout/subscription reference; `credits_minor_units`
-- is what a checkout seeds. A free space has no row and is `free`.
CREATE TABLE subscriptions (
    space_id text PRIMARY KEY REFERENCES spaces (id) ON DELETE CASCADE,
    plan text NOT NULL CHECK (plan IN ('free', 'pro')),
    status text NOT NULL CHECK (status IN ('active', 'canceled', 'past_due')),
    provider_ref text,
    credits_minor_units bigint NOT NULL DEFAULT 0,
    current_period_end timestamptz,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE subscriptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE subscriptions FORCE ROW LEVEL SECURITY;
CREATE POLICY subscriptions_tenant_isolation ON subscriptions
    USING (space_id = boma.current_space_id() OR boma.is_system())
    WITH CHECK (space_id = boma.current_space_id() OR boma.is_system());
