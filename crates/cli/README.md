# Data Anchor CLI

This crate provides a command line interface for interacting with the
`blober` program and the Data Anchor indexer. It can create and manage
`blober` accounts, upload blobs and query either the ledger or the indexer
for uploaded data.

```bash
# Basic invocation
data-anchor [GLOBAL-OPTIONS] <SUBCOMMAND> [COMMAND OPTIONS]
```

The examples below assume the binary is called `data-anchor` but the
application name will match the crate name when built locally.

## Global options

Some options are required for almost every command. They may also be set
via environment variables. Unless stated otherwise the arguments accept
string values.

| Flag/Env var | Description |
| -------------------------------------------------------- | --------------------------------------------------------------------- |
| `-p`, `--program-id` / `DATA_ANCHOR_PROGRAM_ID` | Address of the deployed `blober` program **(required)** |
| `-n`, `--namespace` / `DATA_ANCHOR_NAMESPACE` | Namespace used to derive the `blober` PDA |
| `-b`, `--blober-pda` / `DATA_ANCHOR_BLOBER_PDA` | Explicit `blober` PDA to use instead of deriving from the namespace |
| `-s`, `--payer` / `DATA_ANCHOR_PAYER` | Path to the keypair used to sign transactions |
| `-o`, `--output` / `DATA_ANCHOR_OUTPUT` | Output format: `text`, `json`, `json-pretty`, `csv` (default: `text`) |
| `-i`, `--indexer-url` / `DATA_ANCHOR_INDEXER_URL` | URL of the indexer service for read operations |
| `--indexer-api-token` / `DATA_ANCHOR_INDEXER_API_TOKEN` | Optional API token for the indexer |
| `-c`, `--config-file` / `DATA_ANCHOR_SOLANA_CONFIG_FILE` | Solana CLI config file (default: `~/.config/solana/cli/config.yml`) |

`--program-id` and either `--namespace` or `--blober-pda` must be
supplied. The payer defaults to the keypair configured in the Solana
config file.

## Subcommands

### `blober` (`br`)

Manage a `blober` PDA.

```
data-anchor -p <program> -n <namespace> blober initialize
```

Example commands:

```bash
# Create the PDA for a new namespace
data-anchor -p <PROGRAM_ID> -n <my-namespace> blober initialize

# Close the PDA and reclaim rent
data-anchor -p <PROGRAM_ID> -n <my-namespace> blober close
```

Commands:

- `initialize` (`i`) – create the PDA for the namespace.
- `close` (`c`) – close the PDA and reclaim rent.

### `blob` (`b`)

Upload, discard or retrieve blobs.

Commands:

- `upload` (`u`) – upload data. Use `-d, --data-path <PATH>` to read from a
  file or `--data <HEX>` for inline hex data. Without either, data is read from
  `stdin`.
- `discard` (`d`) – discard a blob using its PDA: `data-anchor b d <BLOB_PUBKEY>`.
- `fetch` (`f`) – fetch blob data from the ledger using transaction
  signatures: `data-anchor b f <SIG> [SIG ...]`.
- `get` (`g`) – retrieve all blobs finalized at a slot. Requires the slot
  number and optionally `-l, --lookback-slots <SLOTS>` to limit how far back to
  search.

Example commands:

```bash
# Upload data from a file
data-anchor -p <PROGRAM_ID> -n <my-namespace> blob upload -d ./data.txt

# Discard an unfinished upload
data-anchor -p <PROGRAM_ID> -n <my-namespace> blob discard -b <BLOB_PUBKEY>

# Fetch blob data from transaction signatures
data-anchor -p <PROGRAM_ID> -n <my-namespace> blob fetch <SIG1> <SIG2>

# Get all blobs finalized at a slot using a lookback window
data-anchor -p <PROGRAM_ID> -n <my-namespace> blob get 54321 -l 10
```

### `indexer` (`i`)

Query the indexer for blobs or proofs.

Commands:

- `blobs` (`b`) `<slot>` – return blobs finalized at the given slot.
- `blobs-for-blober` (`bl`) – requires `--blober <PUBKEY>` and optional
  `--start <RFC3339>`/`--end <RFC3339>` to restrict the time range.
- `blobs-for-payer` (`bp`) – requires `-y, --blob-payer <PUBKEY>` and
  `-m, --network-name <NAME>` with optional time range (`--start`/`--end`).
- `blobs-for-network` (`bn`) – requires `-m, --network-name <NAME>` and
  optional time range.
- `blobs-for-namespace` (`ns`) – requires `--namespace <NAME>` and optional
  `--payer-pubkey <PUBKEY>` plus the time range arguments.
- `proof-for-blob` (`pb`) – `--blob <PUBKEY>` returns the proof for the blob.
- `proof` (`p` or `proofs`) `<slot>` – get the compound proof for a slot.

Example commands:

```bash
# Get blobs for a slot via the indexer
data-anchor -p <PROGRAM_ID> -n <my-namespace> indexer blobs 12345

# Query blobs for a specific blober PDA in a time range
data-anchor indexer blobs-for-blober --blober <BLOBER_PUBKEY> \
    --start 2025-06-01T00:00:00Z --end 2025-06-30T00:00:00Z

# Query blobs paid for by an address on a network
data-anchor indexer blobs-for-payer -y <PAYER_PUBKEY> -m <NETWORK_NAME> \
    --start 2025-06-01T00:00:00Z --end 2025-06-30T00:00:00Z

# Query all blobs uploaded on a network in a time range
data-anchor indexer blobs-for-network -m <NETWORK_NAME> \
    --start 2025-06-01T00:00:00Z --end 2025-06-30T00:00:00Z

# Get blobs for a namespace, optionally filtering by payer
data-anchor indexer blobs-for-namespace --namespace <NAME> \
    --payer-pubkey <PAYER_PUBKEY> --start 2025-06-01T00:00:00Z \
    --end 2025-06-30T00:00:00Z

# Fetch the proof for a blob
data-anchor indexer proof-for-blob --blob <BLOB_PUBKEY>

# Get the compound proof for a slot
data-anchor -p <PROGRAM_ID> -n <my-namespace> indexer proof 12345
```

Time range arguments expect RFC3339 timestamps (e.g. `2025-06-01T00:00:00Z`).

### `benchmark` (`m`)

Tools for generating data and measuring performance.

Commands:

- `generate` (`g`) `<DATA_PATH>` – create random data files. Options:
  - `-s, --size <BYTES>` size of each file (default: 1000).
  - `-c, --count <N>` number of files to generate (default: 100).
  - `-r, --random-length` generate files with random lengths up to `size`.
- `measure` (`m`) `<DATA_PATH>` – upload all files in the directory and measure
  throughput. Options:
  - `-t, --timeout <SECS>` upload timeout per blob (default: 60).
  - `-c, --concurrency <N>` number of concurrent uploads (default: 600).
  - `-p, --priority <LEVEL>` transaction priority (`min`, `low`, `medium`, `high`, `very-high`).
- `automate` (`a`) – run a set of benchmark scenarios.
  - `-d, --data-path <PATH>` directory used for generated data.
  - `-r, --running-csv <FILE>` write intermediate results to this CSV file.

Example commands:

```bash
# Generate 100 files of 1KiB
data-anchor benchmark generate ./bench-data -s 1024 -c 100

# Measure upload throughput with 600 concurrent uploads
data-anchor benchmark measure ./bench-data -c 600 -t 60 -p medium

# Run a full benchmark scenario
data-anchor benchmark automate -d ./bench-data -r results.csv
```

The `help` command or `--help` flag on any subcommand shows these options at
runtime.
