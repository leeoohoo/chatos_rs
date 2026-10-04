ALTER TABLE plugin_catalog_sync_outbox
    ADD COLUMN IF NOT EXISTS processing_attempts INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS claim_token TEXT NULL,
    ADD COLUMN IF NOT EXISTS claim_until TIMESTAMPTZ NULL;

CREATE INDEX IF NOT EXISTS plugin_catalog_sync_outbox_claim_idx
    ON plugin_catalog_sync_outbox(pending, requested_at, claim_until, marketplace_id);
