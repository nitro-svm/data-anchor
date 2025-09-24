# Data Anchor Proofs

This crate is used by the client to verify the correctness of data from the indexer service.

## Proof Modules Overview

This crate exposes several proof types that together allow clients to validate information
returned by the indexer against on-chain state. Each proof is built from data that already
exists on chain so that a client can recompute the expected value and verify it locally:

- **Accounts delta hash proofs** - prove membership (inclusion) or absence (exclusion) of
  an account in the Merkle tree of updated accounts for a slot. The root of that tree is
  the `accounts_delta_hash`, which can be checked against the value found in a [`BankHashProof`](#bank-hash-proof).
- **Bank hash proofs** - contain the components that hash together to form the bank hash
  for a block, including the accounts delta hash. Verifying it shows that the accounts
  delta tree really produced the bank hash returned by Solana RPC.
- **Slot hash proofs** - prove which bank hash was stored in the `SlotHashes` sysvar for
  a specific slot. This ties the bank hash to a slot number and blockhash recorded on chain.
- **Blob proofs** - prove that a blob uploaded through the `blober` program hashes to the
  expected digest. A client provides the raw blob bytes to recompute the digest locally.
- **Compound proofs** - combine accounts delta hash, bank hash, slot hash and blob proofs
  into one structure. They show either that given blobs were included (`CompoundInclusionProof`)
  or that no blobs were present (`CompoundCompletenessProof`) in a specific block.

Inclusion proofs demonstrate that an account's data was updated in a block, whereas
exclusion proofs show that it was untouched. The indexer builds these proofs by walking
the Merkle tree for the slot and gathering the sibling hashes needed to recompute the
`accounts_delta_hash`. To verify them you supply the expected root (often taken from a
`BankHashProof`).

`BankHashProof` is derived from block metadata - parent bank hash, `accounts_delta_hash`,
signature count and blockhash - and can be checked by hashing these fields and comparing
with the bank hash returned by Solana RPC. `SlotHashProof` contains a copy of the
`SlotHashes` sysvar so that a client can check that this bank hash was recorded for the
slot. `BlobProof` is computed from the chunk order and digest stored in the blob account
and lets a client verify the raw bytes.

`CompoundInclusionProof` and `CompoundCompletenessProof` bundle all of the above together.
They are what the indexer returns when a client requests a proof for a slot or for a
particular blob. By verifying a compound proof you ensure that the blobs returned by the
indexer correspond to the actual on-chain state and that no uploads were missed.

When a client retrieves data from the indexer it can use these proofs to check that each
piece of data matches what actually happened on chain.

### Verifying proofs

Below are short snippets showing how a client might verify each proof type:

- **Accounts delta hash proofs**

```rust
use data_anchor_proofs::accounts_delta_hash::{inclusion::InclusionProof, exclusion::ExclusionProof};

inclusion_proof.verify(expected_accounts_delta_hash);
exclusion_proof.verify(expected_accounts_delta_hash)?;
```

- **Bank hash proof**

```rust
use data_anchor_proofs::bank_hash::BankHashProof;

bank_hash_proof.verify(expected_bank_hash);
```

- **Slot hash proof**

```rust
use data_anchor_proofs::slot_hash::SlotHashProof;

slot_hash_proof.verify(slot, bank_hash, accounts_delta_hash)?;
```

- **Blob proof**

```rust
use data_anchor_proofs::blob::BlobProof;

blob_proof.verify(&blob_bytes)?;
```

- **Compound proofs** (`CompoundInclusionProof` / `CompoundCompletenessProof`)

```rust
use data_anchor_proofs::compound::{inclusion::CompoundInclusionProof, completeness::CompoundCompletenessProof, inclusion::ProofBlob};

compound_proof.verify(blober_program, blockhash, &[ProofBlob { blob: blob_pubkey, data: Some(blob_bytes) }])?;
```

## Accounts Exclusion Proofs

The [`AccountMerkleTree`](https://github.com/nitro-svm/data-anchor-oss/blob/main/crates/proofs/src/accounts_delta_hash/account_merkle_tree/tree.rs#L33-L38)
is a 16-ary Merkle tree that supports exclusion proofs, which allow callers to verify a given account is not included among its leaves.
There are [4 types](https://github.com/nitro-svm/data-anchor-oss/blob/main/crates/proofs/src/accounts_delta_hash/exclusion/proof.rs#L12-L20) of exclusion proofs:

- Empty: the tree is empty, so no accounts can be present.
- Left: an account is smaller than the leftmost leaf and must be to its left.
- Right: an account is bigger than the rightmost leaf and must be to its right.
- Inner: an account has to exist between two adjacent leaves, but no such gap exists.

If each leaf node stored its global index, then exclusion checks would be trivial. Unfortunately, the `AccountMerkleTree` doesn't contain this information,
so instead we rely on the relative index from [`InclusionProofLevel`](https://github.com/nitro-svm/data-anchor-oss/blob/dcc09b5e8a16e5a287882ccd4126e8cfb82afc23/crates/proofs/src/accounts_delta_hash/inclusion.rs#L11-L18):

- This represents the node's position within its immediate subtree.
- It ranges from 0 to `fanout - 1`, where `fanout` is n in an n-ary tree (e.g. 2 in binary trees, 16 in our case).

### Left and Right Proofs

If we had global indexes, verifying left or right exclusion would be as simple as checking for index 0 (leftmost)
or `leaf_count - 1` (rightmost). However, we have to be creative with the relative indexes.

If a node is truly the leftmost node, then its relative index up the tree will _always_ be 0.

```
     o       <-- m's parent is o, which also has a relative index of 0
    /\
   m   n     <-- i's parent is m, whose relative index is 0
  /\   /\
 i  j  k l   <-- a's parent is i, whose relative index is 0
/\ /\ /\ /\
ab cd ef gh  <-- start with a, whose relative index is 0 in its subtree
^
```

Even if a node's relative index is 0 to start, one of its ancestors will _not_ be 0.

```
     o
    /\
   m   n     <-- k's parent is n, whose relative index is 1
  /\   /\
 i  j  k l   <-- e's parent is k, whose relative index is 0
/\ /\ /\ /\
ab cd ef gh  <-- start with e, whose relative index is 0 in its subtree
      ^
```

The same logic applies for the rightmost proofs, but comparisons are done against the `fanout` number instead of 0.

### Inner Proofs

If we had global indexes, we'd just verify that the right index minus the left equals 1.
But since we don't, we have to be clever and track the difference between the relative indexes up the tree.

For two leaves to be adjacent, they must follow one of 3 state transitions at each level:

- Subtree -> subtree
  - If two nodes are adjacent but in different subtrees, then the left node must have a relative index of `fanout - 1`
    and the right node must have an index of 0
  - `fanout - 1` -> `fanout - 1`
- Subtree -> siblings
  - If two nodes are adjacent siblings, then their indexes differ by 1
  - `fanout - 1` -> 1
- Siblings -> same parent
  - If two nodes converge to the same node, then the indexes are identical and the difference is 0
  - 1 -> 0

If the path up the tree doesn't fit this pattern, then the two original leaves are _not_ adjacent and don't form a valid exclusion proof.

This is an exclusion proof that starts with adjacent siblings `g` and `h`.

```
     o       <-- end with o (same parent)
    /\
   m   n     <-- l's parent is n, l's parent is still n (same parent)
  /\   /\
 i  j  k l   <-- g's parent is l, h's parent is also l (same parent)
/\ /\ /\ /\
ab cd ef gh  <-- start with g and h (adjacent siblings)
         ^^
```

Here's another exclusion proof that starts with adjacent leaves `d` and `e` in different subtrees.

```
     o       <-- end with o (same parent)
    /\
   m   n     <-- j's parent is m, k's parent is n (adjacent sibling)
  /\   /\
 i  j  k l   <-- d's parent is j, e's parent is k (adjacent subtree)
/\ /\ /\ /\
ab cd ef gh  <-- start with d and e (adjacent subtree)
    ^ ^
```

Note that _adjacent_ nodes means they're immediately next to each other, not only siblings under the same parent or in neighboring subtrees.
