# Data Anchor API

This crate defines the API interfaces used when interacting with the Data Anchor indexer service.

## API

The indexer service exposes data via a `JSONRPC` server.
Here is an overview of the available methods:

### get_blobs

Retrieve a list of blobs for a given slot and blober pubkey. Returns an error if there was a
database or RPC failure, and None if the slot has not been completed yet. If the slot is
completed but no blobs were uploaded, an empty list will be returned.

#### Signature

```rust
async fn get_blobs(&self, blober: Pubkey, slot: u64) -> RpcResult<Option<Vec<Vec<u8>>>>;
```

### get_blobs_by_blober

Retrieve a list of blobs for a given blober pubkey and time range. Returns an error if there
was a database or RPC failure, and an empty list if no blobs were found.

#### Signature

```rust
async fn get_blobs_by_blober(&self, blober: BlobsByBlober) -> RpcResult<Vec<Vec<u8>>>;
```

Parameters:

```rust
pub struct TimeRange {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

pub struct BlobsByBlober {
    pub blober: Pubkey,
    #[serde(flatten)]
    pub time_range: TimeRange,
}
```

Which equals to the following options in JSON:

```json
{
  "blober": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK"
}
```

```json
{
  "blober": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
  "start": "2025-06-09T14:35:06.538958843Z"
}
```

```json
{
  "blober": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
  "start": "2025-06-09T14:30:06.538958843Z",
  "end": "2025-06-09T14:35:06.538958843Z"
}
```

### get_blobs_by_payer

Retrieve a list of blobs for a given payer pubkey, network ID, and time range. Returns an
error if there was a database or RPC failure, and an empty list if no blobs were found.

#### Signature

```rust
async fn get_blobs_by_payer(&self, payer: BlobsByPayer) -> RpcResult<Vec<Vec<u8>>>;
```

Parameters:

```rust
pub struct BlobsByPayer {
    pub payer: Pubkey,
    pub network_name: String,
    #[serde(flatten)]
    pub time_range: TimeRange,
}
```

Which equals to the following options in JSON:

```json
{
  "payer": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
  "network_name": "ping"
}
```

```json
{
  "payer": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
  "network_name": "ping",
  "start": "2025-06-09T14:35:06.538958843Z"
}
```

```json
{
  "payer": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
  "network_name": "ping",
  "start": "2025-06-09T14:30:06.538958843Z",
  "end": "2025-06-09T14:35:06.538958843Z"
}
```

### get_proof

Retrieve a proof for a given slot and blober pubkey. Returns an error if there was a
database or RPC failure, and None if the slot has not been completed yet.

#### Signature

```rust
async fn get_proof(&self, blober: Pubkey, slot: u64) -> RpcResult<Option<CompoundProof>>;
```

### get_proof_for_blob

Retrieve a compound proof that covers a particular blob. Returns an error if there was a
database or RPC failure, and None if the blob does not exist.

#### Signature

```rust
async fn get_proof_for_blob(&self, blob_address: Pubkey) -> RpcResult<Option<CompoundProof>>;
```

## Usage

To use the indexer API you can either use our pre-built CLI, Rust client, or if calling from any other
language, simply create and send `JSONRPC` requests.

### curl example

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    --data '{"jsonrpc":"2.0","id":1,"method":"get_blobs","params":["BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",385430344]}'
```
