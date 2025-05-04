CREATE TABLE submissions (
    blober_address BYTEA NOT NULL, -- unique per network and user combo
    submission_time TIMESTAMPTZ NOT NULL,
    slot BIGINT NOT NULL,
    bankhash BYTEA NOT NULL,
    proof BYTEA NOT NULL,
    verified BOOLEAN,
    wallet_address BYTEA NOT NULL, -- owner or payer of the blober PDA
    transaction_signatures BYTEA[],
    network_id SERIAL NOT NULL,

    PRIMARY KEY (blober_address, submission_time),
    FOREIGN KEY (network_id) REFERENCES networks(id)
);

CREATE INDEX submissions_wallet_hash_idx ON submissions USING HASH (wallet_address);
CREATE INDEX submissions_time_btree_idx ON submissions (network_id, submission_time);