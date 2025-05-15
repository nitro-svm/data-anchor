CREATE TABLE proofs (
    blober_address BYTEA NOT NULL,
    slot BIGINT NOT NULL,
    proof JSONB NOT NULL,

    PRIMARY KEY (blober_address, slot),
    FOREIGN KEY (blober_address) REFERENCES blobers(blober_address)
);