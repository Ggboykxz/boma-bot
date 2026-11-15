-- REQ-040: new security audit kinds for the password recovery flow.
-- The 0045 set stays valid; this widens it with the two recovery kinds.
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
        'admin.plugin_reviewed',
        'security.password_recovery_issued', 'security.password_recovery_completed'
    ));
