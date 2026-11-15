-- REQ-045 (+ D-05): make the audit trail consistent with every writer.
--
-- Two latent defects, both introduced with the table in 0030 and never triggered by a test:
--   1. `kind` had a closed CHECK admitting only 'mcp.private_call', while the stores write
--      'mail.dropped' and 'messaging.private_endpoint' (and M9 adds the security set below).
--   2. `space_id` was NOT NULL with ON DELETE CASCADE, so deleting a space deleted its audit
--      rows, and there was nowhere to put deployment-scoped events (sign-in failures, CSP
--      violations) that have no space.
-- The audit trail is append-only (never rewritten): this migration only widens what the
-- columns accept. The write set is a closed set again — three kinds today, the security set
-- from M9 on top — so the CHECK keeps meaning something.
ALTER TABLE audit_events DROP CONSTRAINT audit_events_kind_check;

ALTER TABLE audit_events
    ADD CONSTRAINT audit_events_kind_check
    CHECK (kind IN (
        'mcp.private_call', 'mail.dropped', 'messaging.private_endpoint',
        'security.sign_in_failed', 'security.csp_violation',
        'account.deleted', 'export.downloaded', 'import.completed',
        'scim.provisioned', 'scim.deprovisioned',
        'admin.member_role_changed', 'admin.member_removed',
        'admin.grant_created', 'admin.grant_revoked',
        'admin.plugin_reviewed'
    ));

-- Deployment-scoped rows have no space, and the spec's rule is that deleting an account
-- leaves its audit unreadable in that space but not erased.
ALTER TABLE audit_events ALTER COLUMN space_id DROP NOT NULL;
ALTER TABLE audit_events DROP CONSTRAINT audit_events_space_id_fkey;
ALTER TABLE audit_events
    ADD CONSTRAINT audit_events_space_id_fkey
    FOREIGN KEY (space_id) REFERENCES spaces (id) ON DELETE SET NULL;

DROP POLICY audit_events_tenant_isolation ON audit_events;
-- Members read only their own space's rows. Deployment-scoped rows (space_id IS NULL) are
-- invisible to every member; they exist for operators querying Postgres and for the
-- system-scoped export path.
CREATE POLICY audit_events_tenant_isolation ON audit_events
    USING ((space_id IS NOT NULL AND (space_id = boma.current_space_id() OR boma.is_system()))
        OR boma.is_system())
    WITH CHECK (
        (space_id IS NULL AND boma.is_system())
        OR (space_id IS NOT NULL AND space_id = boma.current_space_id())
        OR boma.is_system()
    );
