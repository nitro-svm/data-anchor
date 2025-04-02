# Nitro Da CLI

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

## `nitro-da-cli blober` or `nitro-da-cli br`

This subcommand allows you to initialize and close `blober` PDA accounts with the followign commands:

- `nitro-da-cli br initialize` or `nitro-da-cli br i` to initialize a new `blober` PDA account

```bash
# Initialize a new namespace with the seed "some-namespace"
nitro-da-cli -p "<program-id-of-blober>" -n "some-namespace" br i
```

- `nitro-da-cli br close` or `nitro-da-cli br c` to close an existing `blober` PDA account

```bash
# Close a existing namespace with the seed "some-namespace"
nitro-da-cli -p "<program-id-of-blober>" -n "some-namespace" br c
```

## `nitro-da-cli blob` or `nitro-da-cli b`

This subcommand allows you to upload and discard blobs of data with the following commands:

- `nitro-da-cli b upload` or `nitro-da-cli b u` to upload data read from a file at the given `--data-path` (or `-d`)

```bash
# Upload a blob of data read from a file
nitro-da-cli -p "<program-id-of-blober>" -n "some-namespace" b u -d "./data.txt"
```

- `nitro-da-cli b discard` or `nitro-da-cli b d` to discard a blob upload with the given `blob` PDA (`--blob` or `-b`)
  before it is finalized

```bash
# Discard a failed or stale blob upload with the given PDA ID
nitro-da-cli -p "<program-id-of-blober>" -n "some-namespace" b d -b "<some-blob-pda-id>"
```

- `nitro-da-cli b fetch` or `nitro-da-cli b f` to fetch blob data from the ledger given a list of transaction signatures

```bash
# Fetch blob data from the ledger given a list of transactions
nitro-da-cli -p "<program-id-of-blober>" -n "some-namespace" b f "<some-tx-signature>" "<some-other-tx-signature>"
```

- `nitro-da-cli b get` or `nitro-da-cli b g` to retrieve all blobs finalized at a given slot

```bash
# Get all blobs which were finalized at the given slot
nitro-da-cli -p "<program-id-of-blober>" -n "some-namespace" b g 54321
```

## `nitro-da-cli indexer` or `nitro-da-cli i`

This subcommand allows you to fetch blobs and proofs for a given `blober` PDA and `slot` using the following commands:

- `nitro-da-cli i blobs` or `nitro-da-cli i b` to fetch blobs

```bash
# Retrieve all blobs finalized at the given slot
nitro-da-cli -p "<program-id-of-blober>" -n "some-namespace" i b 54321
```

- `nitro-da-cli i proofs` or `nitro-da-cli i p` to fetch proofs

```bash
# Fetch a compound proof (inclusion or completness) for a given slot
nitro-da-cli -p "<program-id-of-blober>" -n "some-namespace" i p 54321
```

## `nitro-da-cli measure` or `nitro-da-cli m`

This subcommand allow you to generate data files and upload them to capture some measurements using the following
commands:

- `nitro-da-cli m generate` or `nitro-da-cli m g` to generate data files to a given directory
- `nitro-da-cli m measure` or `nitro-da-cli m m` to upload data from a given directory and capture measurements
- `nitro-da-cli m automate` or `nitro-da-cli m a` to run automated benchmarks with combinations of different data
  generation and measurement scenarios
