# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.2](https://github.com/nitro-svm/data-anchor-oss/compare/data-anchor-api-v0.4.1...data-anchor-api-v0.4.2) - 2025-08-27

### Added

- Add verifier alias instead of pubkey for checkpoint authority ([#345](https://github.com/nitro-svm/data-anchor-oss/pull/345))

### Other

- Use AccountDeserialize instead of AnchorDeserialize for account data ([#344](https://github.com/nitro-svm/data-anchor-oss/pull/344))

## [0.4.1](https://github.com/nitro-svm/data-anchor-oss/compare/data-anchor-api-v0.4.0...data-anchor-api-v0.4.1) - 2025-08-19

### Added

- Add foot-gun protection against early blober closing ([#310](https://github.com/nitro-svm/data-anchor-oss/pull/310))
- Improve end-to-end test and fix bugs discovered by it ([#308](https://github.com/nitro-svm/data-anchor-oss/pull/308))
- Add proof request status query ([#305](https://github.com/nitro-svm/data-anchor-oss/pull/305))
- Add proof posting on-chain after ZK generation ([#303](https://github.com/nitro-svm/data-anchor-oss/pull/303))
- Add client and CLI methods for checkpoint handling ([#281](https://github.com/nitro-svm/data-anchor-oss/pull/281))
- Add initial checkpoint PDA ([#280](https://github.com/nitro-svm/data-anchor-oss/pull/280))
- Add prover program and proof generation script ([#272](https://github.com/nitro-svm/data-anchor-oss/pull/272))
- Strip down proof system ([#268](https://github.com/nitro-svm/data-anchor-oss/pull/268))

### Other

- Clean up all dependencies ([#302](https://github.com/nitro-svm/data-anchor-oss/pull/302))
- Use solana_transaction instead of solana_sdk Transaction import ([#299](https://github.com/nitro-svm/data-anchor-oss/pull/299))
- Use solana_pubkey instead of solana_sdk Pubkey import ([#296](https://github.com/nitro-svm/data-anchor-oss/pull/296))
- Make time range param consistently optional ([#283](https://github.com/nitro-svm/data-anchor-oss/pull/283))
- Add health check endpoint ([#267](https://github.com/nitro-svm/data-anchor-oss/pull/267))

## [0.4.0](https://github.com/nitro-svm/data-anchor-oss/compare/data-anchor-api-v0.3.1...data-anchor-api-v0.4.0) - 2025-07-10

### Added

- Separate RPC into standalone binary ([#245](https://github.com/nitro-svm/data-anchor-oss/pull/245))
- Improve PDA management and outputs ([#241](https://github.com/nitro-svm/data-anchor-oss/pull/241))
- Improve CLI integration ([#234](https://github.com/nitro-svm/data-anchor-oss/pull/234))

### Fixed

- Don't run insert network query if no networks found ([#230](https://github.com/nitro-svm/data-anchor-oss/pull/230))

### Other

- Update indexer API docs ([#231](https://github.com/nitro-svm/data-anchor-oss/pull/231))
- Simplify API params on indexer RPC ([#228](https://github.com/nitro-svm/data-anchor-oss/pull/228))

## [0.3.1](https://github.com/nitro-svm/data-anchor-oss/compare/data-anchor-api-v0.3.0...data-anchor-api-v0.3.1) - 2025-07-01

### Added

- Add ledger size fallback for indexer to pick up blobs ([#221](https://github.com/nitro-svm/data-anchor-oss/pull/221))

## [0.3.0](https://github.com/nitro-svm/data-anchor-oss/compare/data-anchor-api-v0.2.0...data-anchor-api-v0.3.0) - 2025-06-26

### Added

- Add S3 storage for completeness proofs ([#210](https://github.com/nitro-svm/data-anchor-oss/pull/210))
- Add `payer_pubkey` to namespace query ([#203](https://github.com/nitro-svm/data-anchor-oss/pull/203))
- Add more queries to the API ([#182](https://github.com/nitro-svm/data-anchor-oss/pull/182))

### Other

- Upgrade edition, version and formatting ([#211](https://github.com/nitro-svm/data-anchor-oss/pull/211))
- Refactor and update docs for indexer API ([#180](https://github.com/nitro-svm/data-anchor-oss/pull/180))

## [0.1.7](https://github.com/nitro-svm/nitro-data-module/compare/nitro-da-indexer-api-v0.1.6...nitro-da-indexer-api-v0.1.7) - 2025-05-30

### Added

- Expand indexer API to support more usefull queries ([#150](https://github.com/nitro-svm/nitro-data-module/pull/150))

### Fixed

- Properly migrate to postgres and add test script ([#146](https://github.com/nitro-svm/nitro-data-module/pull/146))

## [0.1.5](https://github.com/nitro-svm/nitro-data-module/compare/nitro-da-indexer-api-v0.1.4...nitro-da-indexer-api-v0.1.5) - 2025-04-30

### Other

- Update crate documentation ([#115](https://github.com/nitro-svm/nitro-data-module/pull/115))
