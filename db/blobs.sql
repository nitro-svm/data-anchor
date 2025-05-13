CREATE TABLE blobs (
    blob_address BYTEA PRIMARY KEY NOT NULL,
    blober_address BYTEA NOT NULL,
    data BYTEA NOT NULL,
    transaction_signatures BYTEA[],
    created_at TIMESTAMPTZ NOT NULL,
    slot BIGINT NOT NULL,
    verified BOOLEAN,

    FOREIGN KEY (blober_address) REFERENCES blobers(blober_address),
    CONSTRAINT blob_address_32_bytes CHECK (octet_length(blob_address) = 32)
);

CREATE INDEX blobs_blober_hash_idx ON blobs USING HASH (blober_address);
CREATE INDEX blobs_create_time_btree_idx ON blobs (created_at);
CREATE INDEX blobs_update_time_btree_idx ON blobs (updated_at);