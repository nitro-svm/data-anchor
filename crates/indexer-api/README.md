# Data Anchor API

This crate defines the API interfaces used when interacting with the Data Anchor indexer service.

## API

The indexer service exposes data via a `JSONRPC` server. Here is an overview of the available methods:

### get_blobs

Retrieve a list of blobs for a given slot and blober pubkey. Returns an error if there was a database
or RPC failure, and None if the slot has not been completed yet. If the slot is completed but no blobs
were uploaded, an empty list will be returned.

#### Signature

```rust
async fn get_blobs(&self, blober: Pubkey, slot: u64) -> RpcResult<Option<Vec<Vec<u8>>>>;
```

#### curl examples

Array parameters:

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs",
  "params": ["BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK", 385430344]
}
JSON
```

Object parameters:

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs",
  "params": {
    "blober": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
    "slot": 385430344
  }
}
JSON
```

### get_blobs_by_blober

Retrieve a list of blobs for a given blober pubkey and optional time range. Returns an error if there
was a database or RPC failure, and an empty list if no blobs were found.

#### Signature

```rust
async fn get_blobs_by_blober(&self, blober: Pubkey, time_range: Option<TimeRange>) -> RpcResult<Vec<Vec<u8>>>;
```

The `TimeRange` structure is defined as:

```rust
pub struct TimeRange {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}
```

#### curl examples

Without optional `time_range` (array):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_blober",
  "params": ["BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK"]
}
JSON
```

Without optional `time_range` (object):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_blober",
  "params": {
    "blober": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK"
  }
}
JSON
```

With a `time_range` (array):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_blober",
  "params": ["BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK", {"start": "2025-06-09T14:30:06Z", "end": "2025-06-09T14:35:06Z"}]
}
JSON
```

With a `time_range` (object):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_blober",
  "params": {
    "blober": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
    "time_range": {
      "start": "2025-06-09T14:30:06Z",
      "end": "2025-06-09T14:35:06Z"
    }
  }
}
JSON
```

### get_blobs_by_payer

Retrieve a list of blobs for a given payer pubkey, network name and optional time range. Returns an error
if there was a database or RPC failure, and an empty list if no blobs were found.

#### Signature

```rust
async fn get_blobs_by_payer(&self, payer: Pubkey, network_name: String, time_range: Option<TimeRange>) -> RpcResult<Vec<Vec<u8>>>;
```

#### curl examples

Without optional `time_range` (array):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_payer",
  "params": ["BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK", "ping"]
}
JSON
```

Without optional `time_range` (object):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_payer",
  "params": {
    "payer": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
    "network_name": "ping"
  }
}
JSON
```

With a `time_range` (array):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_payer",
  "params": [
    "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
    "ping",
    {"start": "2025-06-09T14:30:06Z", "end": "2025-06-09T14:35:06Z"}
  ]
}
JSON
```

With a `time_range` (object):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_payer",
  "params": {
    "payer": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
    "network_name": "ping",
    "time_range": {
      "start": "2025-06-09T14:30:06Z",
      "end": "2025-06-09T14:35:06Z"
    }
  }
}
JSON
```

### get_blobs_by_network

Retrieve a list of blobs for a given network name and time range. Returns an error if there was a database
or RPC failure, and an empty list if no blobs were found.

#### Signature

```rust
async fn get_blobs_by_network(&self, network_name: String, time_range: TimeRange) -> RpcResult<Vec<Vec<u8>>>;
```

#### curl examples

Without `start` and `end` in `time_range` (array):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_network",
  "params": ["ping", {}]
}
JSON
```

Without `start` and `end` in `time_range` (object):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_network",
  "params": {
    "network_name": "ping",
    "time_range": {}
  }
}
JSON
```

