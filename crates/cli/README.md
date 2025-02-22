# Nitro Da CLI

This crate allows users to interact with the `blober` program in order to register new `blober` PDA accounts, upload blobs and retrieve data from the indexer.

The mandatory parameters for this tool are the address of the `blober` program (`--program-id` or `-p`), the path to the `payer` keypair of the transactions (`--payer` or `-s`),
the namespace of the `blober` PDA to interact with (`--namespace` or `-n`) and the indexer URL (`--indexer-url` or `-i`).

The CLI has a `help` command (available for each subcommand as well) which you can use to check the usage.
Here is a high level overview of the available commands.

## `nitro-da-cli blober` or `nitro-da-cli br`

This subcommand allows you to initialize and close `blober` PDA accounts with the followign commands:

- `nitro-da-cli br initialize` or `nitro-da-cli br i` to initialize a new `blober` PDA account
- `nitro-da-cli br close` or `nitro-da-cli br c` to close an existing `blober` PDA account

## `nitro-da-cli blob` or `nitro-da-cli b`

This subcommand allows you to upload and discard blobs of data with the following commands:

- `nitro-da-cli b upload` or `nitro-da-cli b u` to upload data read from a file at the given `--data-path` (or `-d`)
- `nitro-da-cli b discard` or `nitro-da-cli b d` to discard a blob upload with the given `blob` PDA (`--blob` or `-b`) before it is finalized

## `nitro-da-cli indexer` or `nitro-da-cli i`

This subcommand allows you to fetch blobs and proofs for a given `blober` PDA and `slot` using the following commands:

- `nitro-da-cli i blobs` or `nitro-da-cli i b` to fetch blobs
- `nitro-da-cli i proofs` or `nitro-da-cli i p` to fetch proofs

## `nitro-da-cli measure` or `nitro-da-cli m`

This subcommand allow you to generate data files and upload them to capture some measurements using the following commands:

- `nitro-da-cli m generate` or `nitro-da-cli m g` to generate data files to a given directory
- `nitro-da-cli m measure` or `nitro-da-cli m m` to upload data from a given directory and capture measurements
