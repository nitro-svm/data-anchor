use std::cmp::max;

use arbitrary::Arbitrary;
use solana_sdk::{account::Account, clock::Epoch};

use super::arb_keypair::ArbKeypair;

/// An arbitrary account, since we can't implement [`arbitrary::Arbitrary`] for [`solana_sdk::account::Account`].
#[derive(Debug, Arbitrary, Clone, PartialEq, Eq, Hash)]
pub struct ArbAccount {
    pub lamports: u64,
    pub data: Vec<u8>,
    pub owner: ArbKeypair,
    pub executable: bool,
    pub rent_epoch: Epoch,
}

impl From<ArbAccount> for Account {
    fn from(val: ArbAccount) -> Self {
        Account {
            lamports: max(1, val.lamports),
            data: val.data,
            owner: val.owner.pubkey(),
            executable: val.executable,
            rent_epoch: val.rent_epoch,
        }
    }
}
