use crate::accounts_delta_hash::MERKLE_FANOUT;

/// Creates a Merkle tree from a list of hashes.
///
/// # Arguments
/// - `hashes` - The list of hashes to create the tree from. Must not be empty.
pub fn hash_tree(hashes: Vec<solana_sdk::hash::Hash>) -> Vec<Vec<solana_sdk::hash::Hash>> {
    if hashes.is_empty() {
        // This is how an empty tree is defined in Solana.
        return vec![vec![solana_sdk::hash::Hasher::default().result()]];
    }

    // Here we get a tree with only one level - the leaves.
    let mut tree = vec![hashes];

    // Loop instead of while because it needs to run at least once.
    loop {
        // Take the highest level of the tree and create the next level.
        let highest_level = tree.last().expect("tree to have at least one level");

        // Add each groups hashes to the next level of the tree.
        let next_level_hashes = highest_level
            // At most 16 hashes in a group.
            .chunks(MERKLE_FANOUT)
            .map(|group| {
                let mut hasher = solana_sdk::hash::Hasher::default();
                // Create a hash of siblings.
                for hash in group {
                    hasher.hash(hash.as_ref());
                }
                hasher.result()
            })
            .collect::<Vec<_>>();

        assert!(
            !next_level_hashes.is_empty(),
            "No hashes left in the tree, something went wrong."
        );

        let is_root = single_hash_remains(&next_level_hashes);

        // Add the next level to the tree.
        tree.push(next_level_hashes);

        // If there is only one hash left, it is the root of the tree.
        if is_root {
            break;
        }
    }

    tree
}

// Extracted to a function to skip mutation testing on this function, it just causes timeouts.
#[cfg_attr(test, mutants::skip)]
fn single_hash_remains(current_hashes: &[solana_sdk::hash::Hash]) -> bool {
    current_hashes.len() == 1
}

#[cfg(test)]
mod tests {
    use arbtest::arbtest;
    use solana_accounts_db::accounts_hash::AccountsHasher;

    use super::*;

    #[test]
    fn hash_tree_empty_returns_default_hash() {
        let leaves = vec![];
        let levels = hash_tree(leaves.clone());
        assert_eq!(
            levels,
            vec![vec![solana_sdk::hash::Hasher::default().result()]]
        );
    }

    #[test]
    fn hash_tree_single_leaf() {
        arbtest(|u| {
            let leaves = vec![u.arbitrary::<[u8; 32]>()?.into()];
            let tree = hash_tree(leaves.clone());
            // Even a single leaf results in a two-level tree.
            assert_eq!(tree.len(), 2);
            assert_eq!(tree.last().unwrap().len(), 1);
            assert_eq!(
                tree.last().unwrap()[0],
                solana_sdk::hash::hash(leaves[0].as_ref())
            );
            // Should have the same root as the Solana implementation.
            assert_eq!(
                tree.last().unwrap()[0],
                AccountsHasher::compute_merkle_root_recurse(leaves, MERKLE_FANOUT)
            );

            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn hash_tree_three_levels() {
        arbtest(|u| {
            let leaves: Vec<solana_sdk::hash::Hash> = u
                .arbitrary::<[[u8; 32]; MERKLE_FANOUT + 1]>()?
                .into_iter()
                .map(|x| x.into())
                .collect();
            let tree = hash_tree(leaves.clone());
            assert_eq!(tree.len(), 3);
            assert_eq!(tree.last().unwrap().len(), 1);
            // Should have the same root as the Solana implementation.
            assert_eq!(
                tree.last().unwrap()[0],
                AccountsHasher::compute_merkle_root_recurse(leaves, MERKLE_FANOUT)
            );

            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn hash_tree_four_levels() {
        arbtest(|u| {
            let leaves: Vec<solana_sdk::hash::Hash> = u
                .arbitrary::<[[u8; 32]; MERKLE_FANOUT * MERKLE_FANOUT + 1]>()?
                .into_iter()
                .map(|x| x.into())
                .collect();
            let tree = hash_tree(leaves.clone());
            assert_eq!(tree.len(), 4);
            assert_eq!(tree.last().unwrap().len(), 1);
            // Should have the same root as the Solana implementation.
            assert_eq!(
                tree.last().unwrap()[0],
                AccountsHasher::compute_merkle_root_recurse(leaves, MERKLE_FANOUT)
            );

            Ok(())
        })
        .size_max(100_000_000);
    }
}
