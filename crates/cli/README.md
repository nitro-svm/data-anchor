# Data Anchor CLI

This crate allows users to interact with the `blober` program in order to register new `blober` PDA accounts, upload
blobs and retrieve data from the indexer.

The mandatory parameters for this tool are the address of the `blober` program (`--program-id` or `-p`) and the
namespace of the `blober` PDA to interact with (`--namespace` or `-n`).

Optional global parameters are:

- payer (`-s`, `--payer`): The payer account to use for transactions
- output (`-o`, `--output`): The output format to use [default: text] [possible values: text, json, json-pretty, csv] -
  falls back to text output if a format is not supported for a command
- indexer url (`-i`, `--indexer-url`): The URL of the indexer to use
- config file (`-c`, `--config-file`): The path to the Solana [`Config`] file [default: ~/.config/solana/cli/config.yml]

The CLI has a `help` command (available for each subcommand as well) which you can use to check the usage.
Here is a high level overview of the available commands.

## `data-anchor blober` or `data-anchor br`

This subcommand allows you to initialize and close `blober` PDA accounts with the followign commands:

- `data-anchor br initialize` or `data-anchor br i` to initialize a new `blober` PDA account

```bash
# Initialize a new namespace with the seed "some-namespace"
data-anchor -p "<program-id-of-blober>" -n "some-namespace" br i
```

- `data-anchor br close` or `data-anchor br c` to close an existing `blober` PDA account

```bash
# Close a existing namespace with the seed "some-namespace"
data-anchor -p "<program-id-of-blober>" -n "some-namespace" br c
```

## `data-anchor blob` or `data-anchor b`

This subcommand allows you to upload and discard blobs of data with the following commands:

- `data-anchor b upload` or `data-anchor b u` to upload data read from a file at the given `--data-path` (or `-d`)

```bash
# Upload a blob of data read from a file
data-anchor -p "<program-id-of-blober>" -n "some-namespace" b u -d "./data.txt"
```

- `data-anchor b discard` or `data-anchor b d` to discard a blob upload with the given `blob` PDA (`--blob` or `-b`)
  before it is finalized

```bash
# Discard a failed or stale blob upload with the given PDA ID
data-anchor -p "<program-id-of-blober>" -n "some-namespace" b d -b "<some-blob-pda-id>"
```

- `data-anchor b fetch` or `data-anchor b f` to fetch blob data from the ledger given a list of transaction signatures

```bash
# Fetch blob data from the ledger given a list of transactions
data-anchor -p "<program-id-of-blober>" -n "some-namespace" b f "<some-tx-signature>" "<some-other-tx-signature>"
```

- `data-anchor b get` or `data-anchor b g` to retrieve all blobs finalized at a given slot

```bash
# Get all blobs which were finalized at the given slot
data-anchor -p "<program-id-of-blober>" -n "some-namespace" b g 54321
```

## `data-anchor indexer` or `data-anchor i`

This subcommand allows you to fetch blobs and proofs for a given `blober` PDA and `slot` using the following commands:

- `data-anchor i blobs` or `data-anchor i b` to fetch blobs

```bash
# Retrieve all blobs finalized at the given slot
data-anchor -p "<program-id-of-blober>" -n "some-namespace" i b 54321
```

- `data-anchor i proofs` or `data-anchor i p` to fetch proofs

```bash
# Fetch a compound proof (inclusion or completness) for a given slot
data-anchor -p "<program-id-of-blober>" -n "some-namespace" i p 54321
```

## `data-anchor measure` or `data-anchor m`

This subcommand allow you to generate data files and upload them to capture some measurements using the following
commands:

- `data-anchor m generate` or `data-anchor m g` to generate data files to a given directory
- `data-anchor m measure` or `data-anchor m m` to upload data from a given directory and capture measurements
- `data-anchor m automate` or `data-anchor m a` to run automated benchmarks with combinations of different data
  generation and measurement scenarios
