-- REQ-017: the pending tool call vocabulary, enforced at the database layer.
--
-- 0021 introduced the relation; `status` could hold any string. This migration pins the domain
-- vocabulary ('pending', 'applied', 'refused'), which the Rust enum round-trips byte-exactly.
ALTER TABLE pending_tool_calls
    ADD CONSTRAINT pending_tool_calls_status_check
    CHECK (status IN ('pending', 'applied', 'refused'));
