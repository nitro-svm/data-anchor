# Nitro DA Client

This crate is a Rust client which handles interactions with the [Nitro Blober](https://crates.io/nitro-da-blober) on-chain program
and with the Nitro DA indexer service in an optimized way.

## Usage

All you need to do to get started with the client is add it to your project dependencies:

```bash
cargo add nitro-da-client
```

### Connecting

To start uploading and reading data, pass the configs into the client like the following:

```rust
let blober_client = BloberClient::builder()
    .payer(payer)
    .program_id(program_id)
    .indexer_from_url(&indexer_url)
    .await?
    .build_with_config(config)
    .await?;
```

- The `payer` is a `Arc<Keypair>` - a solana keypair you want to use with the client
- The `program_id` is the address of the blober on-chain program you want to interact with
- The `indexer_url` is an optional parameter to provide if you are using our indexer service
- The `config` is a `solana_cli_config::Config` object used to determine RPC details with which to send the transactions

### Uploading data

Uploading data once you have a blober client is as simple as:

```rust
let transaction_outcomes = blober_client.upload_blob(data, fee, blober_id, timeout).await?;
```

- The `data` is a slice of bytes (`&[u8]`) to upload
- The `fee` is a fee strategy for how much you want to send as the priority fee
- The `blober_id` is the blober PDA (namespace) you want to upload to
- The `timeout` is an optional parameter which specifies how long to wait before discarding a started data upload

> The transaction outcomes is a vector of `TransactionOutcome` enum structs which contain the success state (successfull, failed or unknown) and
> in case of success the transaction signature and slot at which the transaction landed.

### Querying data

To later retrieve the data that was uploaded, you can either do it from the ledger directly:

```rust
let blob = blober_client
    .get_ledger_blobs_from_signatures(blober, signatures)
    .await?;
```

Where the `signatures` are the list of signatures you got by sending the upload request.

```rust
let blobs = blober_client.get_ledger_blobs(slot, blober, lookback_slots).await?;
```

Where the `slot` is the slot at which the upload was finalized and `lookback_slots` is an optional parameter to limit how many slots before the `slot`
to fetch in the past.

Or from the indexer service with:

```rust
let blobs = blober_client.get_blobs(slot, blober).await?;
```

And getting the indexer proofs (these prove that the indexer is sending you valid data):

```rust
let proof = blober_client.get_slot_proof(slot, blober).await?;
```

For more details, check out the [docs](https://docs.rs/nitro-da-client).
