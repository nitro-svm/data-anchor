# Data Anchor Proofs

This crate is used by the client to verify the correctness of data from the indexer service.

## Accounts Exclusion Proofs

The [`AccountMerkleTree`](https://github.com/nitro-svm/data-anchor/blob/main/crates/proofs/src/accounts_delta_hash/account_merkle_tree/tree.rs#L33-L38)
is a 16-ary Merkle tree that supports exclusion proofs, which allow callers to verify a given account is not included among its leaves.
There are [4 types](https://github.com/nitro-svm/data-anchor/blob/main/crates/proofs/src/accounts_delta_hash/exclusion/proof.rs#L12-L20) of exclusion proofs:

- Empty: the tree is empty, so no accounts can be present.
- Left: an account is smaller than the leftmost leaf and must be to its left.
- Right: an account is bigger than the rightmost leaf and must be to its right.
- Inner: an account has to exist between two adjacent leaves, but no such gap exists.

If each leaf node stored its global index, then exclusion checks would be trivial. Unfortunately, the `AccountMerkleTree` doesn't contain this information,
so instead we rely on the relative index from [`InclusionProofLevel`](https://github.com/nitro-svm/data-anchor/blob/dcc09b5e8a16e5a287882ccd4126e8cfb82afc23/crates/proofs/src/accounts_delta_hash/inclusion.rs#L11-L18):

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