With `start` and `end` provided (array):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_network",
  "params": ["ping", {"start": "2025-06-09T14:30:06Z", "end": "2025-06-09T14:35:06Z"}]
}
JSON
```

With `start` and `end` provided (object):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_network",
  "params": {
    "network_name": "ping",
    "time_range": {
      "start": "2025-06-09T14:30:06Z",
      "end": "2025-06-09T14:35:06Z"
    }
  }
}
JSON
```

### get_blobs_by_namespace

Retrieve a list of blobs for a given namespace and time range. A payer may be supplied to filter results.
Returns an error if there was a database or RPC failure, and an empty list if no blobs were found.

#### Signature

```rust
async fn get_blobs_by_namespace_for_payer(&self, namespace: String, payer: Option<Pubkey>, time_range: TimeRange) -> RpcResult<Vec<Vec<u8>>>;
```

#### curl examples

Without optional `payer` (array):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_namespace",
  "params": ["my_namespace", null, {}]
}
JSON
```

Without optional `payer` (object):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_namespace",
  "params": {
    "namespace": "my_namespace",
    "payer": null,
    "time_range": {}
  }
}
JSON
```

With `payer` and a `time_range` (array):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_namespace",
  "params": ["my_namespace", "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK", {"start": "2025-06-09T14:30:06Z", "end": "2025-06-09T14:35:06Z"}]
}
JSON
```

With `payer` and a `time_range` (object):

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_blobs_by_namespace",
  "params": {
    "namespace": "my_namespace",
    "payer": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
    "time_range": {
      "start": "2025-06-09T14:30:06Z",
      "end": "2025-06-09T14:35:06Z"
    }
  }
}
JSON
```

### get_payers_by_network

Retrieve a list of payers for a given network name. Returns an error if there was a database or RPC
failure, and an empty list if no payers were found.

#### Signature

```rust
async fn get_payers_by_network(&self, network_name: String) -> RpcResult<Vec<Pubkey>>;
```

#### curl examples

Array parameters:

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_payers_by_network",
  "params": ["ping"]
}
JSON
```

Object parameters:

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_payers_by_network",
  "params": {"network_name": "ping"}
}
JSON
```

### get_proof

Retrieve a proof for a given slot and blober pubkey. Returns an error if there was a database or RPC
failure, and `None` if the slot has not been completed yet.

#### Signature

```rust
async fn get_proof(&self, blober: Pubkey, slot: u64) -> RpcResult<Option<CompoundProof>>;
```

#### curl examples

Array parameters:

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_proof",
  "params": ["BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK", 385430344]
}
JSON
```

Object parameters:

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_proof",
  "params": {
    "blober": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK",
    "slot": 385430344
  }
}
JSON
```

### get_proof_for_blob

Retrieve a compound proof that covers a particular blob. Returns an error if there was a database or RPC
failure, and `None` if the blob does not exist.

#### Signature

```rust
async fn get_proof_for_blob(&self, blob_address: Pubkey) -> RpcResult<Option<CompoundProof>>;
```

#### curl examples

Array parameters:

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_proof_for_blob",
  "params": ["BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK"]
}
JSON
```

Object parameters:

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "get_proof_for_blob",
  "params": {"blob_address": "BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK"}
}
JSON
```

### subscribe_blob_finalization

Listen to blob finalization events from specified blobers. This will return a stream of slots and blober
PDA addresses that have finalized blobs.

#### Signature

```rust
async fn subscribe_blob_finalization(&self, blobers: HashSet<Pubkey>) -> SubscriptionResult;
```

#### curl examples

Array parameters:

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "subscribe_blob_finalization",
  "params": [["BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK"]]
}
JSON
```

Object parameters:

```bash
curl "<INDEXER-URL>" -XPOST \
    -H 'Content-Type: application/json' \
    -H 'x-api-key: <API_KEY>' \
    --data @- <<'JSON'
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "subscribe_blob_finalization",
  "params": {"blobers": ["BAugq2PZwXBCw72YTRe93kgw3X6ghB3HfF7eSYBDhTsK"]}
}
JSON
```
