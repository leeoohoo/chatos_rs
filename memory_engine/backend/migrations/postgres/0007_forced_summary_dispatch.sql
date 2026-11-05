ALTER TABLE engine_threads
    ADD COLUMN IF NOT EXISTS summary_dispatch_force_version BIGINT NOT NULL DEFAULT 0;
