# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.2](https://github.com/nitro-svm/data-anchor/compare/data-anchor-client-v0.4.1...data-anchor-client-v0.4.2) - 2025-08-27

### Added

- Add `set_loaded_account_data_size_limit` instruction to transactions ([#350](https://github.com/nitro-svm/data-anchor/pull/350))
- Add default indexer URL and recognition based on RPC URL ([#338](https://github.com/nitro-svm/data-anchor/pull/338))
- Add encoding and compression markers to data ([#337](https://github.com/nitro-svm/data-anchor/pull/337))

### Other

- Clean up CLI and improve client constants ([#352](https://github.com/nitro-svm/data-anchor/pull/352))
- Use AccountDeserialize instead of AnchorDeserialize for account data ([#344](https://github.com/nitro-svm/data-anchor/pull/344))
- Enhance logging for transaction processing and blob operations ([#343](https://github.com/nitro-svm/data-anchor/pull/343))

## [0.4.1](https://github.com/nitro-svm/data-anchor/compare/data-anchor-client-v0.4.0...data-anchor-client-v0.4.1) - 2025-08-19

### Added

- Add SP1 support for compression ([#326](https://github.com/nitro-svm/data-anchor/pull/326))
- Add compression support to client ([#315](https://github.com/nitro-svm/data-anchor/pull/315))
- Add encoding support to client ([#314](https://github.com/nitro-svm/data-anchor/pull/314))
- Add foot-gun protection against early blober closing ([#310](https://github.com/nitro-svm/data-anchor/pull/310))
- Improve end-to-end test and fix bugs discovered by it ([#308](https://github.com/nitro-svm/data-anchor/pull/308))
- Add proof request status query ([#305](https://github.com/nitro-svm/data-anchor/pull/305))
- Add proof posting on-chain after ZK generation ([#303](https://github.com/nitro-svm/data-anchor/pull/303))
- Add verifiers and improve checkpointing ([#294](https://github.com/nitro-svm/data-anchor/pull/294))
- Add client and CLI methods for checkpoint handling ([#281](https://github.com/nitro-svm/data-anchor/pull/281))
- Strip down proof system ([#268](https://github.com/nitro-svm/data-anchor/pull/268))
- Use vanity address for blober program ([#266](https://github.com/nitro-svm/data-anchor/pull/266))

### Fixed

- Remove memcmp filter ([#330](https://github.com/nitro-svm/data-anchor/pull/330))
- Swap cost and balance in error enum ([#288](https://github.com/nitro-svm/data-anchor/pull/288))

### Other

- Use `sp1-solana` from crates.io ([#332](https://github.com/nitro-svm/data-anchor/pull/332))
- Benchmark cleanup ([#312](https://github.com/nitro-svm/data-anchor/pull/312))
- Add list payers command and improve e2e script ([#306](https://github.com/nitro-svm/data-anchor/pull/306))
- Clean up all dependencies ([#302](https://github.com/nitro-svm/data-anchor/pull/302))
- Remove solana_sdk everywhere (almost) ([#301](https://github.com/nitro-svm/data-anchor/pull/301))
- Use solana_clock instead of solana_sdk Slot import ([#300](https://github.com/nitro-svm/data-anchor/pull/300))
- Use solana_transaction instead of solana_sdk Transaction import ([#299](https://github.com/nitro-svm/data-anchor/pull/299))
- Use solana_signer instead of solana_sdk Signer import ([#298](https://github.com/nitro-svm/data-anchor/pull/298))
- Use solana_keypair instead of solana_sdk Keypair import ([#297](https://github.com/nitro-svm/data-anchor/pull/297))
- Use solana_pubkey instead of solana_sdk Pubkey import ([#296](https://github.com/nitro-svm/data-anchor/pull/296))
- Make time range param consistently optional ([#283](https://github.com/nitro-svm/data-anchor/pull/283))

## [0.4.0](https://github.com/nitro-svm/data-anchor/compare/data-anchor-client-v0.3.1...data-anchor-client-v0.4.0) - 2025-07-10

### Added

- Add balance and account existance checks for on-chain commands ([#261](https://github.com/nitro-svm/data-anchor/pull/261))
- Improve PDA management and outputs ([#241](https://github.com/nitro-svm/data-anchor/pull/241))
- Improve client README.md ([#237](https://github.com/nitro-svm/data-anchor/pull/237))
- Improve client and CLI ergonomics ([#235](https://github.com/nitro-svm/data-anchor/pull/235))
- Improve CLI integration ([#234](https://github.com/nitro-svm/data-anchor/pull/234))

### Fixed

- Add full e2e test and squash discovered bugs ([#257](https://github.com/nitro-svm/data-anchor/pull/257))
- Add user-agent for client calls to pass WAF protection ([#255](https://github.com/nitro-svm/data-anchor/pull/255))
- Serialize blober identifier in parent struct and streamline identifier on CLI level ([#244](https://github.com/nitro-svm/data-anchor/pull/244))
- Explictly set max supported transaction version ([#238](https://github.com/nitro-svm/data-anchor/pull/238))

### Other

- Nicer imports of `Hash` ([#262](https://github.com/nitro-svm/data-anchor/pull/262))
- Also use `BloberIdentifier` on initialization ([#260](https://github.com/nitro-svm/data-anchor/pull/260))
- Remove unused `use_helius_fee` param ([#256](https://github.com/nitro-svm/data-anchor/pull/256))
- Fix typo in error message ([#233](https://github.com/nitro-svm/data-anchor/pull/233))
- Use `HttpClient` instead of `WsClient` ([#232](https://github.com/nitro-svm/data-anchor/pull/232))
- Simplify API params on indexer RPC ([#228](https://github.com/nitro-svm/data-anchor/pull/228))

## [0.3.1](https://github.com/nitro-svm/data-anchor/compare/data-anchor-client-v0.3.0...data-anchor-client-v0.3.1) - 2025-07-01

### Added

- Add ledger size fallback for indexer to pick up blobs ([#221](https://github.com/nitro-svm/data-anchor/pull/221))

## [0.3.0](https://github.com/nitro-svm/data-anchor/compare/data-anchor-client-v0.2.0...data-anchor-client-v0.3.0) - 2025-06-26

### Added

- Add `payer_pubkey` to namespace query ([#203](https://github.com/nitro-svm/data-anchor/pull/203))
- Rename `BloberClient` to `DataAnchorClient` ([#202](https://github.com/nitro-svm/data-anchor/pull/202))
- Add API key support to blober client and CLI ([#201](https://github.com/nitro-svm/data-anchor/pull/201))
- Add new methods to blober client ([#199](https://github.com/nitro-svm/data-anchor/pull/199))

### Other

- Upgrade edition, version and formatting ([#211](https://github.com/nitro-svm/data-anchor/pull/211))
- Modularize blober client impls ([#198](https://github.com/nitro-svm/data-anchor/pull/198))
- Refactor and update docs for indexer API ([#180](https://github.com/nitro-svm/data-anchor/pull/180))

## [0.1.7](https://github.com/nitro-svm/nitro-data-module/compare/nitro-da-client-v0.1.6...nitro-da-client-v0.1.7) - 2025-05-30

### Added

- Expand indexer API in blober client ([#152](https://github.com/nitro-svm/nitro-data-module/pull/152))

### Other

- Run lints ([#147](https://github.com/nitro-svm/nitro-data-module/pull/147))

## [0.1.5](https://github.com/nitro-svm/nitro-data-module/compare/nitro-da-client-v0.1.4...nitro-da-client-v0.1.5) - 2025-04-30

### Added

- Migrate SDK to using namespaces instead of pubkeys ([#124](https://github.com/nitro-svm/nitro-data-module/pull/124))

### Other

- Update crate documentation ([#115](https://github.com/nitro-svm/nitro-data-module/pull/115))
