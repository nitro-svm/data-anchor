CREATE TABLE blobers (
    blober_address BYTEA PRIMARY KEY NOT NULL,
    network_id SERIAL NOT NULL,
    payer_address BYTEA NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,

    FOREIGN KEY (network_id) REFERENCES networks(id),
    CONSTRAINT blober_address_32_bytes CHECK (octet_length(blober_address) = 32),
    CONSTRAINT payer_address_32_bytes CHECK (octet_length(payer_address) = 32)
);

CREATE INDEX blobers_payer_hash_idx ON blobers USING HASH (payer_address);