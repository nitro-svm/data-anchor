CREATE TABLE sync_status (
    last_synced_slot BIGINT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);