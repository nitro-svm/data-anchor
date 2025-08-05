# Data Anchor Client

This crate is a Rust client which handles interactions with the [Nitro Blober](https://crates.io/data-anchor-blober) on-chain program
and with the Data Anchor indexer service in an optimized way.

## Usage

All you need to do to get started with the client is add it to your project dependencies:

```bash
cargo add data-anchor-client
```

### Connecting

To start uploading and reading data, pass the configs into the client like the following:

```rust
let data_anchor_client = DataAnchorClient::builder()
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

### Builder options

The builder exposes a few additional helpers. A common pattern is to use an
indexer together with the [Helius](https://docs.helius.xyz/) fee estimator:

```rust
let client = DataAnchorClient::builder()
    .payer(payer)
    .program_id(program_id)
    .indexer_from_url("https://indexer.example.com", None)
    .await?
    .with_helius_fee_estimate()
    .build_with_config(Config::default())
    .await?;
```

The `with_helius_fee_estimate` flag enables querying the Helius API for a better
priority fee estimate when sending transactions.

### Uploading data

Uploading data once you have a blober client is as simple as:

```rust
let transaction_outcomes = data_anchor_client.upload_blob(data, fee, blober_id, timeout).await?;
```

- The `data` is a slice of bytes (`&[u8]`) to upload
- The `fee` is a fee strategy for how much you want to send as the priority fee
- The `blober_id` is the blober PDA (namespace) you want to upload to
- The `timeout` is an optional parameter which specifies how long to wait before discarding a started data upload

> The transaction outcomes is a vector of `TransactionOutcome` enum structs which contain the success state (successfull, failed or unknown) and
> in case of success the transaction signature and slot at which the transaction landed.

### Estimating fees

Before uploading a blob you can estimate the expected cost using `estimate_fees`:

```rust
let priority = Priority::default();
let expected_fees = data_anchor_client
    .estimate_fees(data.len(), blober_pubkey, priority)
    .await?;
println!("Estimated lamports: {}", expected_fees.total_fee());
```

### Querying data

To later retrieve the data that was uploaded, you can either do it from the ledger directly:

```rust
let blob = data_anchor_client
    .get_ledger_blobs_from_signatures(blober, signatures)
    .await?;
```

Where the `signatures` are the list of signatures you got by sending the upload request.

```rust
let blobs = data_anchor_client.get_ledger_blobs(slot, blober, lookback_slots).await?;
```

Where the `slot` is the slot at which the upload was finalized and `lookback_slots` is an optional parameter to limit how many slots before the `slot`
to fetch in the past.

Or from the indexer service with:

```rust
let blobs = data_anchor_client.get_blobs(slot, blober).await?;
```

And getting the indexer proofs (these prove that the indexer is sending you valid data):

```rust
let proof = data_anchor_client.get_slot_proof(slot, blober).await?;
```

### Complete workflow

The following snippet demonstrates creating a blober, uploading data and
fetching it back from the ledger:

```rust
use std::{sync::Arc, time::Duration};
use data_anchor_client::{DataAnchorClient, FeeStrategy};
use solana_cli_config::Config;
use solana_keypair::Keypair;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let payer = Arc::new(Keypair::new());
    let client = DataAnchorClient::builder()
        .payer(payer.clone())
        .program_id(data_anchor_blober::id())
        .build_with_config(Config::default())
        .await?;

    let ns = "example";
    client.initialize_blober(FeeStrategy::default(), ns, None).await?;

    let blob = b"hello world";
    let outcomes = client
        .upload_blob(blob, FeeStrategy::default(), ns, Some(Duration::from_secs(10)))
        .await?;

    let sigs = outcomes.iter().map(|o| o.signature).collect::<Vec<_>>();
    let recovered = client
        .get_ledger_blobs_from_signatures(ns.into(), sigs)
        .await?;
    assert_eq!(blob.to_vec(), recovered);
    Ok(())
}
```

For more details, check out the [docs](https://docs.rs/data-anchor-client).

## API reference

The following table contains small examples for the public methods provided by
`DataAnchorClient`. Every function is asynchronous and returns a
`DataAnchorClientResult`.

### Blober management

```rust
let ns = "example";
client.initialize_blober(FeeStrategy::default(), ns, None).await?;
client.close_blober(FeeStrategy::default(), ns, None).await?;
```

### Upload helpers

```rust
let blob_pubkey = Pubkey::new_unique();
client.upload_blob(data, FeeStrategy::default(), ns, None).await?;
client.discard_blob(FeeStrategy::default(), blob_pubkey, ns, None).await?;
client.estimate_fees(data.len(), blob_pubkey, Priority::default()).await?;
```

### Ledger queries

```rust
client.get_ledger_blobs_from_signatures(ns.into(), signatures).await?;
client.get_ledger_blobs(slot, ns.into(), None).await?;
client.get_blob_messages(slot, ns.into()).await?;
```

### Indexer queries

```rust
client.get_blobs(slot, ns.into()).await?;
client.get_blobs_by_blober(ns.into(), None).await?;
client.get_blobs_by_payer(payer_pubkey, network_name.clone(), None).await?;
client.get_blobs_by_network(network_name.clone(), time_range).await?;
client.get_blobs_by_namespace_for_payer(ns.into(), Some(payer_pubkey), time_range).await?;
client.get_proof(slot, ns.into()).await?;
client.get_proof_for_blob(blob_pubkey).await?;
```

### Builder helpers

```rust
let client = DataAnchorClient::builder()
    .payer(payer)
    .program_id(program_id)
    .indexer_from_url(indexer_url, None)
    .await?
    .with_helius_fee_estimate()
    .build_with_config(Config::default())
    .await?;
```
