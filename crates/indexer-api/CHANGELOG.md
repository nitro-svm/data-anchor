# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0](https://github.com/nitro-svm/data-anchor/compare/data-anchor-api-v0.3.1...data-anchor-api-v0.4.0) - 2025-07-10

### Added

- Separate RPC into standalone binary ([#245](https://github.com/nitro-svm/data-anchor/pull/245))
- Improve PDA management and outputs ([#241](https://github.com/nitro-svm/data-anchor/pull/241))
- Improve CLI integration ([#234](https://github.com/nitro-svm/data-anchor/pull/234))

### Fixed

- Don't run insert network query if no networks found ([#230](https://github.com/nitro-svm/data-anchor/pull/230))

### Other

- Update indexer API docs ([#231](https://github.com/nitro-svm/data-anchor/pull/231))
- Simplify API params on indexer RPC ([#228](https://github.com/nitro-svm/data-anchor/pull/228))

## [0.3.1](https://github.com/nitro-svm/data-anchor/compare/data-anchor-api-v0.3.0...data-anchor-api-v0.3.1) - 2025-07-01

### Added

- Add ledger size fallback for indexer to pick up blobs ([#221](https://github.com/nitro-svm/data-anchor/pull/221))

## [0.3.0](https://github.com/nitro-svm/data-anchor/compare/data-anchor-api-v0.2.0...data-anchor-api-v0.3.0) - 2025-06-26

### Added

- Add S3 storage for completeness proofs ([#210](https://github.com/nitro-svm/data-anchor/pull/210))
- Add `payer_pubkey` to namespace query ([#203](https://github.com/nitro-svm/data-anchor/pull/203))
- Add more queries to the API ([#182](https://github.com/nitro-svm/data-anchor/pull/182))

### Other

- Upgrade edition, version and formatting ([#211](https://github.com/nitro-svm/data-anchor/pull/211))
- Refactor and update docs for indexer API ([#180](https://github.com/nitro-svm/data-anchor/pull/180))

## [0.1.7](https://github.com/nitro-svm/nitro-data-module/compare/nitro-da-indexer-api-v0.1.6...nitro-da-indexer-api-v0.1.7) - 2025-05-30

### Added

- Expand indexer API to support more usefull queries ([#150](https://github.com/nitro-svm/nitro-data-module/pull/150))

### Fixed

- Properly migrate to postgres and add test script ([#146](https://github.com/nitro-svm/nitro-data-module/pull/146))

## [0.1.5](https://github.com/nitro-svm/nitro-data-module/compare/nitro-da-indexer-api-v0.1.4...nitro-da-indexer-api-v0.1.5) - 2025-04-30

### Other

- Update crate documentation ([#115](https://github.com/nitro-svm/nitro-data-module/pull/115))
