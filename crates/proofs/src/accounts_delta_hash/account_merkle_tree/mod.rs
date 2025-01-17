mod builder;
mod hash_tree;
mod solana_accounts_db;
mod tree;

pub use builder::AccountMerkleTreeBuilder;
pub use hash_tree::hash_tree;
pub use solana_accounts_db::{hash_account, MERKLE_FANOUT};
use solana_sdk::{account::Account, pubkey::Pubkey};
pub use tree::{AccountMerkleTree, AccountsDeltaHashProof};

#[derive(Clone, Debug, PartialEq)]
pub enum Leaf {
    Partial(solana_sdk::hash::Hash),
    Full(Account),
}

impl Leaf {
    /// Returns the stored hash or hashes the stored account data with the given pubkey.
    pub fn hash(&self, pubkey: &Pubkey) -> solana_sdk::hash::Hash {
        match self {
            Leaf::Partial(hash) => *hash,
            Leaf::Full(account) => hash_account(account, pubkey),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, ops::Bound};

    use arbtest::arbtest;
    use itertools::Itertools;
    use solana_accounts_db::accounts_db::AccountsDb;
    use solana_sdk::{account::Account, hash::Hash, pubkey::Pubkey};

    use crate::accounts_delta_hash::{
        account_merkle_tree::AccountMerkleTree,
        exclusion::{
            inner::ExclusionInnerProof, left::ExclusionLeftProof, right::ExclusionRightProof,
            ExclusionProof,
        },
        inclusion::InclusionProof,
        testing::{generate_accounts, ArbAccount, ArbKeypair, TestAccounts},
        AccountsDeltaHashProof, Leaf,
    };

    fn assert_only_important_leaves_and_their_neighbours_are_kept_as_full(
        tree: &AccountMerkleTree,
        important_pubkeys: &BTreeSet<Pubkey>,
    ) {
        use Bound::*;
        let leaves = tree.leaves();
        for (pubkey, leaf) in leaves {
            let is_important = important_pubkeys.contains(pubkey);
            let left = leaves.range((Unbounded, Excluded(pubkey))).next_back();
            let right = leaves.range((Excluded(pubkey), Unbounded)).next();

            let is_right_of_important = match left {
                Some((left_pubkey, _)) => {
                    important_pubkeys.contains(left_pubkey)
                        || important_pubkeys
                            .iter()
                            .any(|important| left_pubkey < important && important < pubkey)
                }
                None => important_pubkeys.iter().any(|important| important < pubkey),
            };
            let is_left_of_important = match right {
                Some((right_pubkey, _)) => {
                    important_pubkeys.contains(right_pubkey)
                        || important_pubkeys
                            .iter()
                            .any(|important| pubkey < important && important < right_pubkey)
                }
                None => important_pubkeys.iter().any(|important| pubkey < important),
            };

            if is_important || is_left_of_important || is_right_of_important {
                assert!(
                    matches!(leaf, Leaf::Full(_)),
                    "expected full data for {pubkey}, important? {is_important}, left of important? {is_left_of_important}, right of important? {is_right_of_important}"
                );
            } else {
                assert!(
                    matches!(leaf, Leaf::Partial(_)),
                    "expected partial data for {pubkey}, important? {is_important}, left of important? {is_left_of_important}, right of important? {is_right_of_important}"
                );
                assert!(matches!(
                    tree.prove(*pubkey),
                    AccountsDeltaHashProof::AccountNotImportant
                ));
            }
        }
    }

    #[test]
    fn benchmark() {
        arbtest(|u| {
            let important_leaf1: (ArbKeypair, ArbAccount) = u.arbitrary()?;
            let important_leaf2: (ArbKeypair, ArbAccount) = u.arbitrary()?;
            let important_leaf3: (ArbKeypair, ArbAccount) = u.arbitrary()?;

            let important_pubkeys: BTreeSet<_> = [
                important_leaf1.0.pubkey(),
                important_leaf2.0.pubkey(),
                important_leaf3.0.pubkey(),
            ]
            .into_iter()
            .collect();
            let mut always_included_accounts = vec![important_leaf1.clone()];
            if u.arbitrary()? {
                always_included_accounts.push(important_leaf2.clone());
            }
            if u.arbitrary()? {
                always_included_accounts.push(important_leaf3.clone());
            }

            let TestAccounts {
                accounts_delta_hash,
                tree,
                ..
            } = generate_accounts(u, important_pubkeys.clone(), always_included_accounts)?;

            let proof1 = tree.prove_inclusion(important_leaf1.0.pubkey()).unwrap();
            assert!(proof1.verify(accounts_delta_hash));
            assert!(tree.get_account(important_leaf1.0.pubkey()).is_some());
            check_proof(&tree, important_leaf2, accounts_delta_hash);
            check_proof(&tree, important_leaf3, accounts_delta_hash);

            assert_only_important_leaves_and_their_neighbours_are_kept_as_full(
                &tree,
                &important_pubkeys,
            );
            let mut full = 0;
            let mut partial = 0;
            for leaf in tree.leaves().values() {
                match leaf {
                    Leaf::Partial(_) => partial += 1,
                    Leaf::Full(_) => full += 1,
                }
            }
            let total = (full + partial) as f64;
            println!(
                "total: {total: >3}, data saved: {}%",
                (100.0 * partial as f64 / total) as i32
            );
            Ok(())
        })
        .size_max(1_000_000_000);
    }

    fn check_proof(
        tree: &AccountMerkleTree,
        important_leaf: (ArbKeypair, ArbAccount),
        accounts_delta_hash: Hash,
    ) {
        let pubkey = important_leaf.0.pubkey();
        let either_proof = tree.prove(pubkey);
        if tree.leaves().contains_key(&pubkey) {
            let proof = tree.prove_inclusion(pubkey).unwrap();
            assert!(proof.verify(accounts_delta_hash));
            assert_eq!(tree.get_account(pubkey), Some(&important_leaf.1.into()));
            assert!(matches!(either_proof, AccountsDeltaHashProof::Inclusion(_)));
            assert!(tree.prove_exclusion(pubkey).is_none());
        } else {
            let proof = tree.prove_exclusion(pubkey).unwrap();
            assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
            assert!(tree.get_account(pubkey).is_none());
            assert!(matches!(either_proof, AccountsDeltaHashProof::Exclusion(_)));
            assert!(tree.prove_inclusion(pubkey).is_none());
        }
    }

    #[test]
    fn important_leaf_neighbours_are_kept_for_exclusion_proofs() {
        arbtest(|u| {
            println!("--------------------------------------------------------------");
            let important_leaf: (ArbKeypair, ArbAccount) = u.arbitrary()?;

            let important_pubkeys: BTreeSet<_> = [important_leaf.0.pubkey()].into_iter().collect();
            let TestAccounts {
                accounts_delta_hash,
                tree,
                ..
            } = generate_accounts(u, important_pubkeys.clone(), vec![])?;

            if tree.leaves().contains_key(&important_leaf.0.pubkey()) {
                // This would be an inclusion proof, which is not what we're testing.
                return Ok(());
            }

            dbg!(&important_leaf.0.pubkey(), &tree);
            let proof = tree.prove_exclusion(important_leaf.0.pubkey()).unwrap();

            assert_eq!(proof.verify(accounts_delta_hash), Ok(()));

            assert_only_important_leaves_and_their_neighbours_are_kept_as_full(
                &tree,
                &important_pubkeys,
            );
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn important_leaves_are_kept_for_inclusion_proofs() {
        arbtest(|u| {
            println!("--------------------------------------------------------------");
            let important_leaf: (ArbKeypair, ArbAccount) = u.arbitrary()?;

            let important_pubkeys: BTreeSet<_> = [important_leaf.0.pubkey()].into_iter().collect();
            let TestAccounts {
                accounts_delta_hash,
                tree,
                ..
            } = generate_accounts(u, important_pubkeys.clone(), vec![important_leaf.clone()])?;

            dbg!(&important_leaf.0.pubkey(), &tree);
            let proof = tree.prove_inclusion(important_leaf.0.pubkey()).unwrap();

            assert!(proof.verify(accounts_delta_hash));

            assert_only_important_leaves_and_their_neighbours_are_kept_as_full(
                &tree,
                &important_pubkeys,
            );
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn exclusion_empty() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let excluded: ArbKeypair = u.arbitrary()?;
            let excluded = excluded.pubkey();

            let accounts_delta_hash = solana_sdk::hash::Hasher::default().result();
            let tree = AccountMerkleTree::builder([excluded].into_iter().collect()).build();

            let proof = tree.prove_exclusion(excluded);
            let Some(ExclusionProof::ExclusionEmpty(proof)) = proof else {
                panic!("expected exclusion empty proof, got {proof:?}");
            };

            assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn exclusion_inner() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let excluded: ArbKeypair = u.arbitrary()?;
            let excluded = excluded.pubkey();

            let TestAccounts {
                accounts,
                accounts_delta_hash,
                tree,
            } = generate_accounts(u, [excluded].into_iter().collect(), vec![])?;
            let Err(right_index) = accounts.binary_search_by_key(&excluded, |(kp, _)| kp.pubkey())
            else {
                return Ok(());
            };
            let Some(left_index) = right_index.checked_sub(1) else {
                return Ok(());
            };
            let left = accounts.get(left_index).unwrap().clone();
            let Some(right) = accounts.get(right_index) else {
                return Ok(());
            };

            let proof = tree.prove_exclusion(excluded);
            let Some(ExclusionProof::ExclusionInner(proof)) = proof else {
                panic!("expected exclusion inner proof, got {proof:?}");
            };

            assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
            assert_eq!(proof.left.pubkey(), &left.0.pubkey());
            assert_eq!(proof.right.pubkey(), &right.0.pubkey());
            assert_eq!(proof.excluded, excluded);
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn exclusion_left() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let excluded: ArbKeypair = u.arbitrary()?;
            let excluded = excluded.pubkey();

            let TestAccounts {
                accounts,
                accounts_delta_hash,
                tree,
            } = generate_accounts(u, [excluded].into_iter().collect(), vec![])?;

            let Some((leftmost, _)) = &accounts.first() else {
                return Ok(());
            };
            if excluded >= leftmost.pubkey() {
                return Ok(());
            }

            let proof = tree.prove_exclusion(excluded);
            let Some(ExclusionProof::ExclusionLeft(proof)) = proof else {
                panic!("expected exclusion left proof, got {proof:?}");
            };

            assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
            assert_eq!(proof.leftmost.pubkey(), &leftmost.pubkey());
            assert_eq!(proof.excluded, excluded);
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn exclusion_right() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let excluded: ArbKeypair = u.arbitrary()?;
            let excluded = excluded.pubkey();

            let TestAccounts {
                accounts,
                accounts_delta_hash,
                tree,
            } = generate_accounts(u, [excluded].into_iter().collect(), vec![])?;

            let Some((rightmost, _)) = &accounts.last() else {
                return Ok(());
            };
            if excluded <= rightmost.pubkey() {
                return Ok(());
            }

            let proof = tree.prove_exclusion(excluded);
            let Some(ExclusionProof::ExclusionRight(proof)) = proof else {
                panic!("expected exclusion right proof, got {proof:?}");
            };

            assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
            assert_eq!(proof.rightmost.pubkey(), &rightmost.pubkey());
            assert_eq!(proof.excluded, excluded);
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn exclusion_any() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let excluded: ArbKeypair = u.arbitrary()?;
            let excluded = excluded.pubkey();

            let TestAccounts {
                accounts_delta_hash,
                tree,
                ..
            } = generate_accounts(u, [excluded].into_iter().collect(), vec![])?;

            if tree.leaves().contains_key(&excluded) {
                return Ok(());
            }

            dbg!(&excluded);
            dbg!(&tree);
            let proof = tree.prove_exclusion(excluded);
            let Some(proof) = proof else {
                panic!("expected any exclusion proof, got {proof:?}");
            };

            assert_eq!(proof.verify(accounts_delta_hash), Ok(()));
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn inclusion_multiple_accounts() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let included: (ArbKeypair, ArbAccount) = u.arbitrary()?;

            let TestAccounts {
                accounts_delta_hash,
                tree,
                ..
            } = generate_accounts(
                u,
                [included.0.pubkey()].into_iter().collect(),
                vec![included.clone()],
            )?;

            let proof = tree.prove_inclusion(included.0.pubkey()).unwrap();

            assert!(proof.verify(accounts_delta_hash));
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn inclusion_single_account() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let (keypair, account): (ArbKeypair, ArbAccount) = u.arbitrary()?;
            let account: Account = account.into();
            let mut tree = AccountMerkleTree::builder([keypair.pubkey()].into_iter().collect());
            tree.insert(keypair.pubkey(), account.clone());
            let tree = tree.build();

            let proof = tree.prove_inclusion(keypair.pubkey()).unwrap();

            assert!(proof.verify(tree.root()));
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn inclusion_when_not_included_fails() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let TestAccounts { tree, .. } = generate_accounts(u, BTreeSet::new(), vec![])?;

            let excluded: (ArbKeypair, ArbAccount) = u.arbitrary()?;
            if tree.leaves().contains_key(&excluded.0.pubkey()) {
                return Ok(());
            }

            let proof_full = tree.prove_inclusion(excluded.0.pubkey());
            assert!(proof_full.is_none());
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn exclusion_when_not_excluded_fails() {
        arbtest(move |u| {
            println!("--------------------------------------------------------------");
            let TestAccounts { accounts, tree, .. } =
                generate_accounts(u, BTreeSet::new(), vec![])?;

            let included = u.choose(&accounts)?;

            let proof = tree.prove_exclusion(included.0.pubkey());
            assert!(proof.is_none());
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn false_inclusion_full_fails() {
        arbtest(move |u| {
            let excluded: (ArbKeypair, ArbAccount) = u.arbitrary()?;
            let excluded = excluded.0.pubkey();

            let mut tree1 = AccountMerkleTree::builder([excluded].into_iter().collect());

            u.arbitrary_loop(Some(1), Some(10), |u| {
                let (keypair, account): (ArbKeypair, ArbAccount) = u.arbitrary()?;
                let account: Account = account.into();
                tree1.insert_unchecked(keypair.pubkey(), Leaf::Full(account));

                Ok(std::ops::ControlFlow::Continue(()))
            })?;
            let tree1 = tree1.build();
            let Some((to_be_replaced_pubkey, to_be_replaced_leaf)) =
                tree1.leaves().range(..excluded).next_back()
            else {
                return Ok(());
            };
            let would_be_index = tree1
                .leaves()
                .iter()
                .position(|(pubkey, _)| pubkey == to_be_replaced_pubkey)
                .unwrap();

            let Leaf::Full(to_be_replaced_account) = to_be_replaced_leaf else {
                panic!("to_be_replaced_leaf must be a full leaf");
            };

            let false_proof = InclusionProof {
                account_pubkey: excluded,
                account_data: to_be_replaced_account.clone(),
                levels: tree1.calculate_levels_for_inclusion(would_be_index),
            };

            assert!(!false_proof.verify(tree1.root()),);

            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn false_exclusion_left_fails() {
        arbtest(move |u| {
            let (included_key, included_account): (ArbKeypair, ArbAccount) = u.arbitrary()?;
            let included_key = included_key.pubkey();
            let mut tree = AccountMerkleTree::builder([included_key].into_iter().collect());

            u.arbitrary_loop(Some(2), Some(10), |u| {
                let (keypair, account): (ArbKeypair, ArbAccount) = u.arbitrary()?;
                let account: Account = account.into();
                tree.insert_unchecked(keypair.pubkey(), Leaf::Full(account));

                Ok(std::ops::ControlFlow::Continue(()))
            })?;
            tree.insert_unchecked(included_key, Leaf::Full(included_account.into()));
            let tree = tree.build();
            let false_leftmost = InclusionProof {
                account_pubkey: u.arbitrary::<ArbKeypair>()?.pubkey(),
                account_data: u.arbitrary::<ArbAccount>()?.into(),
                levels: tree.calculate_levels_for_inclusion(0),
            };

            if false_leftmost.pubkey() <= &included_key {
                return Ok(());
            }

            let false_proof = ExclusionLeftProof {
                excluded: included_key,
                leftmost: false_leftmost,
            };

            dbg!(&tree, &false_proof);

            assert_ne!(false_proof.verify(tree.root()), Ok(()));
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn false_exclusion_inner_fails() {
        arbtest(move |u| {
            let (included_key, included_account): (ArbKeypair, ArbAccount) = u.arbitrary()?;
            let included_key = included_key.pubkey();
            let mut tree = AccountMerkleTree::builder([included_key].into_iter().collect());

            u.arbitrary_loop(Some(2), Some(10), |u| {
                let (keypair, account): (ArbKeypair, ArbAccount) = u.arbitrary()?;
                let account: Account = account.into();
                tree.insert_unchecked(keypair.pubkey(), Leaf::Full(account));

                Ok(std::ops::ControlFlow::Continue(()))
            })?;
            tree.insert_unchecked(included_key, Leaf::Full(included_account.into()));
            let tree = tree.build();

            let adjacent_pair = *u.choose(
                &tree
                    .leaves()
                    .values()
                    .enumerate()
                    .tuple_windows::<(_, _)>()
                    .collect::<Vec<_>>(),
            )?;
            let ((left_index, _left_account), (right_index, _right_account)) = adjacent_pair;

            let false_left = InclusionProof {
                account_pubkey: u.arbitrary::<ArbKeypair>()?.pubkey(),
                account_data: u.arbitrary::<ArbAccount>()?.into(),
                levels: tree.calculate_levels_for_inclusion(left_index),
            };
            if false_left.pubkey() >= &included_key {
                return Ok(());
            }

            let false_right = InclusionProof {
                account_pubkey: u.arbitrary::<ArbKeypair>()?.pubkey(),
                account_data: u.arbitrary::<ArbAccount>()?.into(),
                levels: tree.calculate_levels_for_inclusion(right_index),
            };
            if false_right.pubkey() <= &included_key {
                return Ok(());
            }

            let false_proof = ExclusionInnerProof {
                left: false_left,
                excluded: included_key,
                right: false_right,
            };

            dbg!(&tree, &false_proof);

            assert_ne!(false_proof.verify(tree.root()), Ok(()));
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn false_exclusion_right_fails() {
        arbtest(move |u| {
            let (included_key, included_account): (ArbKeypair, ArbAccount) = u.arbitrary()?;
            let included_key = included_key.pubkey();
            let mut tree = AccountMerkleTree::builder([included_key].into_iter().collect());

            u.arbitrary_loop(Some(2), Some(10), |u| {
                let (keypair, account): (ArbKeypair, ArbAccount) = u.arbitrary()?;
                let account: Account = account.into();
                tree.insert_unchecked(keypair.pubkey(), Leaf::Full(account));

                Ok(std::ops::ControlFlow::Continue(()))
            })?;
            tree.insert_unchecked(included_key, Leaf::Full(included_account.into()));
            let tree = tree.build();

            let false_rightmost = InclusionProof {
                account_pubkey: u.arbitrary::<ArbKeypair>()?.pubkey(),
                account_data: u.arbitrary::<ArbAccount>()?.into(),
                levels: tree.calculate_levels_for_inclusion(tree.leaves().len() - 1),
            };

            let false_proof = ExclusionRightProof {
                rightmost: false_rightmost,
                excluded: included_key,
            };

            dbg!(&tree, &false_proof);

            assert_ne!(false_proof.verify(tree.root()), Ok(()));
            Ok(())
        })
        .size_max(100_000_000);
    }

    #[test]
    fn different_trees_have_different_roots() {
        arbtest(move |u| {
            let accounts: [(ArbKeypair, ArbAccount); 4] = u.arbitrary()?;
            let mut accounts: Vec<(Pubkey, Account)> = accounts
                .into_iter()
                .map(|(k, a)| (k.pubkey(), a.into()))
                .collect();
            accounts.sort_by_key(|(k, _)| *k);

            if BTreeSet::from_iter(accounts.iter().map(|(p, _)| p)).len() != 4 {
                // If there are duplicate pubkeys, the test is invalid.
                return Ok(());
            } else if AccountsDb::hash_account(&accounts[0].1, &accounts[0].0)
                == AccountsDb::hash_account(&accounts[1].1, &accounts[0].0)
            {
                // If the accounts happen to hash to the same value, the tree *should* be the same. Invalid test.
                return Ok(());
            }

            let important_pubkeys: BTreeSet<_> = accounts.iter().map(|(p, _)| *p).collect();

            let mut good_tree = AccountMerkleTree::builder(important_pubkeys.clone());
            good_tree.insert(accounts[1].0, accounts[1].1.clone());
            good_tree.insert(accounts[2].0, accounts[2].1.clone());
            good_tree.insert(accounts[3].0, accounts[3].1.clone());
            let good_tree = good_tree.build();

            let mut bad_tree = AccountMerkleTree::builder(important_pubkeys);
            // Note: Different account used for the pubkey.
            bad_tree.insert(accounts[1].0, accounts[0].1.clone());
            bad_tree.insert(accounts[2].0, accounts[2].1.clone());
            bad_tree.insert(accounts[3].0, accounts[3].1.clone());
            let bad_tree = bad_tree.build();

            dbg!(&accounts, &good_tree, &bad_tree);
            assert_ne!(good_tree.root(), bad_tree.root());

            Ok(())
        })
        .size_max(100_000_000);
    }
}
