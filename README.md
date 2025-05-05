# Solana DA

This folder contains a few different crates that together form the DA adapter for Solana.

1. [`programs`](programs/programs/blober/README.md) contains the Solana program that needs to be deployed on-chain.
   - The `blober` program is used to split up blobs into chunks, and "stores" the chunks by passing them as arguments to the program. The data is never actually persisted on-chain, instead using the much cheaper ledger storage. As chunks come in one by one they will be hashed as a (incremental/sequential) merkle tree, yielding a final `digest` at the end which _is_ persisted in the `blob` PDA account.
   - When a blob's chunks are all fully submitted, the `blober` program hashes the `digest` itself and this new hash is persisted in the `blober` account. This might seem superfluous at first, but the purpose is to have a well-known address that can be proven whether it is present or not in the list of updated accounts that Solana calculates for each block.
2. [`client`](./crates/client/README.md) contains a client that can be used to interact with the `blober` program. It also contains a client for the `Indexer` RPC. The `BloberClient` then wraps all the complexity of creating a `blober` account, splitting a blob into chunks, uploading the chunks, hashing the `digest`, and closing the PDAs to reclaim rent. It also makes educated guesses about setting good prioritization fees (without overspending) to ensure transactions are included by validators.
3. [`proofs`](./crates/proofs/README.md) contains the various proofs that are used to verify the state of the `blober`. This is how data availability can be proven all the way from a blob's chunks.
4. [`indexer`](./crates/indexer/README.md) contains the code that runs the indexer, which is a geyser plugin used to monitor the `blober` accounts as they are being used. The indexer stores data in an embedded `native_db` database, and must be configured to monitor specific `blober` accounts.
5. [`indexer_api`](./crates/indexer_api/README.md) contains the shared interface between the indexer and the client.
6. [`cli`](./crates/cli/README.md) contains the CLI used to interact with the data module. This includes interacting with the on chain program to manage `blober` accounts and upload blobs, as well as the `indexer` to retrieve blob data and proofs.

## Installation

Install the following tools:

- [Solana CLI](https://docs.solanalabs.com/cli/install)
- [nodejs (using nvm)](https://nodejs.org/en/download/package-manager)
- [yarn (using corepack)](https://yarnpkg.com/getting-started/install)
- [Anchor (using avm)](https://www.anchor-lang.com/docs/installation#installing-using-anchor-version-manager-avm-recommended)
- [cargo-mutants](https://mutants.rs/installation.html)

All following sections assume the tools have been installed. It's also assumed that you've set the current working directory to the crate you want to test.

## Building

To build the `programs` crate, run `anchor build`. This will create program keypairs and store them in `programs/target/deploy/blober-keypair.json`. In a real deployment these would be the highly sensitive deployment keys used for updating the programs on-chain and the public keys would be well-known for each rollup, but during development it's normal for each developer to have their own keypairs.

Before deploying `programs` (e.g. to localnet) the keys must be synced with the source code using `anchor keys sync`. This will overwrite the IDs in the main `lib.rs` files and in `Anchor.toml`. Don't commit this change, it would just cause churn for a key that isn't checked in anywhere.

The other crates are built as usual with `cargo build`.

## Testing

### Unit tests, integration tests, doctests

Most of these run as normal with `cargo nextest run`, with some minor deviations.

#### client

`client` has a unit test that can run in two configurations, one of them requires a local Solana cluster. This test is disabled by default because it requires a local cluster to have been started with `anchor localnet`, to then run it you may issue this command `cargo test -- --include-ignored`. To monitor logs while the test is running run `solana logs --url localhost` in a separate terminal.

Before running integration tests the `programs` crate must be built with `anchor build`. This is because the `client` integration tests deploy the shared object files from `programs` directly before running its tests, instead of running the code natively.

#### proofs

The `proofs` crate makes heavy use of [`arbtest`](https://crates.io/crates/arbtest) for property-based testing. These tests rely on having the environment variable `ARBTEST_BUDGET_MS` set to something reasonable to ensure tests hit diverse cases (there are a lot of corner cases in the code). For quick iteration a good value is `2000` (2 seconds), but for final checks a value of at least `10000` is recommended. The limit is per-test but tests run in parallel so it's not as bad as it might seem.

The `ARBTEST_BUDGET_MS` variable should also be set when running coverage or mutation tests, since they rely on repeated execution to find mutants and cover all code paths.

### Mutation testing

In order to ensure tests are actually useful and not just a code coverage metric, mutation testing is used to find untested code paths. Mutation testing will attempt to inject bugs by modifying the source code, and if none of the tests fail, that's a case that hasn't been properly tested.

To run mutation tests, run `ARBTEST_BUDGET_MS=10000 cargo mutants`.

For now mutation testing has been focused on the `proofs` crate.

## Documentation

Certain crates rely on optional features, so corresponding documentation will fail to be linked by cargo-doc unless those features are specified during documentation generation. For instance to generate docs for the `sov-sdk/crates/adapters/solana/adapter` crate, one needs to generate it as `cargo doc --features native`.
