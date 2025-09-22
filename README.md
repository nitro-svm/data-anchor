<div align="center">
  <picture>
      <img alt="DA" src="https://github.com/user-attachments/assets/552eeb0a-ca6b-49ba-9cea-2b9e1a7ac1c5" />
  </picture>
</div>

### TL;DR
Data Anchor delivers on‑chain verifiable storage at a fraction of the cost.


## Introduction

Data Anchor lets you store your data blobs on Solana’s ledger—packed and fully verifiable—while keeping costs low via efficient storage and an indexer. You pay rent once per blob, fetch data instantly via HTTP or CLI, and enjoy order‑of‑magnitude savings over naïvely stuffing bytes into on‑chain accounts 


## Quickstart

**Usage**
- [CLI Usage](https://github.com/nitro-svm/examples/tree/main/anchoring-data/cli)
- [Client Usage](https://github.com/nitro-svm/examples/tree/main/anchoring-data/client)

**Video Demos**

1. [Uploading and Verifying Data on Solana](https://youtu.be/sgjmaujHYdE?si=a4GJrxBX5B24HHvF)
2. [Upload and Index Data on Solana](https://youtu.be/sgjmaujHYdE?si=a4GJrxBX5B24HHvF)


## Why use Data Anchor

- **Minimized rent costs**: Storing data directly on Solana can cost ≈6.96 SOL per MB annually (rent‑exempt deposit), Data Anchor amortizes rent across data blobs to deliver over 100,000× savings. 

- **Handles arbitrary data**: Supports any payload—IoT metrics or availability proofs, ideal for data‑intensive DePIN networks generating millions of inputs per minute. 

- **Efficient ledger usage**: By packing data blobs into Solana’s append‑only ledger history where storage is cheaper than accounts, Data Anchor retains only minimal commitments in account space.

- **Instant retrieval**: Anchored blobs can be fetched in their original JSON form with no manual reassembly, removing the overhead of reconstructing data from raw transactions.

- **Immutable, verifiable data**: Every blob lives onchain, giving you cryptographic proof of integrity that can be independently audited against Solana’s tamper‑proof ledger. 

- **Built for scale**: Data Anchor leverages Solana’s high throughput (up to ~1,289 TPS) and sub‑second block times (≈0.4 s).


## Architecture

We built Data Anchor as a set of focused Rust crates for anchoring data, on‑chain interaction, indexing, and proof handling—so teams can pick only the pieces they need. This modular design powers DePIN networks, rollups, and data‑heavy dApps seeking verifiable, scalable storage.


## Examples Overview

A quick run‑through of core workflows—see the CLI or RPC docs for full commands:

* **Initialize Namespace**
  Create a new blober PDA for your namespace.

* **Upload Data**
  Store a JSON file as an on‑chain blob.

* **Fetch & Decode Blob**
  Retrieve by signature and decode the hex‑encoded data back into JSON.

* **get\_blobs\_by\_namespace**
  List all blobs under your namespace via the indexer.

* **get\_blobs\_by\_payer**
  List blobs paid for by your wallet.

* **get\_blobs**
  Fetch a specific blob by PDA and slot.

* **get\_payers\_by\_network**
  List all payers on a given network.

* **get\_proof**
  Obtain a cryptographic proof for a blob.


## Full API reference and integration guides
[Data Anchor Developer Documentation](https://docs.termina.technology/documentation/network-extension-stack/modules/data-anchor)

#### Data Anchor Crate Documentations

* [CLI Documentation](https://docs.rs/data-anchor/latest/data_anchor/)
* [Client API](https://docs.rs/data-anchor-client/latest/data_anchor_client/)
* [Blober Program](https://docs.rs/data-anchor-blober/latest/data_anchor_blober/)
* [Indexer API](https://docs.rs/data-anchor-api/latest/data_anchor_api/)
* [Proofs API](https://docs.rs/data-anchor-proofs/latest/data_anchor_proofs/)


## Support

Got questions or feedback? Reach out to us on [Twitter](https://x.com/Terminaxyz)!